// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::path::PathBuf;

use clap::Parser;
use clap::Subcommand;
use unigraph_web_service::ServeMode;

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
    async fn run(&self) {
        let mode = if self.release {
            ServeMode::Release
        } else {
            ServeMode::Dev
        };
        unigraph_web_service::start(&self.file_path, &self.right, mode)
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
