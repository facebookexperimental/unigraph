// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::any::type_name;
use std::io::BufRead;
use std::io::BufReader;
use std::path::Path;
use std::path::PathBuf;
use std::process::Child;
use std::process::Command;
use std::process::Stdio;
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
use tower_http::services::ServeDir;
use tower_http::services::ServeFile;
use tower_http::trace::TraceLayer;
use tracing::Span;
use tracing::info;
use tracing::warn;
use unigraph_app::Unigraph;
use unigraph_app::UnigraphRequest;
use unigraph_core::ArrayGraphSerializable;
use unigraph_core::ArrayGraphSerializablePackage;
use unigraph_core::ArrayGraphSerializablePackageConfig;
use unigraph_core::MapGraph;
use unigraph_core::ui_types::ExplorerComponentInputGraph;
use unigraph_serialization::SerializationFormat;

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
        .route("/api/rpc", post(api_rpc))
        .with_state(state);

    let project_root = PathBuf::from(THIS_FILES_DIR).join("../..");

    // Hold the Vite child process (if any) so it lives until the server exits.
    // Dropped after axum::serve returns, which triggers graceful Vite shutdown.
    let _vite_guard: Option<ViteProcess>;

    let app = match mode {
        ServeMode::Dev => {
            let vite = start_vite(&project_root)?;
            wait_for_vite(5173).await?;
            info!("Vite dev server is ready");

            _vite_guard = Some(vite);
            api.fallback(proxy_to_vite)
        }
        ServeMode::Release => {
            _vite_guard = None;

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

    axum::serve(listener, app.layer(trace_layer))
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    // _vite_guard is dropped here, cleanly killing the Vite process.
    Ok(())
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to listen for ctrl-c");
    info!("Shutting down...");
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
    let mut body = format!(r#"{{"right":{}"#, *state.left_graph);
    if let Some(ref right) = *state.right_graph {
        body.push_str(&format!(r#","left":{right}"#));
    }
    body.push('}');
    ([(http::header::CONTENT_TYPE, "application/json")], body)
}

// --- RPC endpoint ---

async fn api_rpc(
    State(state): State<AppState>,
    axum::Json(req): axum::Json<UnigraphRequest>,
) -> Result<impl IntoResponse, http::StatusCode> {
    let app = state.db.as_ref().ok_or(http::StatusCode::NOT_FOUND)?;
    let task = ll::Task::create_new("api_rpc");
    let response = app
        .exec_rpc(req, &task)
        .await
        .map_err(|_| http::StatusCode::INTERNAL_SERVER_ERROR)?;
    let json =
        serde_json::to_string(&response).map_err(|_| http::StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(([(http::header::CONTENT_TYPE, "application/json")], json))
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
        info!("Stopping Vite dev server...");

        // Vite shares our process group, so it already received SIGINT from
        // Ctrl-C. Give it a moment to exit gracefully before force-killing.
        for _ in 0..20 {
            if self.0.try_wait().ok().flatten().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        // Fallback: hard kill.
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn start_vite(project_root: &Path) -> Result<ViteProcess> {
    let vite_bin = project_root.join("node_modules/.bin/vite");
    info!("Starting Vite dev server...");
    let mut child = Command::new(&vite_bin)
        .current_dir(project_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("Failed to start Vite at {}", vite_bin.display()))?;

    pipe_output(child.stdout.take(), false);
    pipe_output(child.stderr.take(), true);

    Ok(ViteProcess(child))
}

/// Spawn a background thread that reads lines from a stdio stream and logs them.
fn pipe_output<R: std::io::Read + Send + 'static>(stream: Option<R>, is_err: bool) {
    let Some(stream) = stream else { return };
    std::thread::spawn(move || {
        let reader = BufReader::new(stream);
        for line in reader.lines() {
            match line {
                Ok(line) if line.is_empty() => {}
                Ok(line) => {
                    if is_err {
                        warn!(target: "vite", "{line}");
                    } else {
                        info!(target: "vite", "{line}");
                    }
                }
                Err(_) => break,
            }
        }
    });
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
    let task = ll::Task::create_new("");
    let package_base64 = ag
        .pack(&ArrayGraphSerializablePackageConfig::default(), &task)?
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
