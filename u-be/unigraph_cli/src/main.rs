// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::path::Path;
use std::path::PathBuf;

use clap::Parser;
use clap::Subcommand;
use clap::command;
use unigraph_core::ArrayGraph;
use unigraph_core::MapGraph;
use unigraph_core::make_test_graph;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Start(Start),
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
        Commands::Start(start) => {
            start.run().await;
        }
        Commands::Serve(serve) => serve.run().await,
    }
}

impl Start {
    async fn run(&self) {
        let array_graph = if let Some(file_path) = &self.file_path {
            read_graph(file_path)
        } else {
            make_test_graph().unwrap().to_array_graph().unwrap()
        };
        unigraph_wgpu::start(array_graph).await.unwrap();
    }
}

fn read_graph(p: &Path) -> ArrayGraph {
    let file_string_content = std::fs::read_to_string(p).expect("Failed to read file");
    MapGraph::from_json(&file_string_content)
        .expect("Failed to parse JSON")
        .to_array_graph()
        .expect("Failed to convert to ArrayGraph")
}
