// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::path::PathBuf;

use clap::Parser;
use clap::Subcommand;
use clap::command;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Serve(Serve),
}

#[derive(Parser)]
struct Start {
    /// Path to the graph file to visualize
    #[arg(short, long)]
    file_path: Option<PathBuf>,
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
}

impl Serve {
    async fn run(&self) {
        unigraph_web_service::start(&self.file_path, &self.right)
            .await
            .unwrap();
    }
}

#[tokio::main]
async fn main() {
    // Parse command line arguments
    let args = Args::parse();

    match args.command {
        Commands::Serve(serve) => serve.run().await,
    }
}
