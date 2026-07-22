// Copyright (c) Meta Platforms, Inc. and affiliates.

#![allow(clippy::collapsible_if)]
mod console_reporter;
mod wasm_error;

use std::str::FromStr;
use std::sync::Arc;
use std::vec;

use anyhow::Context;
use anyhow::Result;
use log::trace;
use unigraph_core::ArrayGraphSerializable;
use unigraph_core::ArrayGraphSerializablePackage;
use unigraph_core::ArrayGraphSerializablePackageBase64;
use unigraph_core::EdgeOverrides;
use unigraph_core::ExportFormat;
use unigraph_core::ExportScope;
use unigraph_core::GraphQueryConfig;
use unigraph_core::MapGraph;
use unigraph_core::MinCutResult;
use unigraph_core::TraversalConfig;
use unigraph_core::TraversalType;
use unigraph_core::TwinGraph;
use unigraph_core::export_graph_bytes;
use unigraph_core::graph_settings::GraphStructure;
use unigraph_core::types::NodeIDX;
use unigraph_core::ui_types::ExplorerComponentInputGraph;
use unigraph_core::ui_types::ExplorerComponentInputGraphs;
use unigraph_delta::Deltable;
use unigraph_graph_state::GlobalGraphState;
use unigraph_graph_state::global_graph_state;
use unigraph_graph_state::graph_state::GraphMode;
use unigraph_graph_state::types::SimulationParams;
use unigraph_serialization::SerializationFormat;
use unigraph_wgpu::GlobalState;
use unigraph_wgpu::UserEvent;
use unigraph_wgpu::WindowAttributesFactory;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::wasm_bindgen;
#[allow(unused_imports)]
use wasm_bindgen::prelude::*;
use wasm_error::WasmJSError;
use winit::event_loop::EventLoop;
use winit::window::Window;
use winit::window::WindowAttributes;

const ENABLE_LL: bool = false;

#[allow(dead_code)]
fn get_canvas() -> Result<Option<web_sys::HtmlCanvasElement>> {
    web_sys::window()
        .and_then(|win| win.document())
        .and_then(|doc| doc.get_element_by_id("canvas"))
        .map(|el| {
            el.dyn_into::<web_sys::HtmlCanvasElement>().map_err(|_| {
                anyhow::anyhow!(
                    "Found the element with the ID for Canvas but it was not a canvas type"
                )
            })
        })
        .transpose()
}

#[allow(dead_code)]
struct CanvasWindowAttributesFactory;
impl WindowAttributesFactory for CanvasWindowAttributesFactory {
    fn create_attributes(&self) -> Result<Option<WindowAttributes>> {
        let canvas = get_canvas()?;
        if canvas.is_none() {
            trace!("Canvas not found");
            return Ok(None);
        }

        #[allow(unused_mut)]
        let mut attributes = Window::default_attributes();
        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::JsCast;
            use winit::platform::web::WindowAttributesExtWebSys;

            // Apply canvas to window attributes
            attributes = attributes
                .with_canvas(canvas)
                .with_focusable(false)
                .with_prevent_default(false);
        }
        Ok(Some(attributes))
    }
}

#[allow(unused_variables)]
async fn run(event_loop: EventLoop<UserEvent>) {
    {
        #[cfg(target_arch = "wasm32")]
        {
            let mut app =
                unigraph_wgpu::create_application(Box::new(CanvasWindowAttributesFactory))
                    .await
                    .expect("Failed to create application");
            use winit::platform::web::EventLoopExtWebSys;
            // Use spawn_app instead of run_app for non-blocking operation
            event_loop.spawn_app(app);
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen(start))]
pub fn this_will_run_automatically() -> Result<(), WasmJSError> {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    tracing_wasm::set_as_global_default();
    if ENABLE_LL {
        ll::add_reporter(Arc::new(console_reporter::ConsoleReporter::new()));
    }
    console_log::init_with_level(log::Level::Trace).expect("Could't initialize logger");
    GlobalState::init();
    let task = ll::Task::create_new("unigraph_wasm");
    GlobalGraphState::init(&task);
    let event_loop = EventLoop::<UserEvent>::with_user_event().build().unwrap();
    GlobalState::set_event_loop_proxy(event_loop.create_proxy());
    wasm_bindgen_futures::spawn_local(run(event_loop));
    Ok(())
}

#[wasm_bindgen]
pub fn set_event_loop_active(active: bool) -> Result<(), WasmJSError> {
    Ok(unigraph_wgpu::set_event_loop_active(active)?)
}

#[wasm_bindgen]
pub fn set_graphs(explorer_component_input_graphs_json: String) -> Result<(), WasmJSError> {
    let graphs: ExplorerComponentInputGraphs =
        serde_json::from_str(&explorer_component_input_graphs_json)
            .context("Failed to deserialize ExplorerComponentInputGraphs")?;

    let (left, right) = match graphs {
        ExplorerComponentInputGraphs { left: None, right } => {
            let right = parse_input_graph(right).context("right")?;
            (None, right)
        }
        ExplorerComponentInputGraphs {
            left: Some(left),
            right,
        } => {
            let left = parse_input_graph(left).context("left")?;
            let right = parse_input_graph(right).context("right")?;
            (Some(left), right)
        }
    };

    // WASM entry point — no parent task, so this is a legitimate root.
    let task = ll::Task::create_new("set_graphs");

    match left {
        Some(left) => {
            let twin_graph = TwinGraph::from_two(left, right, &task)?;
            GlobalGraphState::graph_state().replace_graph(GraphMode::Twin(twin_graph))?;
        }
        None => {
            let array_graph = right.into_array_graph(&task)?;
            let array_graph = array_graph.append_super_root(false)?;
            GlobalGraphState::graph_state().replace_graph(GraphMode::Single(array_graph))?;
        }
    }

    Ok(())
}

#[wasm_bindgen]
pub fn set_simulation_params(simulation_params_json: String) -> Result<(), WasmJSError> {
    let params = SimulationParams::from_json(&simulation_params_json)?;
    global_graph_state().simulation_params.set(params);
    Ok(())
}

#[wasm_bindgen]
pub fn get_simulation_params() -> Result<String, WasmJSError> {
    Ok(GlobalGraphState::simulation_params().to_json()?)
}

/// This function is mutating the state even though it's a getter.
/// We do this because we can't really affort performing the selection
/// of nodes on every small change in the selection frames cause it
/// involves iterationg though all nodes.
#[wasm_bindgen]
pub fn get_selected_node_idxs() -> Result<Vec<u32>, WasmJSError> {
    Ok(unigraph_wgpu::get_selected_node_idxs()?
        .into_iter()
        .map(|idx| idx.0)
        .collect())
}

#[wasm_bindgen]
pub fn node_idx_to_name(idx: usize) -> Result<String, WasmJSError> {
    let gs = global_graph_state().graph_state.get();
    Ok(gs.mode.idx_to_name(NodeIDX::from(idx)).to_string())
}

#[wasm_bindgen]
pub fn node_name_to_idx_log(name: &str) -> Result<Option<u32>, WasmJSError> {
    let gs = global_graph_state().graph_state.get();
    // Search in R (always present). In twin mode, translate to merged IDX.
    let r = gs.mode.r();
    let local_idx = r.data.node_names_ordered.name_to_idx_log(name);
    Ok(local_idx.map(|idx| gs.mode.to_ui(0b0010, idx).unwrap_or(idx).0))
}

#[wasm_bindgen]
pub fn search_node_name_fuzzy(pattern: &str, limit: usize) -> Result<Vec<String>, WasmJSError> {
    let task = ll::Task::create_new("");
    let gs = global_graph_state().graph_state.get();
    match &gs.mode {
        GraphMode::Single(ag) => Ok(ag
            .data
            .node_names_ordered
            .search_name_fuzzy(pattern, limit, &task)?
            .into_iter()
            .map(|(name, _): (&str, _)| name.to_string())
            .collect()),
        GraphMode::Twin(tg) => Ok(tg
            .search_name_fuzzy(pattern, limit, &task)?
            .into_iter()
            .map(|(name, _): (&str, _)| name.to_string())
            .collect()),
    }
}

#[wasm_bindgen]
pub fn get_metric_names(side: u32) -> Result<Vec<String>, WasmJSError> {
    let gs = GlobalGraphState::graph_state().get();
    Ok(gs
        .mode
        .graph(side)?
        .data
        .node_metadata
        .metrics
        .keys()
        .cloned()
        .collect())
}

#[wasm_bindgen]
pub fn get_node_metrics(
    node_idxs: Vec<u32>,
    metric_name: &str,
    side: u32,
) -> Result<Vec<f32>, WasmJSError> {
    let gs = GlobalGraphState::graph_state().get();
    let ag = gs.mode.graph(side)?;
    let mut result = Vec::with_capacity(node_idxs.len());
    if let Some(metrics) = ag.data.node_metadata.metrics.get(metric_name) {
        for node_idx in node_idxs {
            let local = gs.mode.to_local(side, NodeIDX(node_idx))?;
            result.push(local.map_or(0.0, |idx| metrics[idx]));
        }
        Ok(result)
    } else {
        Ok(vec![0.0; node_idxs.len()])
    }
}

/// Min and max of a metric across ALL nodes (reachable or not), computed in a
/// single O(N) pass in Rust so we don't marshal every value across the boundary.
/// Returns `[min, max]`, or an empty vec when the metric is absent/empty.
#[wasm_bindgen]
pub fn get_metric_min_max(
    metric_name: &str,
    ignore_zero: bool,
    side: u32,
) -> Result<Vec<f32>, WasmJSError> {
    let gs = GlobalGraphState::graph_state().get();
    let ag = gs.mode.graph(side)?;
    Ok(match ag.metric_min_max(metric_name, ignore_zero) {
        Some((min, max)) => vec![min, max],
        None => vec![],
    })
}

#[wasm_bindgen]
pub fn get_transitive_metrics(
    node_idxs: Vec<u32>,
    metric_name: &str,
    dominated: bool,
    side: u32,
) -> Result<Vec<f32>, WasmJSError> {
    let gs = GlobalGraphState::graph_state().get();
    let ag = gs.mode.graph(side)?;
    let mut result = Vec::with_capacity(node_idxs.len());
    for node_idx in node_idxs {
        let local = gs.mode.to_local(side, NodeIDX(node_idx))?;
        let value = match local {
            Some(idx) => ag.get_transitive_metric_value(idx, metric_name, dominated)?,
            None => 0.0,
        };
        result.push(value);
    }
    Ok(result)
}

#[wasm_bindgen]
pub fn get_transitive_tiered_metrics(
    node_idxs: Vec<u32>,
    metric_name: &str,
    dominated: bool,
    side: u32,
) -> Result<String, WasmJSError> {
    let gs = GlobalGraphState::graph_state().get();
    let ag = gs.mode.graph(side)?;
    let mut result = Vec::with_capacity(node_idxs.len());
    for node_idx in node_idxs {
        let local = gs.mode.to_local(side, NodeIDX(node_idx))?;
        let value = match local {
            Some(idx) => ag
                .get_transitive_tiered_metric_values(idx, metric_name, dominated)
                .context("get transitive tiered metrics")?,
            None => std::collections::BTreeMap::new(),
        };
        result.push(value);
    }
    Ok(serde_json::to_string(&result).context("Failed to serialize transitive tiered metrics")?)
}

#[wasm_bindgen]
pub fn get_transitive_tiered_metrics_delta(
    node_idxs: Vec<u32>,
    metric_name: &str,
) -> Result<String, WasmJSError> {
    let gs = GlobalGraphState::graph_state().get();
    let mut result = Vec::with_capacity(node_idxs.len());
    match &gs.mode {
        GraphMode::Single(_) => {
            result.resize(node_idxs.len(), std::collections::BTreeMap::new());
        }
        GraphMode::Twin(tg) => {
            for node_idx in node_idxs {
                result.push(
                    tg.get_transitive_tiered_delta(NodeIDX::from(node_idx), metric_name)
                        .context("get transitive tiered metrics")?,
                );
            }
        }
    }
    Ok(serde_json::to_string(&result)
        .context("Failed to serialize transitive tiered delta metrics")?)
}

#[wasm_bindgen]
pub fn get_combined_metrics_for_nodes(
    node_idxs: Vec<u32>,
    side: u32,
) -> Result<String, WasmJSError> {
    let gs = GlobalGraphState::graph_state().get();
    let ag = gs.mode.graph(side)?;
    let local_idxs: Vec<NodeIDX> = node_idxs
        .iter()
        .filter_map(|&idx| gs.mode.to_local(side, NodeIDX(idx)).ok().flatten())
        .collect();
    let result = ag.get_combined_metrics_for_nodes(&local_idxs)?;
    Ok(serde_json::to_string(&result).context("Failed to serialize combined metrics")?)
}

#[wasm_bindgen]
pub fn get_combined_metrics_for_entrypoints_with_force_include(
    force_edge_include_from: Option<u32>,
    force_edge_include_to: Option<u32>,
    side: u32,
) -> Result<String, WasmJSError> {
    let gs = GlobalGraphState::graph_state().get();
    let overrides = match (force_edge_include_from, force_edge_include_to) {
        (Some(from), Some(to)) => {
            let local_from = gs
                .mode
                .to_local(side, NodeIDX(from))?
                .context("force_edge_include_from node not found on this side")?;
            let local_to = gs
                .mode
                .to_local(side, NodeIDX(to))?
                .context("force_edge_include_to node not found on this side")?;
            EdgeOverrides::from_triplets([(local_from, local_to, true)])
        }
        (None, None) => EdgeOverrides::default(),
        _ => {
            return Err(anyhow::anyhow!(
                "force_edge_include_from and force_edge_include_to must both be set or both be None"
            )
            .into());
        }
    };

    let result = gs
        .mode
        .graph(side)?
        .get_combined_metrics_for_entry_points(&overrides)?;
    Ok(serde_json::to_string(&result).context("Failed to serialize combined metrics")?)
}

#[wasm_bindgen]
pub fn get_available_tiers(side: u32) -> Result<Vec<String>, WasmJSError> {
    let gs = GlobalGraphState::graph_state().get();
    Ok(gs
        .mode
        .graph(side)?
        .runtime
        .state
        .tiers
        .iter()
        .map(|(name, _)| name.to_string())
        .collect())
}

#[wasm_bindgen]
pub fn get_arrow_pairs(
    node_idx: usize,
    graph_structure: u8,
    changed_nodes_only: bool,
) -> Result<String, WasmJSError> {
    let gs = GlobalGraphState::graph_state().get();
    let graph_structure = GraphStructure::from_u8(graph_structure)?;
    let node_idx = NodeIDX::from(node_idx);
    let arrow_pairs = match &gs.mode {
        GraphMode::Single(ag) => {
            let arrows = ag.get_arrows(node_idx, graph_structure)?;
            // Wrap single-graph arrows as TwinArrows with r only
            arrows
                .into_iter()
                .map(|a| unigraph_core::TwinArrow {
                    points_to: a.points_to,
                    points_from: a.points_from,
                    node_diff: unigraph_core::NodeDiff::empty(),
                    l: None,
                    r: Some(a),
                })
                .collect()
        }
        GraphMode::Twin(tg) => tg.get_twin_arrows(node_idx, graph_structure, changed_nodes_only)?,
    };
    Ok(serde_json::to_string(&arrow_pairs).context("Failed to serialize arrows")?)
}

#[wasm_bindgen]
pub fn get_shortest_path(
    from: &[u32],
    to: u32,
    graph_structure: u8,
    traversal_type: u8,
    changed_nodes_only: bool,
) -> Result<Option<Vec<u32>>, WasmJSError> {
    let gs = GlobalGraphState::graph_state().get();
    let graph_structure = GraphStructure::from_u8(graph_structure)?;
    let traversal_type = TraversalType::from_u8(traversal_type)?;
    let from: Vec<NodeIDX> = from.iter().map(|&idx| NodeIDX::from(idx)).collect();
    let to = NodeIDX::from(to);

    let path = match &gs.mode {
        GraphMode::Single(ag) => ag.shortest_path(&from, to, graph_structure, traversal_type),
        GraphMode::Twin(tg) => tg.shortest_path(
            &from,
            to,
            graph_structure,
            traversal_type,
            changed_nodes_only,
        )?,
    };

    match path {
        Some(p) if !p.is_empty() => Ok(Some(p.into_iter().map(|idx| idx.0).collect())),
        _ => Ok(None),
    }
}

/// Minimum edge cut separating `sinks` from the graph's entry points. Only
/// available for a single graph — comparison (twin) mode has no single index
/// space to cut over and the UI hides the panel there. Entry points are derived
/// from the graph itself; the caller only supplies the nodes to cut off.
///
/// `protected_from`/`protected_to` are parallel arrays of edges that must never
/// be cut (made uncuttable), so the algorithm finds an alternative cut that
/// routes around them. The two arrays must be the same length.
///
/// Returns a JSON-serialized [`MinCutResult`].
#[wasm_bindgen]
pub fn min_cut(
    sinks: &[u32],
    protected_from: &[u32],
    protected_to: &[u32],
) -> Result<String, WasmJSError> {
    let gs = GlobalGraphState::graph_state().get();
    let ag = match &gs.mode {
        GraphMode::Single(ag) => ag,
        GraphMode::Twin(_) => {
            return Err(anyhow::anyhow!("min_cut is not supported in comparison mode").into());
        }
    };
    let sources = ag.determine_entrypoints();
    let sinks: Vec<NodeIDX> = sinks.iter().map(|&idx| NodeIDX::from(idx)).collect();
    let protected: std::collections::BTreeSet<(NodeIDX, NodeIDX)> = protected_from
        .iter()
        .zip(protected_to.iter())
        .map(|(&from, &to)| (NodeIDX::from(from), NodeIDX::from(to)))
        .collect();
    let result = unigraph_core::min_cut(ag, &sources, &sinks, &protected);
    Ok(serde_json::to_string(&MinCutResult::from(result))
        .context("Failed to serialize min cut result")?)
}

#[wasm_bindgen]
pub fn get_reverse_edges_len(node_idxs: Vec<u32>, side: u32) -> Result<Vec<usize>, WasmJSError> {
    let gs = GlobalGraphState::graph_state().get();
    let ag = gs.mode.graph(side)?;
    node_idxs
        .into_iter()
        .map(|node_idx| {
            let local = gs.mode.to_local(side, NodeIDX(node_idx))?;
            Ok(local.map_or(0, |idx| ag.parents_len_configured(idx)))
        })
        .collect::<Result<Vec<usize>>>()
        .map_err(Into::into)
}

#[wasm_bindgen]
pub fn get_transitive_count(node_idxs: Vec<u32>, side: u32) -> Result<Vec<usize>, WasmJSError> {
    let gs = GlobalGraphState::graph_state().get();
    let ag = gs.mode.graph(side)?;
    node_idxs
        .into_iter()
        .map(|node_idx| {
            let local = gs.mode.to_local(side, NodeIDX(node_idx))?;
            Ok(local.map_or(0, |idx| ag.transitive_count_configured(idx)))
        })
        .collect::<Result<Vec<usize>>>()
        .map_err(Into::into)
}

#[wasm_bindgen]
pub fn get_transitive_count_delta(node_idxs: Vec<u32>) -> Result<Vec<i32>, WasmJSError> {
    let gs = GlobalGraphState::graph_state().get();
    match &gs.mode {
        GraphMode::Single(_) => Ok(vec![0; node_idxs.len()]),
        GraphMode::Twin(tg) => node_idxs
            .into_iter()
            .map(|node_idx| tg.get_transitive_count_delta(NodeIDX::from(node_idx)))
            .collect::<Result<Vec<i32>>>()
            .map_err(Into::into),
    }
}

#[wasm_bindgen]
pub fn get_transitive_count_dominated(
    node_idxs: Vec<u32>,
    side: u32,
) -> Result<Vec<usize>, WasmJSError> {
    let gs = GlobalGraphState::graph_state().get();
    let ag = gs.mode.graph(side)?;
    node_idxs
        .into_iter()
        .map(|node_idx| {
            let local = gs.mode.to_local(side, NodeIDX(node_idx))?;
            Ok(local.map_or(0, |idx| ag.transitive_count_configured_dominated(idx)))
        })
        .collect::<Result<Vec<usize>>>()
        .map_err(Into::into)
}

#[wasm_bindgen]
pub fn get_node_flags(side: u32) -> Result<Vec<u32>, WasmJSError> {
    let gs = GlobalGraphState::graph_state().get();
    let ag = gs.mode.graph(side)?;
    let node_count = gs.mode.node_count();
    let mut flags = Vec::with_capacity(node_count);
    for idx in 0..node_count {
        let local = gs.mode.to_local(side, NodeIDX::from(idx))?;
        let f = match local {
            Some(local_idx) => ag.runtime.node_flags[local_idx].bits(),
            None => 1, // UNREACHABLE
        };
        flags.push(f);
    }
    Ok(flags)
}

#[wasm_bindgen]
pub fn get_graph_traversal_config(side: u32) -> Result<String, WasmJSError> {
    let gs = GlobalGraphState::graph_state().get();
    let traversal_config = gs
        .mode
        .graph(side)?
        .runtime
        .state
        .traversal_config
        .clone()
        .unwrap_or_default();
    Ok(serde_json::to_string(&traversal_config).context("Failed to serialize traversal config")?)
}

#[wasm_bindgen]
pub fn get_graph_settings(side: u32) -> Result<String, WasmJSError> {
    let gs = GlobalGraphState::graph_state().get();
    let graph_settings = gs
        .mode
        .graph(side)?
        .graph_settings()
        .cloned()
        .unwrap_or_default();
    Ok(serde_json::to_string(&graph_settings).context("Failed to serialize Graph Settings")?)
}

#[wasm_bindgen]
pub fn set_graph_settings(graph_settings_json: String, side: u32) -> Result<(), WasmJSError> {
    let graph_settings: unigraph_core::graph_settings::GraphSettings =
        serde_json::from_str(&graph_settings_json).context("Failed to parse graph settings")?;
    GlobalGraphState::graph_state_mut()
        .mode
        .graph_mut(side)?
        .set_graph_settings(graph_settings);
    Ok(())
}

#[wasm_bindgen]
pub fn available_metric_views(side: u32) -> Result<String, WasmJSError> {
    let gs = GlobalGraphState::graph_state().get();
    let ag = gs.mode.graph(side)?;
    let views: Vec<String> = ag
        .available_metric_views()
        .iter()
        .map(|v| v.to_string())
        .collect();
    Ok(serde_json::to_string(&views).context("Failed to serialize metric views")?)
}

#[wasm_bindgen]
pub fn visible_metric_views(side: u32, structure: u8) -> Result<String, WasmJSError> {
    let structure = GraphStructure::from_u8(structure).context("Invalid graph structure value")?;
    let gs = GlobalGraphState::graph_state().get();
    let ag = gs.mode.graph(side)?;
    let views: Vec<String> = ag
        .visible_metric_views(structure)
        .iter()
        .map(|v| v.to_string())
        .collect();
    Ok(serde_json::to_string(&views).context("Failed to serialize metric views")?)
}

#[wasm_bindgen]
pub fn apply_traversal_config(traversal_config_json: String, side: u32) -> Result<(), WasmJSError> {
    let traversal_config: TraversalConfig =
        serde_json::from_str(&traversal_config_json).context("Failed to parse traversal config")?;
    GlobalGraphState::graph_state_mut()
        .mode
        .graph_mut(side)?
        .apply_traversal_config_and_entry_points(traversal_config)
        .context("Failed to apply traversal config")?;
    GlobalGraphState::graph_state_mut().sync_node_attributes()?;
    GlobalState::send_event_loop_event(UserEvent::GraphUpdated)?;
    Ok(())
}

#[wasm_bindgen]
pub fn get_graph_node_count(_side: u32) -> Result<usize, WasmJSError> {
    Ok(GlobalGraphState::graph_state().get().mode.node_count())
}

#[wasm_bindgen]
pub fn determine_entrypoints(side: u32) -> Result<Vec<u32>, WasmJSError> {
    let gs = GlobalGraphState::graph_state().get();
    let ag = gs.mode.graph(side)?;
    let local_idxs = ag.determine_entrypoints();
    local_idxs
        .iter()
        .map(|&idx| Ok(gs.mode.to_ui(side, idx)?.0))
        .collect::<Result<Vec<u32>>>()
        .map_err(Into::into)
}

#[wasm_bindgen]
pub fn get_array_graph_stats(side: u32) -> Result<String, WasmJSError> {
    let gs = GlobalGraphState::graph_state().get();
    let stats = gs.mode.graph(side)?.stats();
    Ok(serde_json::to_string(&stats).context("Failed to serialize graph stats")?)
}

/// Export the graph on `side` to `format` (`"MapGraphJson"` | `"Gephi"`),
/// including only the nodes/edges selected by `scope` (`"Reachable"` |
/// `"Whole"`). Returns the raw file bytes — the JS side wraps them in a Blob to
/// trigger a download. Bytes (not a String) so we don't pay a UTF-16 copy for
/// large graphs.
#[wasm_bindgen]
pub fn export_graph(side: u32, scope: &str, format: &str) -> Result<Vec<u8>, WasmJSError> {
    let scope = ExportScope::from_str(scope)?;
    let format = ExportFormat::from_str(format)?;
    let gs = GlobalGraphState::graph_state().get();
    let ag = gs.mode.graph(side)?;
    Ok(export_graph_bytes(ag, scope, format)?)
}

#[wasm_bindgen]
/// Takes a base64-encoded (url safe no pad) zstd compressed string and returns it.
/// MUST be a valid string (likely JSON) that can be converted to a UTF-8 string.
pub fn from_zstd_base64_url_safe_no_pad(zstd_base64: &str) -> Result<String, WasmJSError> {
    Ok(SerializationFormat::JsonZstdBestBase64URLSafeNoPad.parse_string(zstd_base64)?)
}

#[wasm_bindgen]
pub fn to_zstd_base64_url_safe_no_pad(s: String) -> Result<String, WasmJSError> {
    Ok(SerializationFormat::JsonZstdBestBase64URLSafeNoPad.to_string(&s)?)
}

/// Compute the delta between a base and modified GraphQueryConfig.
/// Returns a zstd+base64 encoded delta string, or empty string if unchanged.
#[wasm_bindgen]
pub fn derive_gqc_delta(base_json: String, modified_json: String) -> Result<String, WasmJSError> {
    let base: GraphQueryConfig =
        serde_json::from_str(&base_json).context("failed to parse base GQC")?;
    let modified: GraphQueryConfig =
        serde_json::from_str(&modified_json).context("failed to parse modified GQC")?;

    match base.derive_delta(&modified) {
        Some(delta) => {
            let delta_json =
                serde_json::to_string(&delta).context("failed to serialize GQC delta")?;
            Ok(to_zstd_base64_url_safe_no_pad_inner(&delta_json)?)
        }
        None => Ok(String::new()),
    }
}

/// Apply a zstd+base64 encoded delta to a base GraphQueryConfig.
/// Returns the resulting GraphQueryConfig as JSON.
#[wasm_bindgen]
pub fn apply_gqc_delta(base_json: String, delta_base64: &str) -> Result<String, WasmJSError> {
    let mut base: GraphQueryConfig =
        serde_json::from_str(&base_json).context("failed to parse base GQC")?;
    let delta_json: String =
        from_zstd_base64_url_safe_no_pad_inner(delta_base64).context("failed to decode delta")?;
    let delta: <GraphQueryConfig as Deltable>::Delta =
        serde_json::from_str(&delta_json).context("failed to parse GQC delta")?;
    base.apply_delta(delta)
        .context("failed to apply GQC delta")?;
    Ok(serde_json::to_string(&base).context("failed to serialize result GQC")?)
}

fn from_zstd_base64_url_safe_no_pad_inner(zstd_base64: &str) -> Result<String> {
    SerializationFormat::JsonZstdBestBase64URLSafeNoPad.parse_string(zstd_base64)
}

fn to_zstd_base64_url_safe_no_pad_inner(s: &String) -> Result<String> {
    SerializationFormat::JsonZstdBestBase64URLSafeNoPad.to_string(s)
}

fn parse_input_graph(g: ExplorerComponentInputGraph) -> Result<ArrayGraphSerializable> {
    match g {
        ExplorerComponentInputGraph::ArrayGraphSerialized(serialized_str) => serialized_str.parse(),
        ExplorerComponentInputGraph::MapGraphSerialized(serialized_str) => {
            let map_graph = serialized_str
                .parse::<MapGraph>()
                .context("Failed to parse map graph")?;
            map_graph.to_array_graph_serializable()
        }
        ExplorerComponentInputGraph::ArrayGraphSerializedPackageBase64(serialized_str) => {
            let package_base64 = serialized_str
                .parse::<ArrayGraphSerializablePackageBase64>()
                .context("Failed to parse array graph serialized package")?;
            let package = ArrayGraphSerializablePackage::from_base64(package_base64)?;
            let task = ll::Task::create_new("");
            ArrayGraphSerializable::unpack(&package, &task)
        }
    }
    .context("Failed to parse input graph")
}
