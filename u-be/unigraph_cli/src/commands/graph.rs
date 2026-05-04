// Copyright (c) Meta Platforms, Inc. and affiliates.

use clap::Parser;
use clap::Subcommand;
use unigraph_cli::UnigraphCLIContext;
use unigraph_cli::UnigraphCLISubcommand;
use unigraph_cli::graph::GraphExplore;
use unigraph_cli::graph::GraphGet;
use unigraph_cli::graph::GraphGetError;
use unigraph_cli::graph::GraphPut;
use unigraph_cli::graph::GraphUpload;

#[derive(Parser)]
pub struct Graph {
    #[command(subcommand)]
    pub command: GraphCommands,
}

#[derive(Subcommand)]
pub enum GraphCommands {
    Get(GraphGet),
    Put(GraphPut),
    GetError(GraphGetError),
    Explore(GraphExplore),
    Upload(GraphUpload),
}

impl UnigraphCLISubcommand for Graph {
    async fn run(&self, ctx: &UnigraphCLIContext, task: &ll::Task) -> anyhow::Result<()> {
        match &self.command {
            GraphCommands::Get(cmd) => cmd.run(ctx, task).await,
            GraphCommands::Put(cmd) => cmd.run(ctx, task).await,
            GraphCommands::GetError(cmd) => cmd.run(ctx, task).await,
            GraphCommands::Explore(cmd) => cmd.run(ctx, task).await,
            GraphCommands::Upload(cmd) => cmd.run(ctx, task).await,
        }
    }
}
