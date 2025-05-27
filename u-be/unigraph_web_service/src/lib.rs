// Copyright (c) Meta Platforms, Inc. and affiliates.

use anyhow::Context;
use anyhow::Result;
use axum::Router;
use axum::response::Html;
use axum::routing::get;
use tera::Tera;

const HTML_TEMLPATE_PATH: &str = "../../u-fe/index.html.tera";
const THIS_FILES_DIR: &str = env!("CARGO_MANIFEST_DIR");

pub async fn start(graphite_graph_json_file_path: &Option<String>) -> Result<()> {
    let map_graph_json = if let Some(file_path) = graphite_graph_json_file_path {
        let file_string_content =
            std::fs::read_to_string(file_path).context("Failed to read file")?;
        unigraph_core::GraphiteGraph::from_json(&file_string_content)
            .context("Failed to parse JSON")?
            .into_map_graph()?
            .to_json()?
    } else {
        unigraph_core::make_test_graph()?.to_json()?
    };

    // build our application with a single route
    let app = Router::new().route(
        "/",
        get(move || async move { Html(html(&map_graph_json).unwrap()) }),
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
    println!("Listening on http://{}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}

fn html(map_graph_json: &str) -> Result<String> {
    let html_path = format!("{}/{}", THIS_FILES_DIR, HTML_TEMLPATE_PATH);

    let css_path = format!("{}/{}", THIS_FILES_DIR, "../../u-fe/output.css");
    let js_path = format!("{}/{}", THIS_FILES_DIR, "../../.build/index.js");

    let html_template = read_str(&html_path)?;
    let css = read_str(&css_path)?;
    let js = read_str(&js_path)?;

    let mut context = tera::Context::new();
    context.insert("css", &css);
    context.insert("js", &js.replace("</script>", "<\\/script>"));
    context.insert("map_graph_json", map_graph_json);
    Tera::one_off(&html_template, &context, false).context("Failed to render HTML template")
}

fn read_str(path: &str) -> Result<String> {
    std::fs::read_to_string(path).with_context(|| format!("Failed to read file at `{}`", path))
}
