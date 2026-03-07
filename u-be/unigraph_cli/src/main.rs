// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use clap::Subcommand;
use unigraph_ingestion::GraphBuilder as _;
use unigraph_storage_core::TimelineID;
use unigraph_web_service::ServeMode;

fn default_sqlite_path() -> PathBuf {
    dirs::home_dir()
        .expect("could not determine home directory")
        .join(".unigraph")
        .join("sqlite")
}

fn resolve_sqlite_path(path: &PathBuf) -> &PathBuf {
    if *path == default_sqlite_path() {
        eprintln!("Using database: {}", path.display());
    }
    path
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Serve(Serve),
    Ingest(Ingest),
    Frames(Frames),
    Compact(Compact),
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

    /// Path to the SQLite database file (for timeline browsing)
    #[arg(long)]
    sqlite_path: Option<PathBuf>,

    /// Serve pre-built static files instead of proxying to Vite dev server
    #[arg(long)]
    release: bool,
}

impl Serve {
    async fn run(&self) {
        let mode = if self.release {
            ServeMode::Release
        } else {
            ServeMode::Dev
        };
        unigraph_web_service::start(&self.file_path, &self.right, &self.sqlite_path, mode)
            .await
            .unwrap();
    }
}

#[derive(Parser)]
struct Ingest {
    /// Path to the git repository to ingest
    #[arg(long)]
    git_repo_path: PathBuf,

    /// Path to the SQLite database file (created if it doesn't exist)
    #[arg(long, default_value_os_t = default_sqlite_path())]
    sqlite_path: PathBuf,

    /// Which graph builder to use
    #[arg(long, value_enum, default_value = "cargo")]
    graph_builder: GraphBuilderKind,

    /// Path to Cargo.toml relative to the repo root (only for cargo builder)
    #[arg(long, default_value = "Cargo.toml")]
    manifest_path: PathBuf,
}

#[derive(Clone, clap::ValueEnum)]
enum GraphBuilderKind {
    Cargo,
}

impl Ingest {
    async fn run(&self) -> anyhow::Result<()> {
        let path = resolve_sqlite_path(&self.sqlite_path);
        let sqlite = Arc::new(unigraph_storage_sqlite::SqliteStorage::new(path)?);
        let db = unigraph_db::UnigraphDb::new(sqlite.clone(), sqlite);

        let builder = match self.graph_builder {
            GraphBuilderKind::Cargo => {
                unigraph_ingestion::CargoGraphBuilder::new(self.manifest_path.clone())
            }
        };

        let config = unigraph_ingestion::IngestionPipelineConfig {
            source: unigraph_ingestion::IngestionSource::Git {
                repo_path: self.git_repo_path.clone(),
                main_branch: "main".to_string(),
            },
            external_id_namespace: unigraph_storage_core::ExternalIDNamespace(format!(
                "{}/git",
                self.git_repo_path.display()
            )),
            builders: vec![unigraph_ingestion::TimelineBuilderConfig {
                timeline_id: unigraph_storage_core::TimelineID(builder.name().to_string()),
                builder: &builder,
            }],
        };

        unigraph_ingestion::run_ingestion(&config, &db).await
    }
}

#[derive(Parser)]
struct Frames {
    /// Path to the SQLite database file
    #[arg(long, default_value_os_t = default_sqlite_path())]
    sqlite_path: PathBuf,

    /// Timeline ID to inspect (omit to list all timelines)
    #[arg(long)]
    timeline_id: Option<String>,
}

impl Frames {
    async fn run(&self) -> anyhow::Result<()> {
        use unigraph_storage_core::format_frames_table;

        let path = resolve_sqlite_path(&self.sqlite_path);
        let sqlite = Arc::new(unigraph_storage_sqlite::SqliteStorage::new(path)?);
        let db = unigraph_db::UnigraphDb::new(sqlite.clone(), sqlite);

        match &self.timeline_id {
            Some(id) => {
                let timeline_id = TimelineID(id.clone());
                let frames = db.list_frames(&timeline_id).await?;

                if frames.is_empty() {
                    eprintln!("No frames found for timeline '{}'", id);
                    return Ok(());
                }

                println!("{}", format_frames_table(&frames));
                println!("\nTotal: {} frames", frames.len());
            }
            None => {
                let timelines = db.list_timelines().await?;
                if timelines.is_empty() {
                    eprintln!("No timelines found in database.");
                    return Ok(());
                }
                println!("Timelines:");
                for tl in &timelines {
                    let frames = db.list_frames(tl).await?;
                    println!("  {} ({} frames)", tl.0, frames.len());
                }
            }
        }

        Ok(())
    }
}

#[derive(Parser)]
struct Compact {
    /// Path to the SQLite database file
    #[arg(long, default_value_os_t = default_sqlite_path())]
    sqlite_path: PathBuf,

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
    async fn run(&self) -> anyhow::Result<()> {
        let path = resolve_sqlite_path(&self.sqlite_path);
        let sqlite = Arc::new(unigraph_storage_sqlite::SqliteStorage::new(path)?);
        let db = unigraph_db::UnigraphDb::new(sqlite.clone(), sqlite);

        let start = parse_timestamp(self.start.as_deref())?;
        let end = parse_timestamp(self.end.as_deref())?;

        let timeline_id = TimelineID(self.timeline_id.clone());
        let converted = db.compact_timeline(&timeline_id, start, end).await?;

        match converted {
            0 => println!("Nothing to compact."),
            n => println!("Compacted {n} frame(s) from Full to Delta."),
        }

        Ok(())
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
    // Parse command line arguments
    let args = Args::parse();

    match args.command {
        Commands::Serve(serve) => serve.run().await,
        Commands::Ingest(ingest) => {
            if let Err(e) = ingest.run().await {
                eprintln!("Error: {e:#}");
                std::process::exit(1);
            }
        }
        Commands::Frames(frames) => {
            if let Err(e) = frames.run().await {
                eprintln!("Error: {e:#}");
                std::process::exit(1);
            }
        }
        Commands::Compact(compact) => {
            if let Err(e) = compact.run().await {
                eprintln!("Error: {e:#}");
                std::process::exit(1);
            }
        }
    }
}
