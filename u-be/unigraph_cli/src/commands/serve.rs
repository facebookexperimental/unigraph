// Copyright (c) Meta Platforms, Inc. and affiliates.

use clap::Parser;
use unigraph_cli::UnigraphCLIContext;
use unigraph_cli::UnigraphCLISubcommand;
use unigraph_web_service::ServeMode;

#[derive(Parser)]
pub struct Serve {
    /// Path to the graph file to visualize
    #[arg(short, long)]
    file_path: Option<std::path::PathBuf>,

    /// Path to the comparison ("before") graph file for delta view
    #[arg(short = 'l', long)]
    left: Option<std::path::PathBuf>,

    /// Serve pre-built static files instead of proxying to Vite dev server
    #[arg(long)]
    release: bool,
}

impl UnigraphCLISubcommand for Serve {
    async fn run(&self, ctx: &UnigraphCLIContext, task: &ll::Task) -> anyhow::Result<()> {
        let mode = if self.release {
            ServeMode::Release
        } else {
            ServeMode::Dev
        };
        unigraph_web_service::start(&self.file_path, &self.left, &ctx.sqlite_path, mode, task).await
    }
}
