// Copyright (c) Meta Platforms, Inc. and affiliates.

#![allow(clippy::collapsible_if)]
mod wasm_error;

use std::vec;

use anyhow::Context;
use anyhow::Result;
use log::trace;
use unigraph_core::ArrayGraph;
use unigraph_core::ArrayGraphSerializable;
use unigraph_core::ArrayGraphSerializablePackage;
use unigraph_core::ArrayGraphSerializablePackageBase64;
use unigraph_core::MapGraph;
use unigraph_core::TraversalConfig;
use unigraph_core::TraversalType;
use unigraph_core::TwinGraph;
use unigraph_core::graph_settings::GraphStructure;
use unigraph_core::types::NodeIDX;
use unigraph_core::ui_types::ExplorerComponentInputGraph;
use unigraph_core::ui_types::ExplorerComponentInputGraphs;
use unigraph_graph_state::GlobalGraphState;
use unigraph_graph_state::global_graph_state;
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
    console_log::init_with_level(log::Level::Trace).expect("Could't initialize logger");
    GlobalState::init();
    GlobalGraphState::init();
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
        ExplorerComponentInputGraphs { left, right: None } => {
            let left = parse_input_graph(left).context("left")?;
            (left, None)
        }
        ExplorerComponentInputGraphs {
            left,
            right: Some(right),
        } => {
            let left = parse_input_graph(left).context("left")?;
            let right = parse_input_graph(right).context("right")?;
            (left, Some(right))
        }
    };

    match right {
        Some(right) => {
            let twin_graph = TwinGraph::from_two(left, right)?;
            GlobalGraphState::graph_state().replace_graph(twin_graph)?;
        }
        None => {
            let array_graph: ArrayGraph = left.into();
            let twin_graph = TwinGraph::from_one(array_graph.append_super_root()?)?;
            GlobalGraphState::graph_state().replace_graph(twin_graph)?;
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
    Ok(global_graph_state()
        .graph_state
        .get()
        .twin_graph
        .node_names
        .idx_to_name(NodeIDX::from(idx))
        .to_string())
}

#[wasm_bindgen]
pub fn node_name_to_idx_log(name: &str) -> Result<Option<u32>, WasmJSError> {
    Ok(global_graph_state()
        .graph_state
        .get()
        .twin_graph
        .node_names
        .name_to_idx_log(name)
        .map(|idx| idx.0))
}

#[wasm_bindgen]
pub fn search_node_name_fuzzy(pattern: &str, limit: usize) -> Result<Vec<String>, WasmJSError> {
    Ok(global_graph_state()
        .graph_state
        .get()
        .twin_graph
        .node_names
        .search_name_fuzzy(pattern, limit)?
        .into_iter()
        .map(|(name, _node_idx)| name.to_string())
        .collect())
}

#[wasm_bindgen]
pub fn get_metric_names(side: u32) -> Result<Vec<String>, WasmJSError> {
    Ok(GlobalGraphState::graph_state()
        .get()
        .twin_graph
        .graph_u32(side)?
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
    let graph_state = GlobalGraphState::graph_state().get();
    let mut result = Vec::with_capacity(node_idxs.len());
    if let Some(metrics) = graph_state
        .twin_graph
        .graph_u32(side)?
        .metrics
        .get(metric_name)
    {
        for node_idx in node_idxs {
            result.push(metrics[NodeIDX(node_idx)]);
        }
        Ok(result)
    } else {
        Ok(vec![0.0; node_idxs.len()])
    }
}

#[wasm_bindgen]
pub fn get_transitive_metrics(
    node_idxs: Vec<u32>,
    metric_name: &str,
    dominated: bool,
    side: u32,
) -> Result<Vec<f32>, WasmJSError> {
    let graph_state = GlobalGraphState::graph_state().get();
    let mut result = Vec::with_capacity(node_idxs.len());
    for node_idx in node_idxs {
        let transitive_value = graph_state
            .twin_graph
            .graph_u32(side)?
            .get_transitive_metric_value(NodeIDX(node_idx), metric_name, dominated)?;
        result.push(transitive_value);
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
    let graph_state = GlobalGraphState::graph_state().get();
    let mut result = Vec::with_capacity(node_idxs.len());
    for node_idx in node_idxs {
        let transitive_value = graph_state
            .twin_graph
            .graph_u32(side)?
            .get_transitive_tiered_metric_values(NodeIDX(node_idx), metric_name, dominated)
            .context("get transitive tiered metrics")?;
        result.push(transitive_value);
    }
    Ok(serde_json::to_string(&result).context("Failed to serialize transitive tiered metrics")?)
}

#[wasm_bindgen]
pub fn get_combined_metrics_for_nodes(
    node_idxs: Vec<u32>,
    side: u32,
) -> Result<String, WasmJSError> {
    let graph_state = GlobalGraphState::graph_state().get();
    let result = graph_state
        .twin_graph
        .graph_u32(side)?
        .get_combined_metrics_for_nodes(
            &node_idxs
                .iter()
                .map(|&idx| NodeIDX(idx))
                .collect::<Vec<_>>(),
        )?;
    Ok(serde_json::to_string(&result).context("Failed to serialize transitive tiered metrics")?)
}

#[wasm_bindgen]
pub fn get_combined_metrics_for_entrypoints_with_force_include(
    force_edge_include_from: Option<u32>,
    force_edge_include_to: Option<u32>,
    side: u32,
) -> Result<String, WasmJSError> {
    let force_edge_include = match (force_edge_include_from, force_edge_include_to) {
        (Some(from), Some(to)) => Some((NodeIDX(from), NodeIDX(to))),
        (None, None) => None,
        _ => {
            return Err(anyhow::anyhow!(
                "force_edge_include_from and force_edge_include_to must both be set or both be None"
            )
            .into());
        }
    };

    let mut graph_state = GlobalGraphState::graph_state().get_mut();
    let result = graph_state
        .twin_graph
        .graph_u32_mut(side)?
        .get_combined_metrics_for_entry_points(force_edge_include)?;
    Ok(serde_json::to_string(&result).context("Failed to serialize transitive tiered metrics")?)
}

#[wasm_bindgen]
pub fn get_conjoint_cost(side: u32) -> Result<String, WasmJSError> {
    let graph_state = GlobalGraphState::graph_state().get();
    let conjoint_cost = graph_state.twin_graph.graph_u32(side)?.conjoint_cost();
    Ok(serde_json::to_string(&conjoint_cost).context("Failed to serialize conjoint cost")?)
}

#[wasm_bindgen]
pub fn get_available_tiers(side: u32) -> Result<Vec<String>, WasmJSError> {
    let graph_state = GlobalGraphState::graph_state().get();
    Ok(graph_state
        .twin_graph
        .graph_u32(side)?
        .state
        .tiers
        .iter()
        .map(|(name, _)| name.to_string())
        .collect())
}

// TODO: we will need to build combined arrows and dedup
#[wasm_bindgen]
pub fn get_arrow_pairs(
    node_idx: usize,
    graph_structure: u8,
    changed_nodes_only: bool,
) -> Result<String, WasmJSError> {
    let graph_state = GlobalGraphState::graph_state().get();
    let graph_structure = GraphStructure::from_u8(graph_structure)?;
    let node_idx = NodeIDX::from(node_idx);
    let arrow_pairs =
        graph_state
            .twin_graph
            .get_twin_arrows(node_idx, graph_structure, changed_nodes_only)?;

    Ok(serde_json::to_string(&arrow_pairs).context("Failed to serialize arrows")?)
}

// TODO: we need to check both sides and pick the shortest
#[wasm_bindgen]
pub fn get_shortest_path(
    from: &[u32],
    to: u32,
    graph_structure: u8,
    traversal_type: u8,
    changed_nodes_only: bool,
) -> Result<Option<Vec<u32>>, WasmJSError> {
    let graph_state = GlobalGraphState::graph_state().get();
    let tg = &graph_state.twin_graph;
    let graph_structure = GraphStructure::from_u8(graph_structure)?;
    let from = from
        .iter()
        .map(|&idx| NodeIDX::from(idx))
        .collect::<Vec<NodeIDX>>();
    let to = NodeIDX::from(to);

    let traversal_type = TraversalType::from_u8(traversal_type)?;

    if let Some(shortest_path) = tg.shortest_path(
        &from,
        to,
        graph_structure,
        traversal_type,
        changed_nodes_only,
    )? {
        if !shortest_path.is_empty() {
            return Ok(Some(
                shortest_path
                    .into_iter()
                    .map(|idx| idx.0)
                    .collect::<Vec<u32>>(),
            ));
        }
    }

    Ok(None)
}

#[wasm_bindgen]
pub fn get_reverse_edges_len(node_idxs: Vec<u32>, side: u32) -> Result<Vec<usize>, WasmJSError> {
    let graph_state = GlobalGraphState::graph_state().get();

    let result = node_idxs
        .into_iter()
        .map(|node_idx| {
            Ok(graph_state
                .twin_graph
                .graph_u32(side)?
                .parents_len_configured(node_idx.into()))
        })
        .collect::<Result<Vec<usize>>>()?;

    Ok(result)
}

#[wasm_bindgen]
pub fn get_transitive_count(node_idxs: Vec<u32>, side: u32) -> Result<Vec<usize>, WasmJSError> {
    let graph_state = GlobalGraphState::graph_state().get();

    let result = node_idxs
        .into_iter()
        .map(|node_idx| {
            Ok(graph_state
                .twin_graph
                .graph_u32(side)?
                .transitive_count_configured(node_idx.into()))
        })
        .collect::<Result<Vec<usize>>>()?;

    Ok(result)
}

#[wasm_bindgen]
pub fn get_transitive_count_delta(node_idxs: Vec<u32>) -> Result<Vec<i32>, WasmJSError> {
    let graph_state = GlobalGraphState::graph_state().get();

    let result = node_idxs
        .into_iter()
        .map(|node_idx| {
            Ok(graph_state
                .twin_graph
                .get_transitive_count_delta(NodeIDX::from(node_idx))
                .unwrap_or_default())
        })
        .collect::<Result<Vec<i32>>>()?;

    Ok(result)
}

#[wasm_bindgen]
pub fn get_transitive_count_dominated(
    node_idxs: Vec<u32>,
    side: u32,
) -> Result<Vec<usize>, WasmJSError> {
    let graph_state = GlobalGraphState::graph_state().get();

    let result = node_idxs
        .into_iter()
        .map(|node_idx| {
            Ok(graph_state
                .twin_graph
                .graph_u32(side)?
                .transitive_count_configured_dominated(node_idx.into()))
        })
        .collect::<Result<Vec<usize>>>()?;

    Ok(result)
}

// TODO: need to combine with both graphs.
#[wasm_bindgen]
pub fn get_all_reachable_node_idxs(side: u32) -> Result<Vec<u32>, WasmJSError> {
    let graph_state = GlobalGraphState::graph_state().get();
    let reachable_nodes = graph_state
        .twin_graph
        .graph_u32(side)?
        .all_reachable_node_idxs();
    Ok(reachable_nodes.iter().map(|idx| idx.0).collect())
}

#[wasm_bindgen]
pub fn get_graph_traversal_config(side: u32) -> Result<String, WasmJSError> {
    let traversal_config = GlobalGraphState::graph_state()
        .get()
        .twin_graph
        .graph_u32(side)?
        .state
        .traversal_config
        .clone()
        .unwrap_or_default();
    Ok(serde_json::to_string(&traversal_config).context("Failed to serialize traversal config")?)
}

#[wasm_bindgen]
pub fn get_graph_settings(side: u32) -> Result<String, WasmJSError> {
    let graph_settings = GlobalGraphState::graph_state()
        .get()
        .twin_graph
        .graph_u32(side)?
        .graph_settings
        .clone()
        .unwrap_or_default();
    Ok(serde_json::to_string(&graph_settings).context("Failed to serialize Graph Settings")?)
}

#[wasm_bindgen]
pub fn apply_traversal_config(traversal_config_json: String, side: u32) -> Result<(), WasmJSError> {
    let traversal_config: TraversalConfig =
        serde_json::from_str(&traversal_config_json).context("Failed to parse traversal config")?;
    GlobalGraphState::graph_state_mut()
        .twin_graph
        .graph_u32_mut(side)?
        .apply_traversal_config(traversal_config)
        .context("Failed to apply traversal config")?;
    GlobalGraphState::graph_state_mut().sync_node_attributes()?;
    GlobalState::send_event_loop_event(UserEvent::GraphUpdated)?;
    Ok(())
}

// TODO: is this used?
#[wasm_bindgen]
pub fn get_graph_node_count(side: u32) -> Result<usize, WasmJSError> {
    Ok(GlobalGraphState::graph_state()
        .get()
        .twin_graph
        .graph_u32(side)?
        .nodes_len())
}

// TODO: also need to combine with both graphs.
#[wasm_bindgen]
pub fn determine_entrypoints(side: u32) -> Result<Vec<u32>, WasmJSError> {
    let node_idxs = GlobalGraphState::graph_state()
        .get()
        .twin_graph
        .graph_u32(side)?
        .determine_entrypoints();
    Ok(node_idxs.iter().map(|idx| idx.0).collect())
}

#[wasm_bindgen]
pub fn get_array_graph_stats(side: u32) -> Result<String, WasmJSError> {
    let stats = &GlobalGraphState::graph_state()
        .get()
        .twin_graph
        .graph_u32(side)?
        .stats();
    Ok(serde_json::to_string(&stats).context("Failed to serialize graph stats")?)
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
            ArrayGraphSerializable::unpack(&package)
        }
    }
    .context("Failed to parse input graph")
}
