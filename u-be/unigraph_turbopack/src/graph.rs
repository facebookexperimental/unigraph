// Copyright (c) Meta Platforms, Inc. and affiliates.

/// Build a `MapGraph` from parsed Turbopack analyze data.
///
/// Combines the global module dependency graph (`modules.data`) with
/// per-route size and membership data to produce a fully configured
/// Unigraph graph with tiered traversal and metric formatting.
///
/// Module nodes carry only metrics and edges — no labels or properties.
/// Layer, path, and fragment info are encoded in the node name.
/// Route nodes carry `node_type: "route"` in properties.
/// Route membership is derivable by DFS from the `[route] /...` entry nodes.
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::VecDeque;

use anyhow::Result;
use unigraph_core::AscendingTier;
use unigraph_core::AscendingTiersConfig;
use unigraph_core::MapGraph;
use unigraph_core::TieredTraversalConfig;
use unigraph_core::TraversalConfig;
use unigraph_core::graph_settings::ArrayGraphUISettings;
use unigraph_core::graph_settings::Availability;
use unigraph_core::graph_settings::ColumnSettings;
use unigraph_core::graph_settings::GraphSettings;
use unigraph_core::graph_settings::GraphTableSort;
use unigraph_core::graph_settings::MetricConfig;
use unigraph_core::graph_settings::MetricFormat;
use unigraph_core::graph_settings::MetricsConfig;
use unigraph_core::graph_settings::SizeFormatConfig;
use unigraph_core::graph_settings::SizeInputUnits;
use unigraph_core::graph_settings::SizeOutputUnits;
use unigraph_core::graph_settings::SortColumn;
use unigraph_core::graph_settings::SortOrder;
use unigraph_core::types::map_graph::GraphNode;

use crate::Options;
use crate::analyze::RouteData;
use crate::binary_format::ModulesData;
use crate::module_ident::node_id;
use crate::module_ident::parse_ident;

const TAG_LAZY: &str = "lazy";
const TIER_EAGER: &str = "eager";
const TIER_LAZY: &str = "lazy";
const METRIC_SIZE: &str = "size";
const METRIC_COMPRESSED_SIZE: &str = "compressed_size";
const METRIC_EAGER_SIZE: &str = "eager_size";
const METRIC_EAGER_COMPRESSED_SIZE: &str = "eager_compressed_size";
const METRIC_LAZY_SIZE: &str = "lazy_size";
const METRIC_LAZY_COMPRESSED_SIZE: &str = "lazy_compressed_size";

pub fn build_map_graph(
    modules_data: &ModulesData,
    route_data: &RouteData,
    opts: &Options,
) -> Result<MapGraph> {
    let collapse_fragments = !opts.fragments;
    let parsed = parse_all_idents(modules_data, collapse_fragments);
    let mut nodes = build_nodes(modules_data, route_data, &parsed, opts);
    deduplicate_edges(&mut nodes);
    let (route_nodes, entry_points) =
        build_route_nodes(modules_data, route_data, &parsed.node_ids, &nodes);
    nodes.extend(route_nodes);
    compute_route_metrics(&mut nodes);

    Ok(MapGraph {
        nodes,
        traversal_config: Some(traversal_config()),
        graph_settings: Some(graph_settings()),
        entry_points: Some(entry_points),
        properties: BTreeMap::new(),
    })
}

// ── Parsed module data ─────────────────────────────────────────────────────

/// Pre-computed node IDs and layers for all modules.
struct ParsedModules {
    node_ids: Vec<String>,
    layers: Vec<Option<String>>,
}

fn parse_all_idents(modules_data: &ModulesData, collapse_fragments: bool) -> ParsedModules {
    let mut node_ids = Vec::with_capacity(modules_data.header.modules.len());
    let mut layers = Vec::with_capacity(modules_data.header.modules.len());
    for m in &modules_data.header.modules {
        let parsed = parse_ident(&m.ident);
        node_ids.push(node_id(&parsed, collapse_fragments));
        layers.push(parsed.layer);
    }
    ParsedModules { node_ids, layers }
}

// ── Node construction ───────────────────────────────────────────────────────

const LAYER_CLIENT: &str = "app-client";

fn build_nodes(
    modules_data: &ModulesData,
    route_data: &RouteData,
    parsed: &ParsedModules,
    opts: &Options,
) -> BTreeMap<String, GraphNode> {
    let mut nodes: BTreeMap<String, GraphNode> = BTreeMap::new();

    for (i, module) in modules_data.header.modules.iter().enumerate() {
        let id = &parsed.node_ids[i];

        let directed = resolve_edges(
            modules_data,
            &modules_data.header.module_dependencies,
            i,
            &parsed.node_ids,
        );
        let async_deps = resolve_edges(
            modules_data,
            &modules_data.header.async_module_dependencies,
            i,
            &parsed.node_ids,
        );

        let include_size = opts.all_layer_sizes || is_client_layer(&parsed.layers[i]);
        let metrics = if include_size {
            build_metrics(route_data, &module.path)
        } else {
            BTreeMap::new()
        };

        if let Some(existing) = nodes.get_mut(id) {
            merge_into_existing(existing, directed, async_deps, metrics);
        } else {
            let mut tagged = BTreeMap::new();
            if !async_deps.is_empty() {
                tagged.insert(TAG_LAZY.to_string(), async_deps);
            }

            nodes.insert(
                id.clone(),
                GraphNode {
                    properties: None,
                    labels: None,
                    metrics: if metrics.is_empty() {
                        None
                    } else {
                        Some(metrics)
                    },
                    edges_directed: if directed.is_empty() {
                        None
                    } else {
                        Some(directed)
                    },
                    edges_tagged: if tagged.is_empty() {
                        None
                    } else {
                        Some(tagged)
                    },
                    edges_dynamic: None,
                },
            );
        }
    }

    nodes
}

fn resolve_edges(
    modules_data: &ModulesData,
    reference: &crate::binary_format::EdgesDataReference,
    index: usize,
    node_ids: &[String],
) -> BTreeSet<String> {
    modules_data
        .edges_for(reference, index)
        .iter()
        .map(|&j| node_ids[j as usize].clone())
        .collect()
}

fn build_metrics(route_data: &RouteData, module_path: &str) -> BTreeMap<String, f32> {
    let mut metrics = BTreeMap::new();
    if let Some(size) = route_data.sizes.get(module_path) {
        metrics.insert(METRIC_SIZE.to_string(), size.size as f32);
        metrics.insert(
            METRIC_COMPRESSED_SIZE.to_string(),
            size.compressed_size as f32,
        );
    }
    metrics
}

/// Returns true for layers whose code ships to the browser.
/// Layerless nodes (static assets, CSS) are also included.
fn is_client_layer(layer: &Option<String>) -> bool {
    match layer {
        None => true,
        Some(l) => l == LAYER_CLIENT,
    }
}

// ── Merging (for fragment collapse) ─────────────────────────────────────────

fn merge_into_existing(
    existing: &mut GraphNode,
    directed: BTreeSet<String>,
    async_deps: BTreeSet<String>,
    metrics: BTreeMap<String, f32>,
) {
    // Union edges.
    if !directed.is_empty() {
        existing
            .edges_directed
            .get_or_insert_with(BTreeSet::new)
            .extend(directed);
    }
    if !async_deps.is_empty() {
        existing
            .edges_tagged
            .get_or_insert_with(BTreeMap::new)
            .entry(TAG_LAZY.to_string())
            .or_default()
            .extend(async_deps);
    }

    // Sum metrics.
    for (key, value) in metrics {
        *existing
            .metrics
            .get_or_insert_with(BTreeMap::new)
            .entry(key)
            .or_insert(0.0) += value;
    }
}

// ── Remove self-edges ───────────────────────────────────────────────────────

/// After fragment collapse, a node may have edges pointing to itself. Remove them.
fn deduplicate_edges(nodes: &mut BTreeMap<String, GraphNode>) {
    for (name, node) in nodes.iter_mut() {
        if let Some(directed) = &mut node.edges_directed {
            directed.remove(name);
        }
        if let Some(tagged) = &mut node.edges_tagged {
            for targets in tagged.values_mut() {
                targets.remove(name);
            }
            tagged.retain(|_, targets| !targets.is_empty());
        }
    }
}

// ── Route nodes ────────────────────────────────────────────────────────────

/// Create synthetic `[route] /path` nodes that serve as entry points into each
/// route's subgraph. Each route node has directed edges to the global entry
/// points (modules with no incoming edges) that belong to that route.
///
/// Entry points are assigned to routes in two passes:
///   1. Directly, if the module appears in the route's `route_membership`.
///   2. Indirectly, if the module's direct neighbors belong to a route (catches
///      framework virtual entry modules like `app-page.js?page=...`).
///
/// Returns the route nodes and the full set of entry points (route nodes +
/// any global entry points for modules not belonging to any route).
fn build_route_nodes(
    modules_data: &ModulesData,
    route_data: &RouteData,
    node_ids: &[String],
    nodes: &BTreeMap<String, GraphNode>,
) -> (BTreeMap<String, GraphNode>, BTreeSet<String>) {
    let global_entries = find_entry_points(nodes);
    let path_to_node_ids = build_path_to_node_ids(modules_data, node_ids);
    let routes = invert_route_membership(&route_data.route_membership);

    let mut route_nodes = BTreeMap::new();
    let mut entry_points = BTreeSet::new();
    let mut claimed_entries: BTreeSet<String> = BTreeSet::new();

    // Pass 1: assign entry points that are directly in route_membership.
    for (route, module_paths) in &routes {
        let route_node_ids: BTreeSet<String> = module_paths
            .iter()
            .filter_map(|p| path_to_node_ids.get(p.as_str()))
            .flatten()
            .cloned()
            .collect();

        let roots: BTreeSet<String> = global_entries
            .intersection(&route_node_ids)
            .cloned()
            .collect();

        claimed_entries.extend(roots.iter().cloned());

        let name = format!("[route] {}", route);
        route_nodes.insert(
            name.clone(),
            GraphNode {
                properties: Some(BTreeMap::from([(
                    "node_type".to_string(),
                    "route".to_string(),
                )])),
                labels: None,
                metrics: None,
                edges_directed: if roots.is_empty() { None } else { Some(roots) },
                edges_tagged: None,
                edges_dynamic: None,
            },
        );
        entry_points.insert(name);
    }

    // Pass 2: assign unclaimed entry points to routes by checking their
    // neighbors' route membership (catches framework virtual modules).
    assign_by_neighbor_routes(
        &global_entries,
        &claimed_entries,
        nodes,
        &path_to_node_ids,
        &routes,
        &mut route_nodes,
    );

    // Preserve global entry points not claimed by any route.
    let all_claimed: BTreeSet<String> = route_nodes
        .values()
        .flat_map(|n| n.edges_directed.iter().flatten())
        .cloned()
        .collect();
    for ep in &global_entries {
        if !all_claimed.contains(ep) {
            entry_points.insert(ep.clone());
        }
    }

    (route_nodes, entry_points)
}

/// For each unclaimed global entry point, infer its route by intersecting
/// the route membership of its direct edge targets. If the intersection is a
/// single route (or small set), assign the entry point to those routes.
///
/// This handles Next.js virtual entry modules like `app-page.js?page=...`
/// whose targets include a route-specific page.tsx (unique to one route)
/// plus shared layouts (in many routes). The intersection isolates the
/// specific route.
fn assign_by_neighbor_routes(
    global_entries: &BTreeSet<String>,
    claimed: &BTreeSet<String>,
    nodes: &BTreeMap<String, GraphNode>,
    path_to_node_ids: &BTreeMap<&str, BTreeSet<String>>,
    routes: &BTreeMap<String, BTreeSet<String>>,
    route_nodes: &mut BTreeMap<String, GraphNode>,
) {
    // Build a reverse lookup: node_id → set of routes it belongs to.
    let node_id_to_routes = build_node_id_to_routes(path_to_node_ids, routes);

    for ep in global_entries {
        if claimed.contains(ep) {
            continue;
        }
        let Some(node) = nodes.get(ep) else {
            continue;
        };

        let targets: Vec<&str> = node
            .edges_directed
            .iter()
            .flatten()
            .chain(node.edges_tagged.iter().flat_map(|t| t.values().flatten()))
            .map(|s| s.as_str())
            .collect();

        if targets.is_empty() {
            continue;
        }

        // Intersect route membership across targets that belong to routes.
        // Skip targets with no route membership (externals, assets) — they
        // carry no route information and would zero out the intersection.
        let mut ep_routes: Option<BTreeSet<String>> = None;
        for target in &targets {
            let Some(target_routes) = node_id_to_routes.get(*target) else {
                continue;
            };
            if target_routes.is_empty() {
                continue;
            }

            ep_routes = Some(match ep_routes {
                None => target_routes.clone(),
                Some(acc) => acc.intersection(target_routes).cloned().collect(),
            });
        }

        let Some(inferred_routes) = ep_routes else {
            continue;
        };

        for route in &inferred_routes {
            let route_name = format!("[route] {}", route);
            if let Some(route_node) = route_nodes.get_mut(&route_name) {
                route_node
                    .edges_directed
                    .get_or_insert_with(BTreeSet::new)
                    .insert(ep.clone());
            }
        }
    }
}

/// Build a map from node_id → set of routes it belongs to,
/// by joining path_to_node_ids with the inverted route membership.
fn build_node_id_to_routes(
    path_to_node_ids: &BTreeMap<&str, BTreeSet<String>>,
    routes: &BTreeMap<String, BTreeSet<String>>,
) -> BTreeMap<String, BTreeSet<String>> {
    let mut result: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (route, module_paths) in routes {
        for module_path in module_paths {
            if let Some(node_ids) = path_to_node_ids.get(module_path.as_str()) {
                for node_id in node_ids {
                    result
                        .entry(node_id.clone())
                        .or_default()
                        .insert(route.clone());
                }
            }
        }
    }
    result
}

/// Invert route_membership (module_path → routes) into (route → module_paths).
fn invert_route_membership(
    membership: &std::collections::HashMap<String, BTreeSet<String>>,
) -> BTreeMap<String, BTreeSet<String>> {
    let mut routes: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (path, route_set) in membership {
        for route in route_set {
            routes
                .entry(route.clone())
                .or_default()
                .insert(path.clone());
        }
    }
    routes
}

/// Map each module path to the set of node IDs it contributes to
/// (multiple when fragments are not collapsed).
fn build_path_to_node_ids<'a>(
    modules_data: &'a ModulesData,
    node_ids: &[String],
) -> BTreeMap<&'a str, BTreeSet<String>> {
    let mut map: BTreeMap<&'a str, BTreeSet<String>> = BTreeMap::new();
    for (i, module) in modules_data.header.modules.iter().enumerate() {
        map.entry(module.path.as_str())
            .or_default()
            .insert(node_ids[i].clone());
    }
    map
}

// ── Precomputed route metrics ───────────────────────────────────────────────

/// For each route node, BFS its subgraph and sum tiered sizes.
///
/// Eager = reachable via sync edges only.
/// Lazy  = additionally reachable via async (lazy-tagged) edges.
fn compute_route_metrics(nodes: &mut BTreeMap<String, GraphNode>) {
    let route_names: Vec<String> = nodes
        .iter()
        .filter(|(_, node)| is_route_node(node))
        .map(|(name, _)| name.clone())
        .collect();

    for route_name in &route_names {
        let (eager, lazy) = tiered_sizes(nodes, route_name);
        if let Some(route_node) = nodes.get_mut(route_name) {
            let metrics = route_node.metrics.get_or_insert_with(BTreeMap::new);
            metrics.insert(METRIC_EAGER_SIZE.to_string(), eager.size);
            metrics.insert(METRIC_EAGER_COMPRESSED_SIZE.to_string(), eager.compressed);
            metrics.insert(METRIC_LAZY_SIZE.to_string(), lazy.size);
            metrics.insert(METRIC_LAZY_COMPRESSED_SIZE.to_string(), lazy.compressed);
        }
    }
}

fn is_route_node(node: &GraphNode) -> bool {
    node.properties
        .as_ref()
        .and_then(|p| p.get("node_type"))
        .is_some_and(|v| v == "route")
}

#[derive(Default)]
struct TierSizes {
    size: f32,
    compressed: f32,
}

/// BFS from a route node and compute per-tier size totals.
///
/// Pass 1: follow only sync edges → eager tier.
/// Pass 2: follow all edges from eager set → new nodes are lazy tier.
fn tiered_sizes(nodes: &BTreeMap<String, GraphNode>, root: &str) -> (TierSizes, TierSizes) {
    let seeds: Vec<&str> = nodes
        .get(root)
        .and_then(|n| n.edges_directed.as_ref())
        .map(|edges| edges.iter().map(|s| s.as_str()).collect())
        .unwrap_or_default();

    let eager_set = bfs_sync_only(nodes, &seeds);

    let all_set = bfs_all_edges(nodes, &seeds);

    let mut eager = TierSizes::default();
    let mut lazy = TierSizes::default();

    for name in &all_set {
        let Some(node) = nodes.get(name.as_str()) else {
            continue;
        };
        let Some(metrics) = &node.metrics else {
            continue;
        };
        let s = metrics.get(METRIC_SIZE).copied().unwrap_or(0.0);
        let c = metrics.get(METRIC_COMPRESSED_SIZE).copied().unwrap_or(0.0);

        if eager_set.contains(name) {
            eager.size += s;
            eager.compressed += c;
        } else {
            lazy.size += s;
            lazy.compressed += c;
        }
    }

    (eager, lazy)
}

/// BFS following only sync (directed) edges.
fn bfs_sync_only(nodes: &BTreeMap<String, GraphNode>, seeds: &[&str]) -> BTreeSet<String> {
    let mut visited = BTreeSet::new();
    let mut queue: VecDeque<&str> = VecDeque::new();

    for &seed in seeds {
        if visited.insert(seed.to_string()) {
            queue.push_back(seed);
        }
    }

    while let Some(current) = queue.pop_front() {
        let Some(node) = nodes.get(current) else {
            continue;
        };
        if let Some(edges) = &node.edges_directed {
            for target in edges {
                if visited.insert(target.clone()) {
                    queue.push_back(target);
                }
            }
        }
    }

    visited
}

/// BFS following both sync and lazy edges.
fn bfs_all_edges(nodes: &BTreeMap<String, GraphNode>, seeds: &[&str]) -> BTreeSet<String> {
    let mut visited = BTreeSet::new();
    let mut queue: VecDeque<&str> = VecDeque::new();

    for &seed in seeds {
        if visited.insert(seed.to_string()) {
            queue.push_back(seed);
        }
    }

    while let Some(current) = queue.pop_front() {
        let Some(node) = nodes.get(current) else {
            continue;
        };
        if let Some(edges) = &node.edges_directed {
            for target in edges {
                if visited.insert(target.clone()) {
                    queue.push_back(target);
                }
            }
        }
        if let Some(tagged) = &node.edges_tagged {
            for targets in tagged.values() {
                for target in targets {
                    if visited.insert(target.clone()) {
                        queue.push_back(target);
                    }
                }
            }
        }
    }

    visited
}

// ── Entry points ────────────────────────────────────────────────────────────

/// Find entry points: nodes with no incoming edges in the built graph.
///
/// Computed from the graph's actual edges rather than the binary
/// `module_dependents` section, which can be out of sync after
/// fragment collapse.
fn find_entry_points(nodes: &BTreeMap<String, GraphNode>) -> BTreeSet<String> {
    let mut has_incoming: BTreeSet<&str> = BTreeSet::new();
    for node in nodes.values() {
        if let Some(edges) = &node.edges_directed {
            for target in edges {
                has_incoming.insert(target.as_str());
            }
        }
        if let Some(tagged) = &node.edges_tagged {
            for targets in tagged.values() {
                for target in targets {
                    has_incoming.insert(target.as_str());
                }
            }
        }
    }

    nodes
        .keys()
        .filter(|name| !has_incoming.contains(name.as_str()))
        .cloned()
        .collect()
}

// ── Traversal config ────────────────────────────────────────────────────────

fn traversal_config() -> TraversalConfig {
    TraversalConfig {
        tiered_traversal: Some(TieredTraversalConfig::AscendingTiers(
            AscendingTiersConfig {
                tiers: vec![
                    AscendingTier {
                        name: TIER_EAGER.to_string(),
                        tags_that_transition_to_this_tier: vec![],
                    },
                    AscendingTier {
                        name: TIER_LAZY.to_string(),
                        tags_that_transition_to_this_tier: vec![TAG_LAZY.to_string()],
                    },
                ],
                max_tier: None,
            },
        )),
        ..Default::default()
    }
}

// ── Graph settings ──────────────────────────────────────────────────────────

fn graph_settings() -> GraphSettings {
    let size_format = MetricFormat::Size(SizeFormatConfig {
        input_units: SizeInputUnits::Bytes,
        output_units: SizeOutputUnits::VariableUnits,
        min_precision: None,
        max_precision: Some(2),
        use_delimiter: None,
    });

    let unavailable = Some(Availability::Unavailable);

    let mut metrics = BTreeMap::new();

    metrics.insert(
        METRIC_SIZE.to_string(),
        MetricConfig {
            self_view: unavailable,
            transitive: unavailable,
            dominated: unavailable,
            tiered: unavailable,
            tiered_dominated: unavailable,
            format: Some(size_format.clone()),
            description: Some("Source-map attributed module size in output bundles".to_string()),
        },
    );

    metrics.insert(
        METRIC_COMPRESSED_SIZE.to_string(),
        MetricConfig {
            self_view: unavailable,
            transitive: unavailable,
            dominated: unavailable,
            tiered: None,
            tiered_dominated: None,
            format: Some(size_format.clone()),
            description: Some("Estimated compressed (gzip) size".to_string()),
        },
    );

    let hidden = MetricConfig {
        self_view: unavailable,
        transitive: unavailable,
        dominated: unavailable,
        tiered: unavailable,
        tiered_dominated: unavailable,
        format: Some(size_format),
        description: None,
    };
    metrics.insert(METRIC_EAGER_SIZE.to_string(), hidden.clone());
    metrics.insert(METRIC_EAGER_COMPRESSED_SIZE.to_string(), hidden.clone());
    metrics.insert(METRIC_LAZY_SIZE.to_string(), hidden.clone());
    metrics.insert(METRIC_LAZY_COMPRESSED_SIZE.to_string(), hidden);

    GraphSettings {
        description: Some(
            "Next.js Turbopack module dependency graph from `next experimental-analyze`"
                .to_string(),
        ),
        metrics_config: Some(MetricsConfig {
            default_availability: None,
            default_visibility: None,
            metrics: Some(metrics),
            parents_count: None,
            count_transitive: None,
            count_dominated: None,
        }),
        metrics_visibility: None,
        ui_settings: Some(ArrayGraphUISettings {
            columns: Some(ColumnSettings {
                show_tiered_metrics: Some(true),
                hide_metrics: Some(false),
                graph_table_sort: Some(GraphTableSort {
                    column: SortColumn::MetricView {
                        key: format!("{METRIC_COMPRESSED_SIZE}#{TIER_EAGER}"),
                    },
                    order: SortOrder::Desc,
                }),
                show_counts: None,
                show_tier_column: Some(true),
                hide_dominated_tiered_metrics: None,
            }),
            ..Default::default()
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::analyze::ModuleSize;
    use crate::binary_format::AnalyzeModule;
    use crate::binary_format::EdgesDataReference;
    use crate::binary_format::ModulesDataHeader;

    /// Build a synthetic `ModulesData` with the given modules and edges.
    ///
    /// `sync_edges` and `async_edges`: adjacency lists per module index.
    /// `dependents`: who depends on each module (reverse of sync_edges).
    fn make_modules_data(
        modules: Vec<(&str, &str)>,
        sync_edges: &[&[u32]],
        async_edges: &[&[u32]],
    ) -> ModulesData {
        let n = modules.len();
        let header_modules: Vec<AnalyzeModule> = modules
            .iter()
            .map(|(ident, path)| AnalyzeModule {
                ident: ident.to_string(),
                path: path.to_string(),
            })
            .collect();

        // Compute dependents (reverse of sync_edges).
        let mut dependents: Vec<Vec<u32>> = vec![vec![]; n];
        for (i, edges) in sync_edges.iter().enumerate() {
            for &target in *edges {
                dependents[target as usize].push(i as u32);
            }
        }

        let mut binary = Vec::new();

        let deps_ref = encode_edges(sync_edges, &mut binary);
        let async_deps_ref = encode_edges(async_edges, &mut binary);

        let dependents_slices: Vec<&[u32]> = dependents.iter().map(|v| v.as_slice()).collect();
        let dependents_ref = encode_edges(&dependents_slices, &mut binary);

        let empty: Vec<&[u32]> = vec![&[]; n];
        let async_dependents_ref = encode_edges(&empty, &mut binary);

        ModulesData {
            header: ModulesDataHeader {
                modules: header_modules,
                module_dependencies: deps_ref,
                async_module_dependencies: async_deps_ref,
                module_dependents: dependents_ref,
                async_module_dependents: async_dependents_ref,
            },
            binary,
        }
    }

    fn encode_edges(edges: &[&[u32]], binary: &mut Vec<u8>) -> EdgesDataReference {
        let offset = binary.len() as u32;
        let n = edges.len();

        // num_nodes
        binary.extend_from_slice(&(n as u32).to_be_bytes());

        // Cumulative offsets
        let mut cumulative = 0u32;
        for adj in edges {
            cumulative += adj.len() as u32;
            binary.extend_from_slice(&cumulative.to_be_bytes());
        }

        // Targets
        for adj in edges {
            for &target in *adj {
                binary.extend_from_slice(&target.to_be_bytes());
            }
        }

        let length = binary.len() as u32 - offset;
        EdgesDataReference { offset, length }
    }

    fn make_route_data(sizes: Vec<(&str, u64, u64)>, routes: Vec<(&str, Vec<&str>)>) -> RouteData {
        let mut size_map = HashMap::new();
        for (path, size, compressed) in &sizes {
            size_map.insert(
                path.to_string(),
                ModuleSize {
                    size: *size,
                    compressed_size: *compressed,
                },
            );
        }

        let mut route_membership = HashMap::new();
        for (route, paths) in &routes {
            for path in paths {
                route_membership
                    .entry(path.to_string())
                    .or_insert_with(BTreeSet::new)
                    .insert(route.to_string());
            }
        }

        RouteData {
            sizes: size_map,
            route_membership,
        }
    }

    /// Format a MapGraph into a compact, human-readable snapshot string.
    fn format_graph(graph: &MapGraph) -> String {
        let mut lines = Vec::new();
        for (name, node) in &graph.nodes {
            lines.push(format!("NODE: {name}"));

            if let Some(metrics) = &node.metrics {
                for (k, v) in metrics {
                    lines.push(format!("  metric {k} = {v}"));
                }
            }
            if let Some(directed) = &node.edges_directed {
                for target in directed {
                    lines.push(format!("  -> {target}"));
                }
            }
            if let Some(tagged) = &node.edges_tagged {
                for (tag, targets) in tagged {
                    for target in targets {
                        lines.push(format!("  -[{tag}]-> {target}"));
                    }
                }
            }
        }

        if let Some(entry_points) = &graph.entry_points {
            lines.push(String::new());
            lines.push("ENTRY POINTS:".to_string());
            for ep in entry_points {
                lines.push(format!("  {ep}"));
            }
        }

        lines.join("\n")
    }

    #[test]
    fn test_basic_graph() {
        // Module graph:
        //   page.tsx [app-rsc] -> layout.tsx [app-rsc]  (actually layout depends on nothing,
        //                                                page depends on layout + utils)
        //   page.tsx [app-rsc] -> utils.ts [app-rsc]
        //   page.tsx [app-rsc] -async-> chart.tsx [app-client]
        let modules_data = make_modules_data(
            vec![
                (
                    "[project]/src/app/page.tsx [app-rsc] (ecmascript)",
                    "[project]/src/app/page.tsx",
                ),
                (
                    "[project]/src/app/layout.tsx [app-rsc] (ecmascript)",
                    "[project]/src/app/layout.tsx",
                ),
                (
                    "[project]/src/utils.ts [app-rsc] (ecmascript)",
                    "[project]/src/utils.ts",
                ),
                (
                    "[project]/src/chart.tsx [app-client] (ecmascript)",
                    "[project]/src/chart.tsx",
                ),
            ],
            &[&[1, 2], &[], &[], &[]], // sync deps
            &[&[3], &[], &[], &[]],    // async deps
        );

        let route_data = make_route_data(
            vec![
                ("[project]/src/app/page.tsx", 1000, 400),
                ("[project]/src/app/layout.tsx", 500, 200),
                ("[project]/src/utils.ts", 300, 100),
                ("[project]/src/chart.tsx", 2000, 800),
            ],
            vec![
                (
                    "/",
                    vec![
                        "[project]/src/app/page.tsx",
                        "[project]/src/app/layout.tsx",
                        "[project]/src/utils.ts",
                    ],
                ),
                (
                    "/dashboard",
                    vec!["[project]/src/app/page.tsx", "[project]/src/chart.tsx"],
                ),
            ],
        );

        let opts = Options {
            fragments: false,
            all_layer_sizes: false,
        };
        let graph = build_map_graph(&modules_data, &route_data, &opts).unwrap();

        let snapshot = format_graph(&graph);
        k9::snapshot!(
            snapshot,
            "
NODE: [project]/src/app/layout.tsx [app-rsc]
NODE: [project]/src/app/page.tsx [app-rsc]
  -> [project]/src/app/layout.tsx [app-rsc]
  -> [project]/src/utils.ts [app-rsc]
  -[lazy]-> [project]/src/chart.tsx [app-client]
NODE: [project]/src/chart.tsx [app-client]
  metric compressed_size = 800
  metric size = 2000
NODE: [project]/src/utils.ts [app-rsc]
NODE: [route] /
  metric eager_compressed_size = 0
  metric eager_size = 0
  metric lazy_compressed_size = 800
  metric lazy_size = 2000
  -> [project]/src/app/page.tsx [app-rsc]
NODE: [route] /dashboard
  metric eager_compressed_size = 0
  metric eager_size = 0
  metric lazy_compressed_size = 800
  metric lazy_size = 2000
  -> [project]/src/app/page.tsx [app-rsc]

ENTRY POINTS:
  [route] /
  [route] /dashboard
"
        );
    }

    #[test]
    fn test_fragment_collapse() {
        // Two fragments of the same module should merge when collapse_fragments=true.
        let modules_data = make_modules_data(
            vec![
                (
                    "[project]/src/utils.ts [app-rsc] (ecmascript) <exports>",
                    "[project]/src/utils.ts",
                ),
                (
                    "[project]/src/utils.ts [app-rsc] (ecmascript) <module evaluation>",
                    "[project]/src/utils.ts",
                ),
                (
                    "[project]/src/app.tsx [app-rsc] (ecmascript)",
                    "[project]/src/app.tsx",
                ),
            ],
            &[&[], &[], &[0, 1]], // app.tsx depends on both fragments
            &[&[], &[], &[]],
        );

        let route_data = make_route_data(vec![("[project]/src/utils.ts", 500, 200)], vec![]);

        // Collapsed (default) — app-rsc nodes get no sizes by default
        let opts = Options {
            fragments: false,
            all_layer_sizes: true,
        };
        let graph = build_map_graph(&modules_data, &route_data, &opts).unwrap();
        // Both fragments merge into one node
        assert_eq!(graph.nodes.len(), 2); // utils.ts + app.tsx
        let utils = &graph.nodes["[project]/src/utils.ts [app-rsc]"];
        // Size should be summed (500 + 500 = 1000 from two fragment entries
        // pointing to the same path — but actually both share the same path
        // so the size lookup happens twice for the same key, giving 500 + 500 = 1000)
        assert_eq!(utils.metrics.as_ref().unwrap()["size"], 1000.0);

        // Expanded
        let opts = Options {
            fragments: true,
            all_layer_sizes: false,
        };
        let graph = build_map_graph(&modules_data, &route_data, &opts).unwrap();
        assert_eq!(graph.nodes.len(), 3); // two fragments + app.tsx
        assert!(
            graph
                .nodes
                .contains_key("[project]/src/utils.ts [app-rsc] <exports>")
        );
        assert!(
            graph
                .nodes
                .contains_key("[project]/src/utils.ts [app-rsc] <module evaluation>")
        );
    }

    #[test]
    fn test_self_edge_removal_on_collapse() {
        // When fragments collapse, edges between fragments of the same module
        // become self-edges and should be removed.
        let modules_data = make_modules_data(
            vec![
                (
                    "[project]/src/utils.ts [app-rsc] (ecmascript) <exports>",
                    "[project]/src/utils.ts",
                ),
                (
                    "[project]/src/utils.ts [app-rsc] (ecmascript) <module evaluation>",
                    "[project]/src/utils.ts",
                ),
            ],
            &[&[1], &[0]], // fragments reference each other
            &[&[], &[]],
        );

        let route_data = make_route_data(vec![], vec![]);
        let opts = Options {
            fragments: false,
            all_layer_sizes: false,
        };
        let graph = build_map_graph(&modules_data, &route_data, &opts).unwrap();

        assert_eq!(graph.nodes.len(), 1);
        let utils = &graph.nodes["[project]/src/utils.ts [app-rsc]"];
        // Self-edges should have been removed
        assert!(
            utils.edges_directed.is_none() || utils.edges_directed.as_ref().unwrap().is_empty()
        );
    }
}
