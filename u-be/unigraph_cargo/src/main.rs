// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

#[derive(Parser)]
#[command(
    name = "unigraph-cargo",
    about = "Analyze Rust dependency trees and produce MapGraph JSON for unigraph visualization"
)]
struct Args {
    /// Path to the Cargo.toml manifest file
    #[arg(short, long, default_value = "Cargo.toml")]
    manifest_path: PathBuf,

    /// Collect build timing metrics (runs `cargo build --timings=json`)
    #[arg(long)]
    timings: bool,

    /// Collect compiled .rlib sizes from target/debug/deps
    #[arg(long)]
    sizes: bool,

    /// Output file path (default: stdout)
    #[arg(short, long)]
    output: Option<PathBuf>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let manifest_path = args
        .manifest_path
        .canonicalize()
        .unwrap_or_else(|_| args.manifest_path.clone());

    // 1. Collect dependency graph from cargo metadata.
    eprintln!("Collecting cargo metadata...");
    let cargo_graph = unigraph_cargo::collect_metadata(&manifest_path)?;
    eprintln!("Found {} crates", cargo_graph.crates.len());

    // 2. Optionally collect build timings.
    let timings = if args.timings {
        eprintln!("Running cargo build --timings=json...");
        match unigraph_cargo::collect_timings(&manifest_path) {
            Ok(t) => {
                eprintln!("Collected timings for {} units", t.len());
                Some(t)
            }
            Err(e) => {
                eprintln!("Warning: failed to collect timings: {e}");
                None
            }
        }
    } else {
        None
    };

    // 3. Optionally collect rlib sizes.
    let sizes = if args.sizes {
        eprintln!("Scanning rlib sizes...");
        match unigraph_cargo::collect_rlib_sizes(&cargo_graph.target_directory) {
            Ok(s) => {
                eprintln!("Found sizes for {} rlibs", s.len());
                Some(s)
            }
            Err(e) => {
                eprintln!("Warning: failed to collect rlib sizes: {e}");
                None
            }
        }
    } else {
        None
    };

    // 4. Build the MapGraph.
    let map_graph = unigraph_cargo::build_map_graph(&cargo_graph, timings.as_ref(), sizes.as_ref());

    // 5. Serialize to JSON.
    let json = serde_json::to_string_pretty(&map_graph)?;

    // 6. Write output.
    if let Some(output_path) = &args.output {
        std::fs::write(output_path.as_path(), &json)?;
        eprintln!("Wrote graph to {}", output_path.display());
    } else {
        println!("{json}");
    }

    Ok(())
}
