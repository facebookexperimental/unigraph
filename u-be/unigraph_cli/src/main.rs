// Copyright (c) Meta Platforms, Inc. and affiliates.

use clap::Parser;
use clap::Subcommand;
use clap::command;
use unigraph_core::GraphiteGraph;
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
    file_path: Option<String>,
}

#[derive(Parser)]
struct Serve {
    /// Path to the graph file to visualize
    #[arg(short, long)]
    file_path: Option<String>,
}

impl Serve {
    async fn run(&self) {
        unigraph_web_service::start(&self.file_path).await.unwrap();
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
            let file_string_content =
                std::fs::read_to_string(file_path).expect("Failed to read file");
            GraphiteGraph::from_json(&file_string_content)
                .expect("Failed to parse JSON")
                .into_map_graph()
                .expect("Failed to convert to MapGraph")
                .to_array_graph()
                .expect("Failed to convert to ArrayGraph")
        } else {
            make_test_graph().unwrap().to_array_graph().unwrap()
        };
        unigraph_wgpu::start(array_graph).await.unwrap();
    }
}
