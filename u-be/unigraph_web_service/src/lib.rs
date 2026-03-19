// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::any::type_name;
use std::path::Path;
use std::path::PathBuf;
use std::process::Child;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use axum::Router;
use axum::body::Body;
use axum::extract::State;
use axum::http;
use axum::response::IntoResponse;
use axum::response::Response;
use axum::routing::get;
use axum::routing::post;
use serde::Deserialize;
use serde::Serialize;
use tower_http::services::ServeDir;
use tower_http::services::ServeFile;
use tower_http::trace::TraceLayer;
use tracing::Span;
use tracing::info;
use unigraph_app::Unigraph;
use unigraph_core::ArrayGraphSerializable;
use unigraph_core::ArrayGraphSerializablePackage;
use unigraph_core::ArrayGraphSerializablePackageConfig;
use unigraph_core::GraphQueryConfig;
use unigraph_core::MapGraph;
use unigraph_core::config_key::GraphQueryConfigKey;
use unigraph_core::ui_types::ExplorerComponentInputGraph;
use unigraph_serialization::SerializationFormat;
use unigraph_storage_core::FrameType;
use unigraph_storage_core::GraphID;
use unigraph_storage_core::GraphKey;
use unigraph_storage_core::GraphKeyOrTimelineID;
use unigraph_storage_core::TimelineID;

const THIS_FILES_DIR: &str = match option_env!("CARGO_MANIFEST_DIR") {
    Some(dir) => dir,
    None => ".",
};

pub enum ServeMode {
    /// Proxy frontend requests to Vite dev server (React Router + HMR)
    Dev,
    /// Serve pre-built static files from React Router build output
    Release,
}

#[derive(Clone)]
struct AppState {
    left_graph: Arc<String>,
    right_graph: Arc<Option<String>>,
    db: Option<Unigraph>,
}

pub async fn start(
    graphite_graph_json_file_path_left: &Option<PathBuf>,
    graphite_graph_json_file_path_right: &Option<PathBuf>,
    sqlite_path: &Option<PathBuf>,
    mode: ServeMode,
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

    let db = match sqlite_path {
        Some(path) => {
            let sqlite = Arc::new(unigraph_storage_sqlite::SqliteStorage::new(path)?);
            let db = unigraph_db::UnigraphDb::new(sqlite.clone(), sqlite);
            Some(Unigraph::new(db))
        }
        None => None,
    };

    let state = AppState {
        left_graph: Arc::new(left_graph),
        right_graph: Arc::new(right_graph),
        db,
    };

    let api = Router::new()
        .route("/favicon.ico", get(favicon_ico))
        .route("/favicon-192.png", get(favicon_png))
        .route("/api/local_graphs", get(api_local_graphs))
        .route("/api/graph_query", post(api_graph_query))
        .route("/api/timelines", get(api_timelines))
        .route(
            "/api/timelines/{timeline_id}/frames",
            get(api_timeline_frames),
        )
        .route(
            "/api/timelines/{timeline_id}/graphs/{graph_id}",
            get(api_timeline_graph),
        )
        .with_state(state);

    let project_root = PathBuf::from(THIS_FILES_DIR).join("../..");

    let app = match mode {
        ServeMode::Dev => {
            let vite = start_vite(&project_root)?;
            wait_for_vite(5173).await?;
            info!("Vite dev server is ready");

            // Keep the vite process alive as long as the server runs.
            // The Drop impl kills it on shutdown.
            let vite_guard = Arc::new(vite);

            let app = api.fallback(proxy_to_vite);

            // Spawn a task to keep the guard alive until ctrl-c
            let guard = vite_guard.clone();
            tokio::spawn(async move {
                tokio::signal::ctrl_c().await.ok();
                drop(guard);
            });

            app
        }
        ServeMode::Release => {
            let build_dir = project_root.join("build/client");
            if !build_dir.exists() {
                bail!(
                    "Build directory not found at {}. Run `npx react-router build` first.",
                    build_dir.display()
                );
            }

            let index_html = build_dir.join("index.html");
            let serve_dir = ServeDir::new(&build_dir).fallback(ServeFile::new(&index_html));

            api.fallback_service(serve_dir)
        }
    };

    // NOTE: it has to be `localhost` otherwise wgpu will blow up because of the unsecure
    // context.
    let addr = "localhost:3000";

    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("Listening on http://{addr}");
    let trace_layer = TraceLayer::new_for_http()
        .make_span_with(|req: &http::Request<Body>| {
            tracing::info_span!("req", method = %req.method(), uri = %req.uri())
        })
        .on_response(|resp: &http::Response<Body>, latency: Duration, _span: &Span| {
            info!(status = resp.status().as_u16(), latency_ms = latency.as_millis(), "done");
        });

    axum::serve(listener, app.layer(trace_layer)).await?;
    Ok(())
}

// --- Favicons (embedded at compile time) ---

const FAVICON_ICO: &[u8] = include_bytes!("favicon.ico");
const FAVICON_PNG: &[u8] = include_bytes!("favicon-192.png");

async fn favicon_ico() -> Response {
    ([(http::header::CONTENT_TYPE, "image/x-icon")], FAVICON_ICO).into_response()
}

async fn favicon_png() -> Response {
    ([(http::header::CONTENT_TYPE, "image/png")], FAVICON_PNG).into_response()
}

// --- File-based graph endpoint ---

async fn api_local_graphs(State(state): State<AppState>) -> impl IntoResponse {
    let mut body = format!(r#"{{"left":{}"#, *state.left_graph);
    if let Some(ref right) = *state.right_graph {
        body.push_str(&format!(r#","right":{right}"#));
    }
    body.push('}');
    ([(http::header::CONTENT_TYPE, "application/json")], body)
}

// --- Graph query endpoint ---

#[derive(Deserialize)]
struct GraphQueryRequest {
    graph_query_config: Option<GraphQueryConfig>,
    graph_query_config_key: Option<String>,
}

#[derive(Serialize)]
struct GraphQueryResponse {
    graph: serde_json::Value,
    graph_query_config: GraphQueryConfig,
}

async fn api_graph_query(
    State(state): State<AppState>,
    axum::Json(req): axum::Json<GraphQueryRequest>,
) -> Result<impl IntoResponse, http::StatusCode> {
    let app = state.db.as_ref().ok_or(http::StatusCode::NOT_FOUND)?;
    let task = ll::Task::create_new("api_graph_query");

    let gqc = resolve_graph_query_config(app, &req, &task).await?;
    let (graph_json, gqc) = fetch_graph_for_gqc(app, gqc, &task).await?;

    let graph_value: serde_json::Value =
        serde_json::from_str(&graph_json).map_err(|_| http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let response = GraphQueryResponse {
        graph: graph_value,
        graph_query_config: gqc,
    };

    let json =
        serde_json::to_string(&response).map_err(|_| http::StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(([(http::header::CONTENT_TYPE, "application/json")], json))
}

async fn resolve_graph_query_config(
    app: &Unigraph,
    req: &GraphQueryRequest,
    task: &ll::Task,
) -> Result<GraphQueryConfig, http::StatusCode> {
    match (&req.graph_query_config, &req.graph_query_config_key) {
        (Some(gqc), _) => Ok(gqc.clone()),
        (_, Some(key_str)) => {
            let key: GraphQueryConfigKey =
                key_str.parse().map_err(|_| http::StatusCode::BAD_REQUEST)?;
            app.db
                .configs
                .fetch_graph_query_config(&key, task)
                .await
                .map_err(|_| http::StatusCode::INTERNAL_SERVER_ERROR)
        }
        (None, None) => Err(http::StatusCode::BAD_REQUEST),
    }
}

async fn fetch_graph_for_gqc(
    app: &Unigraph,
    mut gqc: GraphQueryConfig,
    task: &ll::Task,
) -> Result<(String, GraphQueryConfig), http::StatusCode> {
    let handle = gqc.handle.as_ref().ok_or(http::StatusCode::BAD_REQUEST)?;
    let parsed: GraphKeyOrTimelineID = handle.parse().map_err(|_| http::StatusCode::BAD_REQUEST)?;

    let graph = match parsed {
        GraphKeyOrTimelineID::GraphKey(key) => {
            let frame = app
                .db
                .frames
                .get(&key, false, task)
                .await
                .map_err(|_| http::StatusCode::INTERNAL_SERVER_ERROR)?
                .ok_or(http::StatusCode::NOT_FOUND)?;

            if frame.frame_type == FrameType::Empty || frame.frame_type == FrameType::Error {
                return Err(http::StatusCode::NOT_FOUND);
            }

            app.db
                .graph
                .fetch(&key, task)
                .await
                .map_err(|_| http::StatusCode::INTERNAL_SERVER_ERROR)?
        }
        GraphKeyOrTimelineID::TimelineID(tid) => {
            let (_key, graph) = app
                .db
                .graph
                .fetch_latest(&tid, task)
                .await
                .map_err(|_| http::StatusCode::INTERNAL_SERVER_ERROR)?;
            graph
        }
    };

    // If the GQC has no TVC, populate from the graph's embedded config
    if gqc.traversal_config.is_none() {
        gqc.traversal_config = graph.traversal_config.clone();
    }

    let graph_json =
        array_graph_to_json(&graph).map_err(|_| http::StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((graph_json, gqc))
}

// --- Storage-backed endpoints ---

#[derive(Serialize)]
struct TimelineResponse {
    timeline_id: String,
}

async fn api_timelines(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, http::StatusCode> {
    let app = state.db.as_ref().ok_or(http::StatusCode::NOT_FOUND)?;
    let task = ll::Task::create_new("api_timelines");
    let timelines = app
        .db
        .timelines
        .list(&task)
        .await
        .map_err(|_| http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let response: Vec<TimelineResponse> = timelines
        .into_iter()
        .map(|tl| TimelineResponse { timeline_id: tl.0 })
        .collect();

    let json =
        serde_json::to_string(&response).map_err(|_| http::StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(([(http::header::CONTENT_TYPE, "application/json")], json))
}

#[derive(Serialize)]
struct FrameResponse {
    graph_id: i64,
    timestamp: String,
    frame_type: String,
    base: Option<i64>,
}

async fn api_timeline_frames(
    State(state): State<AppState>,
    axum::extract::Path(timeline_id): axum::extract::Path<String>,
) -> Result<impl IntoResponse, http::StatusCode> {
    let app = state.db.as_ref().ok_or(http::StatusCode::NOT_FOUND)?;
    let task = ll::Task::create_new("api_timeline_frames");
    let frames = app
        .db
        .frames
        .list(&TimelineID(timeline_id), &task)
        .await
        .map_err(|_| http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let response: Vec<FrameResponse> = frames
        .iter()
        .map(|f| {
            let base = f.base.as_ref().map(|k| k.graph_id.0);
            FrameResponse {
                graph_id: f.frame.graph_id.0,
                timestamp: f.frame.timestamp.to_rfc3339(),
                frame_type: f.frame_type.to_string(),
                base,
            }
        })
        .collect();

    let json =
        serde_json::to_string(&response).map_err(|_| http::StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(([(http::header::CONTENT_TYPE, "application/json")], json))
}

#[derive(Deserialize)]
struct TimelineGraphPath {
    timeline_id: String,
    graph_id: i64,
}

async fn api_timeline_graph(
    State(state): State<AppState>,
    axum::extract::Path(path): axum::extract::Path<TimelineGraphPath>,
) -> Result<impl IntoResponse, http::StatusCode> {
    let app = state.db.as_ref().ok_or(http::StatusCode::NOT_FOUND)?;
    let key = GraphKey {
        timeline_id: TimelineID(path.timeline_id),
        graph_id: GraphID(path.graph_id),
    };

    let task = ll::Task::create_new("api_timeline_graph");
    let frame = app
        .db
        .frames
        .get(&key, false, &task)
        .await
        .map_err(|_| http::StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(http::StatusCode::NOT_FOUND)?;

    if frame.frame_type == FrameType::Empty || frame.frame_type == FrameType::Error {
        return Err(http::StatusCode::NOT_FOUND);
    }

    let graph = app
        .db
        .graph
        .fetch(&key, &task)
        .await
        .map_err(|_| http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let json = array_graph_to_json(&graph).map_err(|_| http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let body = format!(r#"{{"left":{json}}}"#);
    Ok(([(http::header::CONTENT_TYPE, "application/json")], body))
}

// --- Vite proxy ---

async fn proxy_to_vite(req: axum::extract::Request) -> Result<impl IntoResponse, http::StatusCode> {
    use hyper_util::client::legacy::Client;
    use hyper_util::rt::TokioExecutor;

    let client = Client::builder(TokioExecutor::new()).build_http::<Body>();

    let uri = format!("http://localhost:5173{}", req.uri());
    let uri: http::Uri = uri.parse().map_err(|_| http::StatusCode::BAD_REQUEST)?;

    let (parts, body) = req.into_parts();
    let mut proxy_req = http::Request::from_parts(parts, body);
    *proxy_req.uri_mut() = uri;
    // Remove the Host header so hyper can set it correctly for the upstream
    proxy_req.headers_mut().remove(http::header::HOST);

    let resp = client
        .request(proxy_req)
        .await
        .map_err(|_| http::StatusCode::BAD_GATEWAY)?;

    let (parts, body) = resp.into_parts();
    Ok(axum::response::Response::from_parts(parts, Body::new(body)))
}

// --- Vite process management ---

struct ViteProcess(Child);

impl Drop for ViteProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn start_vite(project_root: &Path) -> Result<ViteProcess> {
    let vite_bin = project_root.join("node_modules/.bin/vite");
    info!("Starting Vite dev server...");
    let child = Command::new(&vite_bin)
        .current_dir(project_root)
        .spawn()
        .with_context(|| format!("Failed to start Vite at {}", vite_bin.display()))?;
    Ok(ViteProcess(child))
}

async fn wait_for_vite(port: u16) -> Result<()> {
    let addr = format!("localhost:{port}");
    for _ in 0..200 {
        if tokio::net::TcpStream::connect(&addr).await.is_ok() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    bail!("Vite dev server did not start within 20 seconds")
}

// --- Graph serialization helpers ---

fn array_graph_to_json(ag: &ArrayGraphSerializable) -> Result<String> {
    let package_base64 = ag
        .pack(&ArrayGraphSerializablePackageConfig::default())?
        .into_base_64();
    let serialized_str = SerializationFormat::Json.to_serialized_str(
        &package_base64,
        Some(type_name::<ArrayGraphSerializablePackage>().into()),
    )?;

    SerializationFormat::Json
        .to_string(&ExplorerComponentInputGraph::ArrayGraphSerializedPackageBase64(serialized_str))
}

fn to_serialized_str_json(map_graph: &MapGraph) -> Result<String> {
    let ag = map_graph.to_array_graph()?.into_serializable();
    array_graph_to_json(&ag)
}

fn into_array_graph_json(p: &Path) -> Result<String> {
    let file_string_content = std::fs::read_to_string(p).context("Failed to read file")?;
    let map_graph =
        unigraph_core::MapGraph::from_json(&file_string_content).context("Failed to parse JSON")?;
    to_serialized_str_json(&map_graph)
}
