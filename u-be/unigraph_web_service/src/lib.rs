// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::any::type_name;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use axum::Router;
use axum::response::Html;
use axum::routing::get;
use tera::Tera;
use unigraph_core::MapGraph;
use unigraph_core::ui_types::ExplorerComponentInputGraph;
use unigraph_serialization::SerializationFormat;
use unigraph_storage::ArrayGraphSerializablePackage;
use unigraph_storage::StorageConfig;
use unigraph_storage::pack;

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
        (Some(l), Some(r)) => (into_array_graph_json(l)?, Some(into_array_graph_json(r)?)),
        (Some(l), None) => (into_array_graph_json(l)?, None),
        (None, None) => (
            to_serialized_str_json(&unigraph_core::make_test_graph()?)?,
            None,
        ),
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

fn html(left_json: &str, right_json: &Option<String>) -> Result<String> {
    let html_path = format!("{THIS_FILES_DIR}/{HTML_TEMLPATE_PATH}");

    let js_path = format!("{}/{}", THIS_FILES_DIR, "../../.build/index.js");
    let css_path = format!("{}/{}", THIS_FILES_DIR, "../../.build/output.css");

    let css = read_str(&css_path)?;
    let html_template = read_str(&html_path)?;
    let js = read_str(&js_path)?;

    let empty_json = "".to_string();
    let right_json = right_json.as_ref().unwrap_or(&empty_json);

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

fn to_serialized_str_json(map_graph: &MapGraph) -> Result<String> {
    let ag = map_graph.to_array_graph()?.into_serializable();
    let package = pack(&ag, &StorageConfig::default())?;
    let serialized_str = SerializationFormat::Json.to_serialized_str(
        &package,
        Some(type_name::<ArrayGraphSerializablePackage>().into()),
    )?;

    SerializationFormat::Json.to_string(&ExplorerComponentInputGraph::ArrayGraphSerializedPackage(
        serialized_str,
    ))
}

fn into_array_graph_json(p: &Path) -> Result<String> {
    let file_string_content = std::fs::read_to_string(p).context("Failed to read file")?;
    let map_graph =
        unigraph_core::MapGraph::from_json(&file_string_content).context("Failed to parse JSON")?;
    to_serialized_str_json(&map_graph)
}
