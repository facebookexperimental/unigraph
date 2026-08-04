// Copyright (c) Meta Platforms, Inc. and affiliates.

use clap::Parser;
use clap::Subcommand;
use unigraph_cli::UnigraphCLIContext;
use unigraph_cli::UnigraphCLISubcommand;
use unigraph_cli::history::HistoryCompact;
use unigraph_cli::history::HistoryDelete;
use unigraph_cli::history::HistoryIngest;
use unigraph_cli::history::HistoryShow;

#[derive(Parser)]
pub struct History {
    #[command(subcommand)]
    command: HistoryCommands,
}

#[derive(Subcommand)]
enum HistoryCommands {
    Ingest(HistoryIngest),
    Compact(HistoryCompact),
    Delete(HistoryDelete),
    Show(HistoryShow),
}

impl UnigraphCLISubcommand for History {
    async fn run(&self, ctx: &UnigraphCLIContext, task: &ll::Task) -> anyhow::Result<()> {
        match &self.command {
            HistoryCommands::Ingest(cmd) => cmd.run(ctx, task).await,
            HistoryCommands::Compact(cmd) => cmd.run(ctx, task).await,
            HistoryCommands::Delete(cmd) => cmd.run(ctx, task).await,
            HistoryCommands::Show(cmd) => cmd.run(ctx, task).await,
        }
    }
}
