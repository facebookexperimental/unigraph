// Copyright (c) Meta Platforms, Inc. and affiliates.

use clap::Parser;
use clap::Subcommand;
use unigraph_cli::UnigraphCLIContext;
use unigraph_cli::UnigraphCLISubcommand;
use unigraph_cli::timelines::TimelinesFrames;
use unigraph_cli::timelines::TimelinesGet;
use unigraph_cli::timelines::TimelinesList;
use unigraph_cli::timelines::TimelinesPut;
use unigraph_cli::timelines::TimelinesStats;

#[derive(Parser)]
pub struct Timelines {
    #[command(subcommand)]
    command: TimelinesCommands,
}

#[derive(Subcommand)]
enum TimelinesCommands {
    List(TimelinesList),
    Get(TimelinesGet),
    Put(TimelinesPut),
    Frames(TimelinesFrames),
    Stats(TimelinesStats),
}

impl UnigraphCLISubcommand for Timelines {
    async fn run(&self, ctx: &UnigraphCLIContext, task: &ll::Task) -> anyhow::Result<()> {
        match &self.command {
            TimelinesCommands::List(cmd) => cmd.run(ctx, task).await,
            TimelinesCommands::Get(cmd) => cmd.run(ctx, task).await,
            TimelinesCommands::Put(cmd) => cmd.run(ctx, task).await,
            TimelinesCommands::Frames(cmd) => cmd.run(ctx, task).await,
            TimelinesCommands::Stats(cmd) => cmd.run(ctx, task).await,
        }
    }
}
