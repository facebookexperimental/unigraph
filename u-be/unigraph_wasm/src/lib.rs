// Copyright (c) Meta Platforms, Inc. and affiliates.

mod serialization;
mod wasm_error;

use std::vec;

use anyhow::Context;
use anyhow::Result;
use log::trace;
use unigraph_core::ArrayGraph;
use unigraph_core::ArrayGraphSerializable;
use unigraph_core::MapGraph;
use unigraph_core::TraversalConfig;
use unigraph_core::graph_settings::GraphStructure;
use unigraph_core::make_test_graph;
use unigraph_core::types::NodeIDX;
use unigraph_wgpu::GlobalState;
use unigraph_wgpu::UserEvent;
use unigraph_wgpu::WindowAttributesFactory;
use unigraph_wgpu::global_state;
use unigraph_wgpu::ts_types::SimulationParams;
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
pub fn set_map_graph(graph_json: Option<String>) -> Result<(), WasmJSError> {
    let array_graph = if let Some(graph_json) = graph_json {
        MapGraph::from_json(&graph_json)
            .unwrap()
            .to_array_graph()
            .unwrap()
    } else {
        log::info!("No graph provided, using test graph");
        make_test_graph().unwrap().to_array_graph().unwrap()
    };
    GlobalState::graph_state().replace_graph(array_graph.append_super_root()?)?;
    Ok(())
}

#[wasm_bindgen]
pub fn set_array_graph_json_zstd_base64(
    array_graph_json_zstd_base64: String,
) -> Result<(), WasmJSError> {
    let json_bytes = serialization::from_zstd_base64(&array_graph_json_zstd_base64)
        .context("Failed to decode array_graph_json_zstd_base64")?;

    let array_graph_serializable = ArrayGraphSerializable::from_json_bytes(&json_bytes)
        .context("Failed to deserialize ArrayGraph JSON bytes")?;
    let array_graph: ArrayGraph = array_graph_serializable.into();
    GlobalState::graph_state().replace_graph(array_graph.append_super_root()?)?;
    Ok(())
}

#[wasm_bindgen]
pub fn set_simulation_params(simulation_params_json: String) -> Result<(), WasmJSError> {
    let params = SimulationParams::from_json(&simulation_params_json)?;
    global_state().simulation_params.set(params);
    Ok(())
}

#[wasm_bindgen]
pub fn get_simulation_params() -> Result<String, WasmJSError> {
    Ok(GlobalState::simulation_params().to_json()?)
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
    Ok(global_state()
        .graph_state
        .get()
        .array_graph
        .node_names_ordered
        .idx_to_name(NodeIDX::from(idx))
        .to_string())
}

#[wasm_bindgen]
pub fn node_name_to_idx_log(name: &str) -> Result<Option<u32>, WasmJSError> {
    Ok(global_state()
        .graph_state
        .get()
        .array_graph
        .node_names_ordered
        .name_to_idx_log(name)
        .map(|idx| idx.0))
}

#[wasm_bindgen]
pub fn get_metric_names() -> Result<Vec<String>, WasmJSError> {
    Ok(GlobalState::graph_state()
        .get()
        .array_graph
        .metrics
        .keys()
        .cloned()
        .collect())
}

#[wasm_bindgen]
pub fn get_node_metrics(node_idxs: Vec<u32>, metric_name: &str) -> Result<Vec<f32>, WasmJSError> {
    let graph_state = GlobalState::graph_state().get();
    let mut result = Vec::with_capacity(node_idxs.len());
    if let Some(metrics) = graph_state.array_graph.metrics.get(metric_name) {
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
) -> Result<Vec<f32>, WasmJSError> {
    let graph_state = GlobalState::graph_state().get();
    let mut result = Vec::with_capacity(node_idxs.len());
    for node_idx in node_idxs {
        let transitive_value = graph_state
            .array_graph
            .get_transitive_metric_value(NodeIDX(node_idx), metric_name)?;
        result.push(transitive_value);
    }
    Ok(result)
}

#[wasm_bindgen]
pub fn get_transitive_tiered_metrics(
    node_idxs: Vec<u32>,
    metric_name: &str,
    dominated: bool,
) -> Result<String, WasmJSError> {
    let graph_state = GlobalState::graph_state().get();
    let mut result = Vec::with_capacity(node_idxs.len());
    for node_idx in node_idxs {
        let transitive_value = graph_state
            .array_graph
            .get_transitive_tiered_metric_values(NodeIDX(node_idx), metric_name, dominated)
            .context("get transitive tiered metrics")?;
        result.push(transitive_value);
    }
    Ok(serde_json::to_string(&result).context("Failed to serialize transitive tiered metrics")?)
}

#[wasm_bindgen]
pub fn get_combined_metrics_for_nodes(node_idxs: Vec<u32>) -> Result<String, WasmJSError> {
    let graph_state = GlobalState::graph_state().get();
    let result = graph_state.array_graph.get_combined_metrics_for_nodes(
        &node_idxs
            .iter()
            .map(|&idx| NodeIDX(idx))
            .collect::<Vec<_>>(),
    )?;
    Ok(serde_json::to_string(&result).context("Failed to serialize transitive tiered metrics")?)
}

#[wasm_bindgen]
pub fn get_conjoint_cost() -> Result<String, WasmJSError> {
    let graph_state = GlobalState::graph_state().get();
    let conjoint_cost = graph_state.array_graph.conjoint_cost();
    Ok(serde_json::to_string(&conjoint_cost).context("Failed to serialize conjoint cost")?)
}

#[wasm_bindgen]
pub fn get_available_tiers() -> Result<Vec<String>, WasmJSError> {
    let graph_state = GlobalState::graph_state().get();
    Ok(graph_state
        .array_graph
        .tiers
        .iter()
        .map(|(name, _)| name.to_string())
        .collect())
}

#[wasm_bindgen]
pub fn get_arrows(node_idx: usize, graph_structure: u8) -> Result<String, WasmJSError> {
    let graph_state = GlobalState::graph_state().get();
    let ag = &graph_state.array_graph;
    let graph_structure = GraphStructure::from_u8(graph_structure)?;
    let node_idx = NodeIDX::from(node_idx);

    let arrows = match graph_structure {
        GraphStructure::Forward => ag.get_arrows_forward(node_idx),
        GraphStructure::Dominator => Ok(ag.get_arrows_dominator(node_idx)),
        GraphStructure::Reverse => ag.get_arrows_reverse(node_idx),
    }?;

    Ok(serde_json::to_string(&arrows).context("Failed to serialize arrows")?)
}

#[wasm_bindgen]
pub fn get_shortest_path(
    from: &[u32],
    to: u32,
    graph_structure: u8,
) -> Result<Option<Vec<u32>>, WasmJSError> {
    let graph_state = GlobalState::graph_state().get();
    let ag = &graph_state.array_graph;
    let graph_structure = GraphStructure::from_u8(graph_structure)?;
    let from = from
        .iter()
        .map(|&idx| NodeIDX::from(idx))
        .collect::<Vec<NodeIDX>>();
    let to = NodeIDX::from(to);

    let offset_graph = match graph_structure {
        GraphStructure::Forward => &ag.edges_forward,
        GraphStructure::Dominator => ag.edges_dom(),
        GraphStructure::Reverse => &ag.derived_state.edges_reverse,
    };

    #[allow(clippy::collapsible_if)]
    if let Some(shortest_path) = offset_graph.shortest_path_configured(&from, to) {
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
pub fn get_reverse_edges_len(node_idxs: Vec<u32>) -> Result<Vec<usize>, WasmJSError> {
    let graph_state = GlobalState::graph_state().get();

    let result = node_idxs
        .into_iter()
        .map(|node_idx| {
            graph_state
                .array_graph
                .parents_len_configured(node_idx.into())
        })
        .collect::<Vec<usize>>();

    Ok(result)
}

#[wasm_bindgen]
pub fn get_transitive_count(node_idxs: Vec<u32>) -> Result<Vec<usize>, WasmJSError> {
    let graph_state = GlobalState::graph_state().get();

    let result = node_idxs
        .into_iter()
        .map(|node_idx| {
            graph_state
                .array_graph
                .transitive_count_configured(node_idx.into())
        })
        .collect::<Vec<usize>>();

    Ok(result)
}

#[wasm_bindgen]
pub fn get_transitive_count_dominated(node_idxs: Vec<u32>) -> Result<Vec<usize>, WasmJSError> {
    let graph_state = GlobalState::graph_state().get();

    let result = node_idxs
        .into_iter()
        .map(|node_idx| {
            graph_state
                .array_graph
                .transitive_count_configured_dominated(node_idx.into())
        })
        .collect::<Vec<usize>>();

    Ok(result)
}

#[wasm_bindgen]
pub fn get_all_reachable_node_idxs() -> Result<Vec<u32>, WasmJSError> {
    let graph_state = GlobalState::graph_state().get();
    let reachable_nodes = graph_state.array_graph.all_reachable_node_idxs();
    Ok(reachable_nodes.iter().map(|idx| idx.0).collect())
}

#[wasm_bindgen]
pub fn get_graph_traversal_config() -> Result<String, WasmJSError> {
    let traversal_config = GlobalState::graph_state()
        .get()
        .array_graph
        .traversal_config
        .clone()
        .unwrap_or_default();
    Ok(serde_json::to_string(&traversal_config).context("Failed to serialize traversal config")?)
}

#[wasm_bindgen]
pub fn get_graph_settings() -> Result<String, WasmJSError> {
    let graph_settings = GlobalState::graph_state()
        .get()
        .array_graph
        .graph_settings
        .clone()
        .unwrap_or_default();
    Ok(serde_json::to_string(&graph_settings).context("Failed to serialize Graph Settings")?)
}

#[wasm_bindgen]
pub fn apply_traversal_config(traversal_config_json: String) -> Result<(), WasmJSError> {
    let traversal_config: TraversalConfig =
        serde_json::from_str(&traversal_config_json).context("Failed to parse traversal config")?;
    GlobalState::graph_state_mut()
        .array_graph
        .apply_traversal_config(traversal_config)
        .context("Failed to apply traversal config")?;
    GlobalState::graph_state_mut().sync_node_attributes()?;
    Ok(())
}

#[wasm_bindgen]
pub fn get_graph_node_count() -> Result<usize, WasmJSError> {
    Ok(GlobalState::graph_state().get().array_graph.nodes_len())
}

#[wasm_bindgen]
pub fn determine_entrypoints() -> Result<Vec<u32>, WasmJSError> {
    let node_idxs = GlobalState::graph_state()
        .get()
        .array_graph
        .determine_entrypoints();
    Ok(node_idxs.iter().map(|idx| idx.0).collect())
}

#[wasm_bindgen]
pub fn get_array_graph_stats() -> Result<String, WasmJSError> {
    let stats = &GlobalState::graph_state().get().array_graph.stats();
    Ok(serde_json::to_string(&stats).context("Failed to serialize graph stats")?)
}

#[wasm_bindgen]
/// Takes a base64-encoded (url safe no pad) zstd compressed string and returns it.
/// MUST be a valid string (likely JSON) that can be converted to a UTF-8 string.
pub fn from_zstd_base64_url_safe_no_pad(zstd_base64: &str) -> Result<String, WasmJSError> {
    let bytes = serialization::from_zstd_base64_url_safe_no_pad(zstd_base64)
        .context("Failed to decode zstd base64 string (url safe, no pad)")?;

    let str = String::from_utf8(bytes).context("Failed to convert bytes to UTF-8 string")?;
    Ok(str)
}

#[wasm_bindgen]
pub fn to_zstd_base64_url_safe_no_pad(s: &str) -> Result<String, WasmJSError> {
    let r = serialization::to_zstd_base64_url_safe_no_pad(s.as_bytes(), 10)
        .context("Failed to compress string to zstd base64 URL-safe no pad")?;
    Ok(r)
}
