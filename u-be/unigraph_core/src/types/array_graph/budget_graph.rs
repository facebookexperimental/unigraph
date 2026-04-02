// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Budget graphs: aggregated metric views of dependency graphs.
//!
//! # Overview
//!
//! A budget graph transforms a source dependency graph (e.g. JS module graph,
//! cargo crate graph) into a small summary graph where each node represents a
//! "budget" — a named aggregation of metrics computed over the transitive
//! closure of specified entry points in the source graph.
//!
//! For example, given a JS module graph with per-file `size` metrics, a budget
//! graph might contain one node per route ("homepage", "profile", "settings"),
//! each with the total JS size reachable from that route's entry point.
//!
//! # Architecture
//!
//! ```text
//!  ┌─────────────┐     ┌──────────────┐     ┌──────────────────────┐
//!  │ Source Graph │ ──▶ │ BudgetConfig │ ──▶ │ build_budget_graph() │
//!  │ (ArrayGraph) │     │  (on graph)  │     │                      │
//!  └─────────────┘     └──────────────┘     └──────────┬───────────┘
//!                                                      │
//!                                            ┌─────────▼─────────┐
//!                                            │  (source, budget)  │
//!                                            │  both ArrayGraphs  │
//!                                            └───────────────────┘
//! ```
//!
//! # Data model
//!
//! Three serializable structs define the budget system:
//!
//! - [`BudgetConfig`] — Top-level config, stored on `ArrayGraph.budget_configs`
//!   keyed by project name. Contains the algorithm choice, a map of budget
//!   definitions, and an optional traversal config.
//!
//! - [`BudgetDefinition`] — Per-budget specification: entry points into the
//!   source graph and optional freeform properties for custom algorithms.
//!
//! - [`BudgetAlgoConfig`] — Enum selecting the algorithm:
//!   - `Transitive` — Built-in DFS-based aggregation (flat sums, tiered sums,
//!     node counts).
//!   - `Custom { name }` — Dispatches to a named implementation via a runtime
//!     registry passed to [`build_budget_graph_with_custom_algos`].
//!
//! # Design decisions
//!
//! ## Config lives on the graph
//!
//! Budget configs are stored directly on `ArrayGraph.budget_configs` rather
//! than passed externally. This makes graphs self-describing: given a graph
//! blob, you know what budgets to compute without external config files.
//! The tradeoff is slightly larger serialized graphs, mitigated by
//! `skip_serializing_if = "BTreeMap::is_empty"`.
//!
//! ## Enum dispatch + trait for extensibility
//!
//! The built-in `Transitive` algorithm is a data enum variant (not a trait
//! impl). This keeps its configuration serializable without trait-object
//! gymnastics. Custom algorithms use `BudgetAlgoConfig::Custom { name }`
//! which indexes into a `BTreeMap<String, &dyn BudgetAlgo>` registry at
//! call time. This gives downstream crates full flexibility (e.g. WWW's
//! route budget algorithm can read `BudgetDefinition.properties` for
//! route-specific parameters like `comet_route_trace_policy`).
//!
//! The tradeoff is two code paths in `build_budget_graph_with_custom_algos`
//! (match arm vs. trait dispatch), but it avoids the complexity of
//! serializing trait objects or needing a global registry.
//!
//! ## Returns `(source, budget)` tuple
//!
//! Both the (possibly mutated) source graph and the budget graph are returned.
//! The source is returned because applying a `BudgetConfig.traversal_config`
//! mutates it (marks nodes unreachable), and callers often need both the
//! configured source and the resulting budget. This avoids requiring callers
//! to clone the source before calling.
//!
//! ## Traversal config on BudgetConfig
//!
//! Each `BudgetConfig` can carry its own `traversal_config`, applied to the
//! source graph before budget computation. This enables "what-if" scenarios
//! (e.g. "what would route sizes look like if we excluded package X?")
//! without mutating the original graph's traversal config.
//!
//! ## Delta strategy: replace, not diff
//!
//! `BudgetAlgoConfig` uses `#[deltable(replace)]` — the entire enum value is
//! replaced rather than field-level diffed. Fine-grained diffing across enum
//! variants adds significant complexity for minimal space savings (configs
//! are small). `BudgetConfig` and `BudgetDefinition` use standard field-level
//! diffing via `#[derive(Deltable)]`.
//!
//! ## Tiered metrics use fixed-size arrays
//!
//! The tiered aggregation uses `[f32; 4]` arrays (matching the max tier count
//! in `AscendingTiers`). Values are accumulated cumulatively: a node at tier
//! T contributes to tiers T, T+1, ..., T_max. Output metrics are named
//! `"{metric}__{tier_name}"` (e.g. `"size__T1"`, `"size__T2"`).
//!
//! # API
//!
//! Two entry points:
//!
//! - [`build_budget_graph`] — Simple API for the built-in `Transitive`
//!   algorithm. Errors if the config uses `Custom`.
//!
//! - [`build_budget_graph_with_custom_algos`] — Extended API accepting a
//!   `BTreeMap<String, &dyn BudgetAlgo>` registry for custom algorithms.
//!
//! # Examples
//!
//! ## Flat metric aggregation (cargo deps)
//!
//! ```rust,ignore
//! let config = BudgetConfig {
//!     algo: BudgetAlgoConfig::Transitive {
//!         metrics: BTreeSet::from(["loc".into()]),
//!         counts: true,
//!         tiered_metrics: BTreeSet::new(),
//!     },
//!     budgets: BTreeMap::from([(
//!         "my_crate".into(),
//!         BudgetDefinition {
//!             entry_points: BTreeSet::from(["my_crate".into()]),
//!             properties: None,
//!         },
//!     )]),
//!     traversal_config: None,
//! };
//! let (source, budget) = build_budget_graph(cargo_graph, &config)?;
//! // budget node "my_crate": loc=15000, node_count=42
//! ```
//!
//! ## Tiered metric aggregation (JS routes)
//!
//! ```rust,ignore
//! let config = BudgetConfig {
//!     algo: BudgetAlgoConfig::Transitive {
//!         metrics: BTreeSet::from(["size".into()]),
//!         counts: true,
//!         tiered_metrics: BTreeSet::from(["size".into()]),
//!     },
//!     budgets: BTreeMap::from([(
//!         "homepage".into(),
//!         BudgetDefinition {
//!             entry_points: BTreeSet::from(["routes/Homepage.js".into()]),
//!             properties: None,
//!         },
//!     )]),
//!     traversal_config: None,
//! };
//! let (source, budget) = build_budget_graph(js_graph, &config)?;
//! // budget node "homepage": size=500000, size__T1=200000, ..., node_count=150
//! ```
//!
//! ## Custom algorithm (downstream crate)
//!
//! ```rust,ignore
//! struct MyBudgetAlgo;
//! impl BudgetAlgo for MyBudgetAlgo {
//!     fn build(&self, source: &ArrayGraph, config: &BudgetConfig) -> Result<ArrayGraph> {
//!         // Custom logic using source graph + config.budgets
//!         // Can read BudgetDefinition.properties for algo-specific params
//!     }
//! }
//!
//! let registry = BTreeMap::from([
//!     ("my_algo".into(), &MyBudgetAlgo as &dyn BudgetAlgo),
//! ]);
//! // config.algo == BudgetAlgoConfig::Custom { name: "my_algo".into() }
//! let (source, budget) = build_budget_graph_with_custom_algos(
//!     graph, &config, &registry
//! )?;
//! ```

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use anyhow::Context;
use anyhow::Result;

use crate::ArrayGraph;
use crate::GraphBuilder;
use crate::TraversalConfig;
use crate::traversal::tiered_traversal::TieredTraversalConfig;
use crate::types::MetricName;
use crate::types::NodeName;

// -------------------------------------------------------------------
// Data structures
//
// All types here are serializable, deltable, and stored on the graph.
// They use `#[derive(Deltable)]` for incremental updates via the
// delta system. `BudgetAlgoConfig` uses whole-value replacement
// (`#[deltable(replace)]`) since diffing across enum variants is
// not worth the complexity. `BudgetConfig` and `BudgetDefinition`
// use standard per-field diffing.
// -------------------------------------------------------------------

/// Selects which algorithm to use for budget graph computation.
///
/// Uses `#[deltable(replace)]` — the entire enum value is replaced
/// in deltas rather than attempting per-field diffing across variants.
#[derive(
    Debug,
    serde::Serialize,
    serde::Deserialize,
    typegen::TypeGen,
    Clone,
    PartialEq,
    unigraph_delta::Deltable
)]
#[deltable(replace)]
pub enum BudgetAlgoConfig {
    /// Built-in: transitive aggregation from entry points.
    Transitive {
        /// Source metrics to sum transitively (flat totals).
        metrics: BTreeSet<MetricName>,
        /// Whether to output a "node_count" metric.
        counts: bool,
        /// Source metrics to also aggregate per-tier (cumulative).
        /// Requires the source graph to have a tiered traversal config.
        /// Produces "{metric}__{tier}" metrics in the output.
        tiered_metrics: BTreeSet<MetricName>,
    },

    /// Custom algorithm, looked up by name in the registry
    /// passed to build_budget_graph_with_custom_algos().
    Custom { name: String },
}

/// Top-level budget configuration, stored on `ArrayGraph.budget_configs`
/// keyed by project name (e.g. "CometBudget", "CargoBudget").
///
/// Contains the algorithm choice, a map of named budget definitions,
/// and an optional traversal config to apply before computing.
///
/// Stored on the graph so it survives serialization, pack/unpack, and
/// delta round-trips. This makes graphs self-describing: given a graph
/// blob, you know what budgets to compute.
#[derive(
    Debug,
    serde::Serialize,
    serde::Deserialize,
    typegen::TypeGen,
    Clone,
    PartialEq,
    unigraph_delta::Deltable
)]
pub struct BudgetConfig {
    /// Which algorithm to use and its configuration.
    pub algo: BudgetAlgoConfig,

    /// The budgets to compute. Key = budget name, value = definition.
    pub budgets: BTreeMap<String, BudgetDefinition>,

    /// Dynamic budget definitions resolved from the graph at compute time.
    /// Merged with `budgets` before computation (static budgets win conflicts).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dynamic_budget_definitions: BTreeMap<String, DynamicBudgetDefinition>,

    /// Traversal config to apply to the source graph before computing.
    /// If None, uses the source graph's existing traversal config.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub traversal_config: Option<TraversalConfig>,
}

/// A single budget within a [`BudgetConfig`].
///
/// Each budget specifies entry points in the source graph that serve
/// as DFS roots for metric aggregation. The `properties` map provides
/// freeform key-value pairs for custom algorithm parameters (e.g.
/// `"comet_route_trace_policy"` for WWW route budgets).
#[derive(
    Debug,
    serde::Serialize,
    serde::Deserialize,
    typegen::TypeGen,
    Clone,
    PartialEq,
    unigraph_delta::Deltable
)]
pub struct BudgetDefinition {
    /// Node names in the source graph that serve as DFS roots.
    pub entry_points: BTreeSet<NodeName>,

    /// Algo-specific properties per budget definition.
    /// Custom algos read what they need from here.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<BTreeMap<String, String>>,
}

/// Defines how to dynamically generate budget definitions from the graph.
///
/// Dynamic definitions are resolved at compute time by inspecting the source
/// graph, then merged with static `BudgetConfig.budgets` (static wins on
/// name conflicts).
#[derive(
    Debug,
    serde::Serialize,
    serde::Deserialize,
    typegen::TypeGen,
    Clone,
    PartialEq,
    unigraph_delta::Deltable
)]
#[deltable(replace)]
pub enum DynamicBudgetDefinition {
    /// Create one budget per entry point in the graph.
    ///
    /// Entry points are determined by [`ArrayGraph::determine_entrypoints()`]
    /// (parentless nodes, or explicit `entry_points` if set on the graph).
    /// Each budget gets the entry point's node name as its budget name and
    /// a single-element `entry_points` set.
    AllEntryPoints {},
}

// -------------------------------------------------------------------
// Trait for custom budget algorithms
//
// Downstream crates implement this trait for domain-specific budget
// computation (e.g. WWW route budgets with Comet-specific logic).
// The trait is intentionally minimal: one method, receives the
// source graph with traversal config already applied.
// -------------------------------------------------------------------

/// Extension point for custom budget algorithms.
///
/// Implement this trait in downstream crates and pass instances to
/// [`build_budget_graph_with_custom_algos`] via the registry map.
/// The source graph passed to `build()` already has the
/// `BudgetConfig.traversal_config` applied, so custom algos don't
/// need to handle that.
pub trait BudgetAlgo {
    /// Produce a budget graph from the source graph and config.
    ///
    /// The returned `ArrayGraph` should contain one node per budget
    /// (keyed by budget name) with aggregated metrics. The source
    /// graph has already had the traversal config applied.
    fn build(&self, source: &ArrayGraph, config: &BudgetConfig) -> Result<ArrayGraph>;
}

// -------------------------------------------------------------------
// Public API
//
// Two entry points: simple (built-in only) and extended (with custom
// algo registry). Both return `(source, budget)` — the source is
// returned because applying a traversal config mutates it.
// -------------------------------------------------------------------

/// Compute a budget graph using the built-in transitive algorithm.
///
/// This is the simple API for the common case. Errors if
/// `config.algo` is `BudgetAlgoConfig::Custom` (use
/// [`build_budget_graph_with_custom_algos`] instead).
///
/// Returns `(source, budget)` where `source` is the input graph
/// (possibly mutated by `config.traversal_config`) and `budget` is
/// the aggregated budget graph.
pub fn build_budget_graph(
    source: ArrayGraph,
    config: &BudgetConfig,
) -> Result<(ArrayGraph, ArrayGraph)> {
    build_budget_graph_with_custom_algos(source, config, &BTreeMap::new())
}

/// Compute a budget graph with support for custom algorithms.
///
/// Accepts a registry mapping algorithm names to trait implementations.
/// The computation proceeds in two phases:
///
/// 1. **Apply traversal config** — If `config.traversal_config` is set,
///    it is applied to the source graph (marking nodes/edges as
///    unreachable). This mutates the source graph in place.
///
/// 2. **Dispatch** — Routes to the built-in transitive algorithm or
///    looks up the named custom algorithm in the registry.
///
/// Returns `(source, budget)`. The source is returned because phase 1
/// mutates it, and callers may need both.
pub fn build_budget_graph_with_custom_algos(
    mut source: ArrayGraph,
    config: &BudgetConfig,
    custom_algos: &BTreeMap<String, &dyn BudgetAlgo>,
) -> Result<(ArrayGraph, ArrayGraph)> {
    // Phase 1: Apply traversal config if provided
    if let Some(tvc) = &config.traversal_config {
        source.apply_traversal_config(tvc.clone())?;
    }

    // Phase 2: Resolve dynamic budget definitions
    let effective_config;
    let config = if config.dynamic_budget_definitions.is_empty() {
        config
    } else {
        let mut merged = resolve_dynamic_budgets(&source, &config.dynamic_budget_definitions);
        // Static budgets win on name conflicts
        merged.extend(config.budgets.clone());
        effective_config = BudgetConfig {
            budgets: merged,
            dynamic_budget_definitions: BTreeMap::new(),
            ..config.clone()
        };
        &effective_config
    };

    // Phase 3: Dispatch to algo
    let budget_graph = match &config.algo {
        BudgetAlgoConfig::Transitive {
            metrics,
            counts,
            tiered_metrics,
        } => transitive_budget_algo(&source, config, metrics, *counts, tiered_metrics)?,
        BudgetAlgoConfig::Custom { name } => {
            let algo = custom_algos.get(name).with_context(|| {
                format!(
                    "Custom budget algo '{}' not found. Available: {:?}",
                    name,
                    custom_algos.keys().collect::<Vec<_>>()
                )
            })?;
            algo.build(&source, config)?
        }
    };

    Ok((source, budget_graph))
}

// -------------------------------------------------------------------
// Dynamic budget resolution
// -------------------------------------------------------------------

/// Resolve dynamic budget definitions into concrete [`BudgetDefinition`]s.
fn resolve_dynamic_budgets(
    source: &ArrayGraph,
    dynamic_defs: &BTreeMap<String, DynamicBudgetDefinition>,
) -> BTreeMap<String, BudgetDefinition> {
    let mut result = BTreeMap::new();
    for def in dynamic_defs.values() {
        match def {
            DynamicBudgetDefinition::AllEntryPoints {} => {
                let entry_idxs = source.determine_entrypoints();
                for idx in entry_idxs {
                    let name = source.idx_to_name(idx).to_string();
                    result.insert(
                        name.clone(),
                        BudgetDefinition {
                            entry_points: BTreeSet::from([name]),
                            properties: None,
                        },
                    );
                }
            }
        }
    }
    result
}

// -------------------------------------------------------------------
// Built-in transitive algorithm
//
// For each budget definition, performs a DFS from the entry points
// through the source graph's forward edges (respecting traversal
// config). Accumulates:
//   - Flat metrics: simple sums over all reachable nodes.
//   - Tiered metrics: cumulative per-tier sums (node at tier T
//     contributes to tiers T..T_max). Requires the source graph
//     to have a tiered traversal config.
//   - Node count: number of reachable nodes (if `counts` is true).
//
// The result is assembled into a MapGraph → ArrayGraph with one
// node per budget.
// -------------------------------------------------------------------

struct BudgetNodeResult {
    flat_metrics: BTreeMap<MetricName, f32>,
    tiered_metrics: BTreeMap<MetricName, BTreeMap<String, f32>>,
    node_count: usize,
}

fn transitive_budget_algo(
    source: &ArrayGraph,
    config: &BudgetConfig,
    flat_metric_names: &BTreeSet<MetricName>,
    counts: bool,
    tiered_metric_names: &BTreeSet<MetricName>,
) -> Result<ArrayGraph> {
    let use_tiered = !tiered_metric_names.is_empty();

    let tier_config = source
        .state
        .traversal_config
        .as_ref()
        .and_then(|tc| tc.tiered_traversal.as_ref());

    // Pre-index metric vecs for fast access
    let flat_metrics: Vec<(&MetricName, Option<&Vec<f32>>)> = flat_metric_names
        .iter()
        .map(|name| (name, source.node_metrics.get(name)))
        .collect();

    let tiered_metrics: Vec<(&MetricName, Option<&Vec<f32>>)> = tiered_metric_names
        .iter()
        .map(|name| (name, source.node_metrics.get(name)))
        .collect();

    // Compute metrics for each budget
    let mut budget_results: BTreeMap<String, BudgetNodeResult> = BTreeMap::new();

    for (budget_name, budget_def) in &config.budgets {
        let entry_idxs: Vec<_> = budget_def
            .entry_points
            .iter()
            .filter_map(|name| source.nodes.name_to_idx_log(name))
            .filter(|&idx| !source.is_node_unreachable(idx))
            .collect();

        if entry_idxs.is_empty() {
            budget_results.insert(
                budget_name.clone(),
                BudgetNodeResult {
                    flat_metrics: flat_metric_names.iter().map(|n| (n.clone(), 0.0)).collect(),
                    tiered_metrics: BTreeMap::new(),
                    node_count: 0,
                },
            );
            continue;
        }

        if use_tiered {
            if let Some(TieredTraversalConfig::AscendingTiers(ascending_tiers)) = tier_config {
                let mut flat_totals = vec![0.0f32; flat_metrics.len()];
                let mut tiered_totals: Vec<[f32; 4]> = vec![[0.0f32; 4]; tiered_metrics.len()];
                let mut node_count: usize = 0;

                for next in source
                    .edges_forward
                    .dfs_tiered_configured(&ascending_tiers.tiers, &entry_idxs)?
                {
                    let (node_idx, tier_idx) = next?;
                    node_count += 1;

                    for (i, (_name, values)) in flat_metrics.iter().enumerate() {
                        if let Some(values) = values {
                            flat_totals[i] += values[node_idx];
                        }
                    }

                    for (i, (_name, values)) in tiered_metrics.iter().enumerate() {
                        if let Some(values) = values {
                            let value = values[node_idx];
                            // Make cumulative: add to this tier and all above
                            for add_to_tier in tier_idx..4 {
                                tiered_totals[i][add_to_tier] += value;
                            }
                        }
                    }
                }

                // Build result maps
                let flat_map: BTreeMap<MetricName, f32> = flat_metrics
                    .iter()
                    .enumerate()
                    .map(|(i, (name, _))| ((*name).clone(), flat_totals[i]))
                    .collect();

                let mut tiered_map: BTreeMap<MetricName, BTreeMap<String, f32>> = BTreeMap::new();
                for (i, (name, _)) in tiered_metrics.iter().enumerate() {
                    let mut per_tier = BTreeMap::new();
                    for (tier_idx, tier) in ascending_tiers.tiers.iter().enumerate() {
                        let value = tiered_totals[i][tier_idx];
                        if value > 0.0 {
                            per_tier.insert(tier.name.clone(), value);
                        }
                    }
                    if !per_tier.is_empty() {
                        tiered_map.insert((*name).clone(), per_tier);
                    }
                }

                budget_results.insert(
                    budget_name.clone(),
                    BudgetNodeResult {
                        flat_metrics: flat_map,
                        tiered_metrics: tiered_map,
                        node_count,
                    },
                );
            } else {
                anyhow::bail!(
                    "BudgetAlgoConfig::Transitive has tiered_metrics but source graph has no tiered traversal config"
                );
            }
        } else {
            // Flat-only: use dfs_configured
            let mut flat_totals = vec![0.0f32; flat_metrics.len()];
            let mut node_count: usize = 0;

            for node_idx in source.edges_forward.dfs_configured(&entry_idxs) {
                node_count += 1;
                for (i, (_name, values)) in flat_metrics.iter().enumerate() {
                    if let Some(values) = values {
                        flat_totals[i] += values[node_idx];
                    }
                }
            }

            let flat_map: BTreeMap<MetricName, f32> = flat_metrics
                .iter()
                .enumerate()
                .map(|(i, (name, _))| ((*name).clone(), flat_totals[i]))
                .collect();

            budget_results.insert(
                budget_name.clone(),
                BudgetNodeResult {
                    flat_metrics: flat_map,
                    tiered_metrics: BTreeMap::new(),
                    node_count,
                },
            );
        }
    }

    // Assemble budget graph
    assemble_budget_graph(&budget_results, counts)
}

fn assemble_budget_graph(
    budget_results: &BTreeMap<String, BudgetNodeResult>,
    counts: bool,
) -> Result<ArrayGraph> {
    let mut builder = GraphBuilder::new();

    for budget_name in budget_results.keys() {
        builder.add_node(budget_name.clone());
    }

    let mut map_graph = builder.build();

    for (budget_name, result) in budget_results {
        let node = map_graph
            .nodes
            .get_mut(budget_name)
            .context("Missing budget node")?;

        let mut node_metrics = BTreeMap::new();

        // Flat metrics
        for (metric_name, &value) in &result.flat_metrics {
            node_metrics.insert(metric_name.clone(), value);
        }

        // Tiered metrics: "{metric}__{tier}"
        for (metric_name, tier_map) in &result.tiered_metrics {
            for (tier_name, &value) in tier_map {
                let tiered_name = format!("{}__{}", metric_name, tier_name);
                node_metrics.insert(tiered_name, value);
            }
        }

        // Node count
        if counts {
            node_metrics.insert("node_count".to_string(), result.node_count as f32);
        }

        node.metrics = Some(node_metrics);
    }

    map_graph.to_array_graph()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::collections::BTreeSet;

    use anyhow::Result;
    use k9::snapshot;

    use super::*;
    use crate::Decision;
    use crate::tests::test_graphs::make_test_array_graph_2;

    fn budget_graph_to_string(ag: &ArrayGraph) -> String {
        let mut lines = vec![];
        for node_idx in ag.node_idx_iter() {
            let name = ag.idx_to_name(node_idx);
            let mut metric_strs: Vec<String> = ag
                .node_metrics
                .iter()
                .map(|(metric_name, values)| format!("{metric_name}={}", values[node_idx]))
                .collect();
            metric_strs.sort();
            lines.push(format!("{name}: {}", metric_strs.join(", ")));
        }
        lines.join("\n")
    }

    #[test]
    fn test_budget_graph_flat_metrics() -> Result<()> {
        let ag = make_test_array_graph_2()?;

        let config = BudgetConfig {
            algo: BudgetAlgoConfig::Transitive {
                metrics: BTreeSet::from(["size".into()]),
                counts: true,
                tiered_metrics: BTreeSet::new(),
            },
            budgets: BTreeMap::from([
                (
                    "from_A".into(),
                    BudgetDefinition {
                        entry_points: BTreeSet::from(["A".into()]),
                        properties: None,
                    },
                ),
                (
                    "from_L".into(),
                    BudgetDefinition {
                        entry_points: BTreeSet::from(["L".into()]),
                        properties: None,
                    },
                ),
                (
                    "from_E".into(),
                    BudgetDefinition {
                        entry_points: BTreeSet::from(["E".into()]),
                        properties: None,
                    },
                ),
            ]),
            dynamic_budget_definitions: BTreeMap::new(),
            traversal_config: None,
        };

        let (_source, budget) = build_budget_graph(ag, &config)?;

        snapshot!(
            budget_graph_to_string(&budget),
            "
from_A: node_count=11, size=11
from_E: node_count=2, size=2
from_L: node_count=12, size=12
"
        );

        Ok(())
    }

    #[test]
    fn test_budget_graph_with_traversal_config() -> Result<()> {
        let ag = make_test_array_graph_2()?;

        let mut tvc = TraversalConfig::default();
        let force_nodes = tvc.force_nodes.get_or_insert_default();
        force_nodes.insert("F".into(), Decision::exclude());
        force_nodes.insert("J".into(), Decision::exclude());

        let config = BudgetConfig {
            algo: BudgetAlgoConfig::Transitive {
                metrics: BTreeSet::from(["size".into()]),
                counts: true,
                tiered_metrics: BTreeSet::new(),
            },
            budgets: BTreeMap::from([
                (
                    "from_A".into(),
                    BudgetDefinition {
                        entry_points: BTreeSet::from(["A".into()]),
                        properties: None,
                    },
                ),
                (
                    "from_L".into(),
                    BudgetDefinition {
                        entry_points: BTreeSet::from(["L".into()]),
                        properties: None,
                    },
                ),
            ]),
            dynamic_budget_definitions: BTreeMap::new(),
            traversal_config: Some(tvc),
        };

        let (_source, budget) = build_budget_graph(ag, &config)?;

        snapshot!(
            budget_graph_to_string(&budget),
            "
from_A: node_count=6, size=6
from_L: node_count=8, size=8
"
        );

        Ok(())
    }

    #[test]
    fn test_budget_graph_custom_algo_not_found() -> Result<()> {
        let ag = make_test_array_graph_2()?;

        let config = BudgetConfig {
            algo: BudgetAlgoConfig::Custom {
                name: "nonexistent".into(),
            },
            budgets: BTreeMap::new(),
            dynamic_budget_definitions: BTreeMap::new(),
            traversal_config: None,
        };

        let result = build_budget_graph(ag, &config);
        assert!(result.is_err());
        let err_msg = format!("{}", result.err().unwrap());
        assert!(err_msg.contains("nonexistent"));
        assert!(err_msg.contains("not found"));

        Ok(())
    }

    #[test]
    fn test_budget_graph_serialization_round_trip() -> Result<()> {
        let mut ag = make_test_array_graph_2()?;
        ag.budget_configs.insert(
            "test_project".into(),
            BudgetConfig {
                algo: BudgetAlgoConfig::Transitive {
                    metrics: BTreeSet::from(["size".into()]),
                    counts: true,
                    tiered_metrics: BTreeSet::new(),
                },
                budgets: BTreeMap::from([(
                    "budget_1".into(),
                    BudgetDefinition {
                        entry_points: BTreeSet::from(["A".into()]),
                        properties: None,
                    },
                )]),
                dynamic_budget_definitions: BTreeMap::new(),
                traversal_config: None,
            },
        );

        let serializable = ag.into_serializable();
        let json = serializable.to_json()?;
        let deserialized = crate::ArrayGraphSerializable::from_json(&json)?;
        let restored = deserialized.into_array_graph();

        assert_eq!(restored.budget_configs.len(), 1);
        let restored_config = restored.budget_configs.get("test_project").unwrap();
        assert_eq!(
            restored_config.algo,
            BudgetAlgoConfig::Transitive {
                metrics: BTreeSet::from(["size".into()]),
                counts: true,
                tiered_metrics: BTreeSet::new(),
            }
        );
        assert_eq!(restored_config.budgets.len(), 1);
        assert!(restored_config.budgets.contains_key("budget_1"));

        Ok(())
    }

    #[test]
    fn test_budget_graph_empty_entry_points() -> Result<()> {
        let ag = make_test_array_graph_2()?;

        let config = BudgetConfig {
            algo: BudgetAlgoConfig::Transitive {
                metrics: BTreeSet::from(["size".into()]),
                counts: true,
                tiered_metrics: BTreeSet::new(),
            },
            budgets: BTreeMap::from([(
                "nonexistent_root".into(),
                BudgetDefinition {
                    entry_points: BTreeSet::from(["DOES_NOT_EXIST".into()]),
                    properties: None,
                },
            )]),
            dynamic_budget_definitions: BTreeMap::new(),
            traversal_config: None,
        };

        let (_source, budget) = build_budget_graph(ag, &config)?;

        snapshot!(
            budget_graph_to_string(&budget),
            "nonexistent_root: node_count=0, size=0"
        );

        Ok(())
    }

    #[test]
    fn test_budget_graph_tiered_metrics() -> Result<()> {
        let ag = make_test_array_graph_2()?;

        let mut tvc = TraversalConfig::default();
        use crate::tests::test_utils::traversal_config_test_trait::TraversalConfigTestTrait;
        tvc.with_tier_config();

        let config = BudgetConfig {
            algo: BudgetAlgoConfig::Transitive {
                metrics: BTreeSet::from(["size".into()]),
                counts: true,
                tiered_metrics: BTreeSet::from(["size".into()]),
            },
            budgets: BTreeMap::from([(
                "from_A".into(),
                BudgetDefinition {
                    entry_points: BTreeSet::from(["A".into()]),
                    properties: None,
                },
            )]),
            dynamic_budget_definitions: BTreeMap::new(),
            traversal_config: Some(tvc),
        };

        let (_source, budget) = build_budget_graph(ag, &config)?;

        snapshot!(
            budget_graph_to_string(&budget),
            "from_A: node_count=11, size=11, size__T1=7, size__T2=9, size__T3=10, size__T4=11"
        );

        Ok(())
    }
}
