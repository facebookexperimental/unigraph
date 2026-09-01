// Copyright (c) Meta Platforms, Inc. and affiliates.

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
use unigraph_core::MapGraph;
use unigraph_db::UnigraphDb;
use unigraph_storage_core::BlobStorageMode;
use unigraph_storage_core::FullOrDeltaConfig;
use unigraph_storage_core::GraphID;
use unigraph_storage_core::GraphTimeKey;
use unigraph_storage_core::TimelineConfig;
use unigraph_storage_core::TimelineID;
use unigraph_storage_core::TimelineSchema;
use unigraph_storage_core::Timestamp;
use unigraph_storage_sqlite::SqliteStorage;

const THIS_FILES_DIR: &str = match option_env!("CARGO_MANIFEST_DIR") {
    Some(dir) => dir,
    None => ".",
};

/// Timeline the `--file-path` graphs are ingested under.
const LOCAL_TIMELINE_ID: &str = "local";
/// The `--left` comparison ("before") graph.
const LOCAL_LEFT_GRAPH_ID: GraphID = GraphID(0);
/// The `--file-path` primary ("after") graph.
const LOCAL_RIGHT_GRAPH_ID: GraphID = GraphID(1);

pub enum ServeMode {
    /// Proxy frontend requests to Vite dev server (React Router + HMR)
    Dev,
    /// Serve pre-built static files from React Router build output
    Release,
}

#[derive(Clone)]
struct AppState {
    db: Unigraph,
}

pub async fn start(
    graph_file_path: &Option<PathBuf>,
    comparison_file_path: &Option<PathBuf>,
    sqlite_path: &Path,
    mode: ServeMode,
    task: &ll::Task,
) -> Result<()> {
    let graphs = load_local_graphs(graph_file_path, comparison_file_path, task)?;
    let db = open_db(graphs.as_ref(), sqlite_path, task).await?;

    let state = AppState { db };

    let api = Router::new()
        .route("/favicon.ico", get(favicon_ico))
        .route("/favicon-192.png", get(favicon_png))
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
    info!(
        "Listening on http://{addr}{}",
        landing_path(graphs.as_ref())
    );
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

// --- RPC endpoint ---

async fn api_rpc(
    State(state): State<AppState>,
    axum::Json(req): axum::Json<UnigraphRequest>,
) -> Result<impl IntoResponse, http::StatusCode> {
    let task = ll::Task::create_new("api_rpc");
    let response = state
        .db
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

// --- Local (file-backed) graphs ---

/// Graphs passed on the command line with `--file-path` / `--left`.
struct LocalGraphs {
    /// The `--file-path` primary ("after") graph.
    right: ArrayGraphSerializable,
    /// The `--left` comparison ("before") graph, for delta view.
    left: Option<ArrayGraphSerializable>,
}

fn load_local_graphs(
    graph_file_path: &Option<PathBuf>,
    comparison_file_path: &Option<PathBuf>,
    task: &ll::Task,
) -> Result<Option<LocalGraphs>> {
    match (graph_file_path, comparison_file_path) {
        (Some(r), Some(l)) => Ok(Some(LocalGraphs {
            right: load_array_graph(r, task)?,
            left: Some(load_array_graph(l, task)?),
        })),
        (Some(r), None) => Ok(Some(LocalGraphs {
            right: load_array_graph(r, task)?,
            left: None,
        })),
        (None, None) => Ok(None),
        (None, Some(_)) => {
            bail!("Primary graph must be present if comparison graph is passed");
        }
    }
}

/// Build the database the RPC layer runs against.
///
/// Graphs passed with `--file-path` get a throwaway in-memory SQLite with those
/// graphs ingested as the `local` timeline. That way they resolve through the same
/// handle pipeline as stored timelines — traversal configs, deltas, node search and
/// shareable URLs all work — without writing scratch graphs into the user's real
/// database on every `serve`. The cost is that stored timelines are not reachable
/// in the same session, which already matched how file-backed serving behaved.
///
/// With no `--file-path`, the configured database is opened as-is and the UI lands
/// on the timeline list.
async fn open_db(
    graphs: Option<&LocalGraphs>,
    sqlite_path: &Path,
    task: &ll::Task,
) -> Result<Unigraph> {
    let Some(graphs) = graphs else {
        let sqlite = Arc::new(SqliteStorage::new(sqlite_path)?);
        return Ok(Unigraph::new(UnigraphDb::new(sqlite.clone(), sqlite)));
    };

    let sqlite = Arc::new(SqliteStorage::new_in_memory()?);
    let db = UnigraphDb::new(sqlite.clone(), sqlite);
    ingest_local_timeline(&db, graphs, task).await?;
    Ok(Unigraph::new(db))
}

/// Where to point the user on startup: straight at the ingested graphs when there
/// are any, otherwise the timeline list.
fn landing_path(graphs: Option<&LocalGraphs>) -> String {
    let Some(graphs) = graphs else {
        return "/".to_owned();
    };
    let right = format!("/{LOCAL_TIMELINE_ID}~{}", LOCAL_RIGHT_GRAPH_ID.0);
    match graphs.left {
        Some(_) => format!("/{LOCAL_TIMELINE_ID}~{}{right}", LOCAL_LEFT_GRAPH_ID.0),
        None => right,
    }
}

async fn ingest_local_timeline(
    db: &UnigraphDb,
    graphs: &LocalGraphs,
    task: &ll::Task,
) -> Result<()> {
    let timeline_id = TimelineID(LOCAL_TIMELINE_ID.to_owned());
    db.timelines
        .create(&timeline_id, &local_timeline_config(), task)
        .await?;

    // Both frames share a timestamp. `fetch_latest` orders by `(timestamp, graph_id)`
    // descending, so the tie breaks on graph_id and a bare `local` handle resolves to
    // the primary graph rather than the comparison one.
    let timestamp = Timestamp::now();
    let frame_key = |graph_id| GraphTimeKey {
        timeline_id: timeline_id.clone(),
        timestamp,
        graph_id,
    };

    if let Some(left) = &graphs.left {
        db.graph
            .store(&frame_key(LOCAL_LEFT_GRAPH_ID), left, None, task)
            .await?;
    }
    db.graph
        .store(&frame_key(LOCAL_RIGHT_GRAPH_ID), &graphs.right, None, task)
        .await?;

    Ok(())
}

/// `FullOrDelta` rather than `AdjacentDeltas`: the two local frames are unrelated
/// snapshots, and `AdjacentDeltas` would enforce a monotonic ordering they don't have.
fn local_timeline_config() -> TimelineConfig {
    TimelineConfig {
        schema: TimelineSchema::FullOrDelta(FullOrDeltaConfig {}),
        external_id_namespace: None,
        blob_storage: BlobStorageMode::Inline,
        store_metric_history: None,
    }
}

// --- Graph loading helpers ---

fn load_array_graph(p: &Path, task: &ll::Task) -> Result<ArrayGraphSerializable> {
    let file_string_content = std::fs::read_to_string(p).context("Failed to read file")?;
    let map_graph = MapGraph::from_json(&file_string_content).context("Failed to parse JSON")?;
    Ok(map_graph.to_array_graph(task)?.into_serializable())
}

#[cfg(test)]
mod tests {
    use k9::snapshot;
    use unigraph_app::GraphQueryMapGraphInput;
    use unigraph_app::UnigraphResponse;
    use unigraph_core::config_query::GraphQueryConfig;

    use super::*;

    /// `--file-path` graphs must be reachable through the ordinary handle pipeline —
    /// that is the whole point of ingesting them, and the frontend now has no other
    /// way to reach them.
    #[tokio::test]
    async fn local_files_resolve_through_handle_pipeline() -> Result<()> {
        let task = ll::Task::create_new("test");
        let dir = temp_dir("handle_pipeline")?;
        let right = write_graph(&dir, "right.json", &["A", "B", "C"])?;
        let left = write_graph(&dir, "left.json", &["A", "B"])?;

        let graphs = load_local_graphs(&Some(right), &Some(left), &task)?;
        let app = open_db(graphs.as_ref(), &dir.join("unused.sqlite"), &task).await?;

        // A bare `local` handle must land on the primary graph, not the comparison one.
        let mut rows = vec![format!("{:<8}  {:<8}  {}", "handle", "resolved", "nodes")];
        for handle in ["local", "local~0", "local~1"] {
            rows.push(resolve_row(&app, handle, &task).await?);
        }

        snapshot!(
            rows.join("\n"),
            "
handle    resolved  nodes
local     local~1   A,B,C
local~0   local~0   A,B
local~1   local~1   A,B,C
"
        );

        // The configured sqlite path must be untouched — scratch graphs never reach it.
        assert!(
            !dir.join("unused.sqlite").exists(),
            "serving local files must not create the configured database"
        );

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    /// The landing path is what `serve` prints and what the UI is expected to open.
    #[tokio::test]
    async fn landing_path_matches_what_was_passed() -> Result<()> {
        let task = ll::Task::create_new("test");
        let dir = temp_dir("landing_path")?;
        let right = write_graph(&dir, "right.json", &["A"])?;
        let left = write_graph(&dir, "left.json", &["B"])?;

        let rows = [
            ("no graphs", load_local_graphs(&None, &None, &task)?),
            (
                "primary only",
                load_local_graphs(&Some(right.clone()), &None, &task)?,
            ),
            (
                "primary + left",
                load_local_graphs(&Some(right), &Some(left), &task)?,
            ),
        ]
        .iter()
        .map(|(label, graphs)| format!("{label:<14}  {}", landing_path(graphs.as_ref())))
        .collect::<Vec<_>>();

        snapshot!(
            rows.join("\n"),
            "
no graphs       /
primary only    /local~1
primary + left  /local~0/local~1
"
        );

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    /// Without `--file-path` nothing is ingested — the configured db is opened as-is
    /// so the UI lands on the real timeline list.
    #[tokio::test]
    async fn no_graphs_opens_the_configured_db() -> Result<()> {
        let task = ll::Task::create_new("test");
        let dir = temp_dir("configured_db")?;
        let sqlite_path = dir.join("db.sqlite");

        let graphs = load_local_graphs(&None, &None, &task)?;
        assert!(graphs.is_none(), "no paths means no local graphs");

        let app = open_db(graphs.as_ref(), &sqlite_path, &task).await?;
        assert!(sqlite_path.exists(), "the configured database is opened");
        assert!(
            app.db.timelines.list(&task).await?.is_empty(),
            "a fresh db has no timelines — no test graph is ingested"
        );

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    // ── Helpers ──────────────────────────────────────────────────

    async fn resolve_row(app: &Unigraph, handle: &str, task: &ll::Task) -> Result<String> {
        let req = UnigraphRequest::GraphQueryMapGraph(GraphQueryMapGraphInput {
            query: GraphQueryConfig {
                handle: handle.parse()?,
                roots: None,
                traversal: None,
            },
        });
        let out = match app.exec_rpc(req, task).await? {
            UnigraphResponse::GraphQueryMapGraph(out) => out,
            other => bail!("unexpected response: {}", other.variant_name()),
        };
        let names: Vec<&str> = out.map_graph.nodes.keys().map(|n| n.as_str()).collect();
        Ok(format!(
            "{:<8}  {:<8}  {}",
            handle,
            out.graph_key,
            names.join(",")
        ))
    }

    fn temp_dir(name: &str) -> Result<PathBuf> {
        let dir = std::env::temp_dir().join(format!("unigraph_web_service_{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// Write a graph file whose nodes form a simple chain, so each file is
    /// distinguishable purely by its node set.
    fn write_graph(dir: &Path, file_name: &str, nodes: &[&str]) -> Result<PathBuf> {
        let entries: Vec<String> = nodes
            .iter()
            .enumerate()
            .map(|(i, n)| {
                let edges = match nodes.get(i + 1) {
                    Some(next) => format!("[\"{next}\"]"),
                    None => "[]".to_owned(),
                };
                format!(
                    "\"{n}\": {{ \"edges_directed\": {edges}, \"size\": {} }}",
                    i + 1
                )
            })
            .collect();

        let path = dir.join(file_name);
        std::fs::write(&path, format!("{{\"nodes\": {{{}}}}}", entries.join(",")))?;
        Ok(path)
    }
}
