// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use axum::Router;
use axum::response::Html;
use axum::routing::get;
use base64::Engine;
use tera::Tera;
use unigraph_core::MapGraph;

const HTML_TEMLPATE_PATH: &str = "../../u-fe/index.html.tera";
const THIS_FILES_DIR: &str = env!("CARGO_MANIFEST_DIR");

pub async fn start(
    graphite_graph_json_file_path_left: &Option<PathBuf>,
    graphite_graph_json_file_path_right: &Option<PathBuf>,
) -> Result<()> {
    let (left_graph, right_graph) = match (
        &graphite_graph_json_file_path_left,
        &graphite_graph_json_file_path_right,
    ) {
        (Some(l), Some(r)) => (parse_graph(l)?, Some(parse_graph(r)?)),
        (Some(l), None) => (parse_graph(l)?, None),
        (None, None) => (unigraph_core::make_test_graph()?, None),
        (None, Some(_)) => {
            bail!("Left graph must be present if right graph is passed");
        }
    };

    let left_graph = Arc::new(left_graph);
    let right_graph = Arc::new(right_graph);

    // build our application with a single route
    let app = Router::new().route(
        "/",
        get(move || {
            let left_graph = Arc::clone(&left_graph);
            let right_graph = Arc::clone(&right_graph);
            async move { Html(html(&left_graph, &right_graph).unwrap()) }
        }),
    );

    // NOTE: it has to be `localhost` otherwise wgpu will blow up because of the unsecure
    // context.
    // Error looks like:
    //      Failed to find an appropriate adapter: NotFound { active_backends: Backends(BROWSER_WEBGPU),
    //      requested_backends: Backends(NOOP | VULKAN | GL | METAL | DX12 | BROWSER_WEBGPU),
    //      supported_backends: Backends(BROWSER_WEBGPU), no_fallback_backends: Backends(0x0),
    //      no_adapter_backends: Backends(BROWSER_WEBGPU), incompatible_surface_backends: Backends(0x0) }
    let addr = "localhost:3000";

    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("Listening on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

fn html(left: &MapGraph, right: &Option<MapGraph>) -> Result<String> {
    let html_path = format!("{THIS_FILES_DIR}/{HTML_TEMLPATE_PATH}");

    let js_path = format!("{}/{}", THIS_FILES_DIR, "../../.build/index.js");
    let css_path = format!("{}/{}", THIS_FILES_DIR, "../../.build/output.css");

    let css = read_str(&css_path)?;
    let html_template = read_str(&html_path)?;
    let js = read_str(&js_path)?;

    let left_json = to_array_graph_json_zstd_base64(left)?;
    let right_json = if let Some(right_graph) = right {
        to_array_graph_json_zstd_base64(right_graph)?
    } else {
        "".to_string()
    };

    let mut context = tera::Context::new();
    context.insert("css", &css);
    context.insert("js", &js.replace("</script>", "<\\/script>"));
    context.insert("array_graph_json_zstd_base64_left", &left_json);
    context.insert("array_graph_json_zstd_base64_right", &right_json);
    Tera::one_off(&html_template, &context, false).context("Failed to render HTML template")
}

fn read_str(path: &str) -> Result<String> {
    std::fs::read_to_string(path).with_context(|| format!("Failed to read file at `{path}`"))
}

fn to_array_graph_json_zstd_base64(map_graph: &MapGraph) -> Result<String> {
    let json = map_graph.to_array_graph()?.into_serializable().to_json()?;
    let compressed = zstd::encode_all(json.as_bytes(), 18)?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(compressed);
    Ok(b64)
}

fn parse_graph(p: &Path) -> Result<MapGraph> {
    let file_string_content = std::fs::read_to_string(p).context("Failed to read file")?;
    unigraph_core::MapGraph::from_json(&file_string_content).context("Failed to parse JSON")
}
