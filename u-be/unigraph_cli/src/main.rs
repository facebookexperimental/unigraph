// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use clap::Parser;
use clap::Subcommand;
use crossterm::tty::IsTty;
use unigraph_core::BudgetConfig;
use unigraph_core::build_budget_graph;
use unigraph_storage_core::GraphKeyOrTimelineID;
use unigraph_storage_core::TimelineID;
use unigraph_web_service::ServeMode;

fn default_sqlite_path() -> PathBuf {
    dirs::home_dir()
        .expect("could not determine home directory")
        .join(".unigraph")
        .join("sqlite")
}

fn resolve_sqlite_path(path: &Path) -> &Path {
    if *path == default_sqlite_path() {
        eprintln!("Using database: {}", path.display());
    }
    path
}

#[derive(Parser)]
#[command(long_about = None)]
struct Args {
    /// Path to the SQLite database file
    #[arg(long, default_value_os_t = default_sqlite_path(), global = true)]
    sqlite_path: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Serve(Serve),
    Ingest(Ingest),
    Frames(Frames),
    Compact(Compact),
    Graph(Graph),
}

#[derive(Parser)]
struct Serve {
    /// Path to the graph file to visualize
    #[arg(short, long)]
    file_path: Option<PathBuf>,

    /// Path to the second graph file that will be used
    /// to compare (delta view) with the main graph
    #[arg(short, long)]
    right: Option<PathBuf>,

    /// Serve pre-built static files instead of proxying to Vite dev server
    #[arg(long)]
    release: bool,
}

impl Serve {
    async fn run(&self, sqlite_path: &Path) {
        let mode = if self.release {
            ServeMode::Release
        } else {
            ServeMode::Dev
        };
        let sqlite_path = Some(sqlite_path.to_path_buf());
        unigraph_web_service::start(&self.file_path, &self.right, &sqlite_path, mode)
            .await
            .unwrap();
    }
}

#[derive(Parser)]
struct Ingest {
    /// Path to the ingestion config file
    #[arg(long)]
    config: PathBuf,

    /// Maximum number of new commits to ingest (for testing)
    #[arg(long)]
    limit: Option<usize>,
}

impl Ingest {
    async fn run(&self, sqlite_path: &Path, task: &ll::Task) -> anyhow::Result<()> {
        let path = resolve_sqlite_path(sqlite_path);
        let sqlite = Arc::new(unigraph_storage_sqlite::SqliteStorage::new(path)?);
        let db = unigraph_db::UnigraphDb::new(sqlite.clone(), sqlite);

        let config_file = &self.config;
        let configs = unigraph_ingestion::load_ingestion_configs(config_file)?;

        eprintln!(
            "Loaded {} ingestion config(s) from {}",
            configs.len(),
            config_file.display()
        );

        let options = unigraph_ingestion::IngestionOptions { limit: self.limit };

        for (i, ingestion_config) in configs.iter().enumerate() {
            eprintln!("\n=== Ingestion config {}/{} ===", i + 1, configs.len());
            run_one_config(ingestion_config, &db, &options, task).await?;
        }

        Ok(())
    }
}

async fn run_one_config(
    config: &unigraph_ingestion::IngestionConfig,
    db: &unigraph_db::UnigraphDb,
    options: &unigraph_ingestion::IngestionOptions,
    task: &ll::Task,
) -> anyhow::Result<()> {
    // Resolve the source
    let source = resolve_source(&config.source, db, task).await?;

    // Resolve and own the builders
    let mut cargo_builders = Vec::new();
    let mut budget_builders = Vec::new();

    // Track which builder type and index for each timeline entry
    enum BuilderRef {
        Cargo(usize),
        Budget(usize),
    }
    let mut builder_refs = Vec::new();
    let mut timeline_ids = Vec::new();

    for entry in &config.timelines {
        timeline_ids.push(TimelineID(entry.timeline_id.clone()));
        match &entry.builder {
            unigraph_ingestion::GraphBuilderConfig::Cargo {
                manifest_path,
                collect_timings,
                collect_sizes,
            } => {
                let idx = cargo_builders.len();
                cargo_builders.push(unigraph_ingestion::CargoGraphBuilder::new(
                    PathBuf::from(manifest_path),
                    *collect_timings,
                    *collect_sizes,
                ));
                builder_refs.push(BuilderRef::Cargo(idx));
            }
            unigraph_ingestion::GraphBuilderConfig::BudgetGraph { budget_config } => {
                let idx = budget_builders.len();
                budget_builders.push(unigraph_ingestion::BudgetGraphBuilder {
                    name: entry.timeline_id.clone(),
                    budget_config: budget_config.as_deref().cloned(),
                });
                builder_refs.push(BuilderRef::Budget(idx));
            }
        }
    }

    // Build the pipeline config with references into the owned builders
    let builders: Vec<unigraph_ingestion::TimelineBuilderConfig<'_>> = builder_refs
        .iter()
        .enumerate()
        .map(|(i, bref)| {
            let builder = match bref {
                BuilderRef::Cargo(idx) => {
                    unigraph_ingestion::Builder::FromRepo(&cargo_builders[*idx])
                }
                BuilderRef::Budget(idx) => {
                    unigraph_ingestion::Builder::BudgetGraph(&budget_builders[*idx])
                }
            };
            unigraph_ingestion::TimelineBuilderConfig {
                timeline_id: timeline_ids[i].clone(),
                builder,
            }
        })
        .collect();

    let pipeline_config = unigraph_ingestion::IngestionPipelineConfig { source, builders };

    unigraph_ingestion::run_ingestion(&pipeline_config, db, options).await
}

async fn resolve_source(
    source_config: &unigraph_ingestion::IngestionSourceConfig,
    db: &unigraph_db::UnigraphDb,
    task: &ll::Task,
) -> anyhow::Result<unigraph_ingestion::IngestionSource> {
    match source_config {
        unigraph_ingestion::IngestionSourceConfig::Git {
            repo_path,
            main_branch,
        } => {
            let repo_path = expand_tilde(repo_path);
            let ns =
                unigraph_storage_core::ExternalIDNamespace(format!("{}/git", repo_path.display()));
            Ok(unigraph_ingestion::IngestionSource::Git {
                repo_path,
                main_branch: main_branch.clone(),
                external_id_namespace: ns,
            })
        }
        unigraph_ingestion::IngestionSourceConfig::AnotherTimeline { source_timeline_id } => {
            let timeline_id = TimelineID(source_timeline_id.clone());
            let timeline_config = db
                .timelines
                .get_config(&timeline_id, task)
                .await?
                .with_context(|| {
                    format!(
                        "Source timeline '{}' not found in database",
                        source_timeline_id
                    )
                })?;
            let ns = timeline_config.external_id_namespace.with_context(|| {
                format!(
                    "Source timeline '{}' has no external_id_namespace",
                    source_timeline_id
                )
            })?;
            Ok(unigraph_ingestion::IngestionSource::AnotherTimeline {
                source_timeline_id: timeline_id,
                external_id_namespace: ns,
            })
        }
    }
}

#[derive(Parser)]
struct Frames {
    /// Timeline ID to inspect (omit to list all timelines)
    #[arg(long)]
    timeline_id: Option<String>,
}

impl Frames {
    async fn run(&self, sqlite_path: &Path, task: &ll::Task) -> anyhow::Result<()> {
        use unigraph_storage_core::format_frames_table;

        let path = resolve_sqlite_path(sqlite_path);
        let sqlite = Arc::new(unigraph_storage_sqlite::SqliteStorage::new(path)?);
        let db = unigraph_db::UnigraphDb::new(sqlite.clone(), sqlite);

        match &self.timeline_id {
            Some(id) => {
                let timeline_id = TimelineID(id.clone());
                let frames = db.frames.list(&timeline_id, task).await?;

                if frames.is_empty() {
                    eprintln!("No frames found for timeline '{}'", id);
                    return Ok(());
                }

                println!("{}", format_frames_table(&frames));
                println!("\nTotal: {} frames", frames.len());
            }
            None => {
                let timelines = db.timelines.list(task).await?;
                if timelines.is_empty() {
                    eprintln!("No timelines found in database.");
                    return Ok(());
                }
                println!("Timelines:");
                for tl in &timelines {
                    let frames = db.frames.list(tl, task).await?;
                    println!("  {} ({} frames)", tl.0, frames.len());
                }
            }
        }

        Ok(())
    }
}

#[derive(Parser)]
struct Compact {
    /// Timeline ID to compact
    #[arg(long)]
    timeline_id: String,

    /// Start of the time range (RFC 3339, e.g. 2025-01-01T00:00:00Z). Defaults to beginning of time.
    #[arg(long)]
    start: Option<String>,

    /// End of the time range (RFC 3339, e.g. 2025-12-31T23:59:59Z). Defaults to now.
    #[arg(long)]
    end: Option<String>,
}

impl Compact {
    async fn run(&self, sqlite_path: &Path, task: &ll::Task) -> anyhow::Result<()> {
        let path = resolve_sqlite_path(sqlite_path);
        let sqlite = Arc::new(unigraph_storage_sqlite::SqliteStorage::new(path)?);
        let db = unigraph_db::UnigraphDb::new(sqlite.clone(), sqlite);

        let start = parse_timestamp(self.start.as_deref())?;
        let end = parse_timestamp(self.end.as_deref())?;

        let timeline_id = TimelineID(self.timeline_id.clone());
        let converted = db.graph.compact(&timeline_id, start, end, task).await?;

        match converted {
            0 => println!("Nothing to compact."),
            n => println!("Compacted {n} frame(s) from Full to Delta."),
        }

        Ok(())
    }
}

#[derive(Parser)]
struct Graph {
    #[command(subcommand)]
    command: GraphCommands,
}

#[derive(Subcommand)]
enum GraphCommands {
    /// Fetch a graph and dump it as MapGraph JSON
    Get(GraphGet),
    /// Build a budget graph from a stored graph
    Budget(GraphBudget),
}

#[derive(Parser)]
struct GraphGet {
    /// Graph key (cargo~356) or timeline ID (cargo) for latest
    key: String,

    /// Print JSON to stdout instead of writing to a temp file
    #[arg(long)]
    stdout: bool,
}

impl GraphGet {
    async fn run(&self, sqlite_path: &Path, task: &ll::Task) -> anyhow::Result<()> {
        let path = resolve_sqlite_path(sqlite_path);
        let sqlite = Arc::new(unigraph_storage_sqlite::SqliteStorage::new(path)?);
        let db = unigraph_db::UnigraphDb::new(sqlite.clone(), sqlite);

        let parsed: GraphKeyOrTimelineID = self.key.parse()?;

        let (key, serializable) = match parsed {
            GraphKeyOrTimelineID::GraphKey(key) => {
                eprintln!("Fetching graph {}...", key);
                let graph = db.graph.fetch(&key, task).await?;
                (key, graph)
            }
            GraphKeyOrTimelineID::TimelineID(tid) => {
                eprintln!("Fetching latest graph from timeline {}...", tid);
                let (key, graph) = db.graph.fetch_latest(&tid, task).await?;
                eprintln!("Resolved to {}", key);
                (key, graph)
            }
        };

        let node_count = serializable.node_names_ordered.combined_nodes_len();
        let array_graph = serializable.into_array_graph();

        eprintln!("{} nodes", node_count);
        let map_graph = array_graph.to_map_graph()?;
        let json = serde_json::to_string_pretty(&map_graph)?;

        write_json_output(&json, self.stdout, &format!("graph_{}", key))
    }
}

#[derive(Parser)]
struct GraphBudget {
    /// Graph key (cargo~356) or timeline ID (cargo) for latest
    key: String,

    /// Budget config as JSON string
    #[arg(long)]
    budget_config_json: String,

    /// Print JSON to stdout instead of writing to a temp file
    #[arg(long)]
    stdout: bool,
}

impl GraphBudget {
    async fn run(&self, sqlite_path: &Path, task: &ll::Task) -> anyhow::Result<()> {
        let path = resolve_sqlite_path(sqlite_path);
        let sqlite = Arc::new(unigraph_storage_sqlite::SqliteStorage::new(path)?);
        let db = unigraph_db::UnigraphDb::new(sqlite.clone(), sqlite);

        let parsed: GraphKeyOrTimelineID = self.key.parse()?;
        let budget_config: BudgetConfig = serde_json::from_str(&self.budget_config_json)
            .context("failed to parse --budget-config-json")?;

        let (_key, serializable) = match parsed {
            GraphKeyOrTimelineID::GraphKey(key) => {
                eprintln!("Fetching graph {}...", key);
                let graph = db.graph.fetch(&key, task).await?;
                (key, graph)
            }
            GraphKeyOrTimelineID::TimelineID(tid) => {
                eprintln!("Fetching latest graph from timeline {}...", tid);
                let (key, graph) = db.graph.fetch_latest(&tid, task).await?;
                eprintln!("Resolved to {}", key);
                (key, graph)
            }
        };

        let node_count = serializable.node_names_ordered.combined_nodes_len();
        let array_graph = serializable.into_array_graph();

        eprintln!("Building budget graph ({} nodes in source)...", node_count);
        let (_source, budget) = build_budget_graph(array_graph, &budget_config)?;

        let map_graph = budget.to_map_graph()?;
        let json = serde_json::to_string_pretty(&map_graph)?;

        write_json_output(&json, self.stdout, "budget")
    }
}

/// Write JSON output to a temp file (printing the path to stdout) or to stdout directly.
fn write_json_output(json: &str, to_stdout: bool, label: &str) -> anyhow::Result<()> {
    if to_stdout {
        println!("{json}");
    } else {
        let dir = std::env::temp_dir().join("unigraph");
        std::fs::create_dir_all(&dir)?;
        let filename = format!("{label}_{}.json", std::process::id());
        let path = dir.join(filename);
        std::fs::write(&path, json).context("failed to write temp file")?;
        println!("{}", path.display());
    }
    Ok(())
}

fn expand_tilde(path: &Path) -> PathBuf {
    if let Ok(rest) = path.strip_prefix("~") {
        dirs::home_dir()
            .expect("could not determine home directory")
            .join(rest)
    } else {
        path.to_path_buf()
    }
}

fn parse_timestamp(s: Option<&str>) -> anyhow::Result<Option<unigraph_timestamp::Timestamp>> {
    match s {
        Some(s) => Ok(Some(
            unigraph_timestamp::Timestamp::from_rfc3339(s)
                .map_err(|e| anyhow::anyhow!("Invalid timestamp: {e}"))?,
        )),
        None => Ok(None),
    }
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let sqlite_path = &args.sqlite_path;

    // Show interactive task tree only on TTY terminals
    if std::io::stderr().is_tty() {
        ll::reporters::term_status::show();
    }

    let task = ll::Task::create_new("unigraph");

    let result = match args.command {
        Commands::Serve(serve) => {
            serve.run(sqlite_path).await;
            Ok(())
        }
        Commands::Ingest(ingest) => ingest.run(sqlite_path, &task).await,
        Commands::Frames(frames) => frames.run(sqlite_path, &task).await,
        Commands::Compact(compact) => compact.run(sqlite_path, &task).await,
        Commands::Graph(g) => match g.command {
            GraphCommands::Get(get) => get.run(sqlite_path, &task).await,
            GraphCommands::Budget(budget) => budget.run(sqlite_path, &task).await,
        },
    };

    if let Err(e) = result {
        eprintln!("Error: {e:#}");
        std::process::exit(1);
    }
}
