// Copyright (c) Meta Platforms, Inc. and affiliates.

mod commands;

use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use clap::Subcommand;
use commands::compact::Compact;
use commands::graph::Graph;
use commands::graph::GraphCommands;
use commands::impact_analysis::ImpactAnalysisCmd;
use commands::ingest::Ingest;
use commands::serve::Serve;
use commands::timelines::Timelines;
use crossterm::tty::IsTty;
use unigraph_cli::UnigraphCLISubcommand;

#[tokio::main]
async fn main() {
    let args = Args::parse();

    if std::io::stderr().is_tty() && args.task_tree {
        ll_stdio::term_status::show();
    } else {
        tracing_subscriber::fmt()
            .compact()
            .with_target(false)
            .init();
    }

    if let Err(e) = run(args).await {
        eprintln!("Error: {e:#}");
        std::process::exit(1);
    }
}

async fn run(args: Args) -> anyhow::Result<()> {
    let task = ll::Task::create_new("unigraph");

    let sqlite_path = args.sqlite_path;
    let sqlite = Arc::new(unigraph_storage_sqlite::SqliteStorage::new(&sqlite_path)?);
    let db = unigraph_db::UnigraphDb::new(sqlite.clone(), sqlite);
    let ctx = unigraph_cli::UnigraphCLIContext::new(db, sqlite_path);

    task.data("database", format!("{}", ctx.sqlite_path.display()));

    let result = match args.command {
        Commands::Serve(cmd) => cmd.run(&ctx, &task).await,
        Commands::Ingest(cmd) => cmd.run(&ctx, &task).await,
        Commands::Timelines(cmd) => cmd.run(&ctx, &task).await,
        Commands::Compact(cmd) => cmd.run(&ctx, &task).await,
        Commands::Graph(g) => match g.command {
            GraphCommands::Get(cmd) => cmd.run(&ctx, &task).await,
            GraphCommands::Put(cmd) => cmd.run(&ctx, &task).await,
            GraphCommands::GetError(cmd) => cmd.run(&ctx, &task).await,
            GraphCommands::Explore(cmd) => cmd.run(&ctx, &task).await,
        },
        Commands::ImpactAnalysis(cmd) => cmd.run(&ctx, &task).await,
    };

    ctx.flush_deferred();
    result
}

#[derive(Parser)]
#[command(long_about = None)]
struct Args {
    // show ll task tree in the terminal
    #[arg(long)]
    task_tree: bool,

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
    /// Timeline operations: list, inspect frames, collect stats
    Timelines(Timelines),
    Compact(Compact),
    Graph(Graph),
    /// Run impact analysis on a graph JSON file
    ImpactAnalysis(ImpactAnalysisCmd),
}

// ---------------------------------------------------------------------------
// Shared utilities
// ---------------------------------------------------------------------------

pub(crate) fn default_sqlite_path() -> PathBuf {
    dirs::home_dir()
        .expect("could not determine home directory")
        .join(".unigraph")
        .join("sqlite")
}

pub(crate) fn expand_tilde(path: &Path) -> PathBuf {
    if let Ok(rest) = path.strip_prefix("~") {
        dirs::home_dir()
            .expect("could not determine home directory")
            .join(rest)
    } else {
        path.to_path_buf()
    }
}

pub(crate) fn parse_timestamp(
    s: Option<&str>,
) -> anyhow::Result<Option<unigraph_timestamp::Timestamp>> {
    match s {
        Some(s) => Ok(Some(
            unigraph_timestamp::Timestamp::from_rfc3339(s)
                .map_err(|e| anyhow::anyhow!("Invalid timestamp: {e}"))?,
        )),
        None => Ok(None),
    }
}
