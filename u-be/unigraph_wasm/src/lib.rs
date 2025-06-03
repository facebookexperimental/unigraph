// Copyright (c) Meta Platforms, Inc. and affiliates.

mod wasm_error;

use std::vec;

use anyhow::Context;
use anyhow::Result;
use log::trace;
use unigraph_core::MapGraph;
use unigraph_core::TraversalConfig;
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
pub fn set_graph(graph_json: Option<String>) -> Result<(), WasmJSError> {
    let array_graph = if let Some(graph_json) = graph_json {
        MapGraph::from_json(&graph_json)
            .unwrap()
            .to_array_graph()
            .unwrap()
    } else {
        log::info!("No graph provided, using test graph");
        make_test_graph().unwrap().to_array_graph().unwrap()
    };
    GlobalState::graph_state().replace_graph(array_graph);
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

#[wasm_bindgen]
pub fn get_selected_node_idxs() -> Result<Vec<usize>, WasmJSError> {
    Ok(unigraph_wgpu::get_selected_node_idxs()?)
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
pub fn get_arrows_forward(node_idx: usize) -> Result<String, WasmJSError> {
    let graph_state = GlobalState::graph_state().get();
    let edges = graph_state
        .array_graph
        .get_arrows_forward(NodeIDX::from(node_idx))
        .context("Failed to get arrows")?;

    Ok(serde_json::to_string(&edges).context("Failed to serialize arrows")?)
}

#[wasm_bindgen]
pub fn apply_traversal_config(traversal_config_json: String) -> Result<(), WasmJSError> {
    let traversal_config: TraversalConfig =
        serde_json::from_str(&traversal_config_json).context("Failed to parse traversal config")?;
    GlobalState::graph_state_mut()
        .array_graph
        .apply_traversal_config(&traversal_config)
        .context("Failed to apply traversal config")?;
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
