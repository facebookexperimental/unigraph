// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use anyhow::Context;
use anyhow::Result;
use unigraph_core::MapGraph;

/// Trait for building a graph from a repository checked out at a specific commit.
///
/// Implementations receive the repository root path (working tree already
/// checked out at the target commit) and return a `MapGraph`.
pub trait GraphBuilder {
    /// Human-readable name for this builder (used in timeline IDs and logging).
    fn name(&self) -> &str;

    /// Build a graph from the repository at the given path.
    ///
    /// The repository is already checked out at the desired commit.
    /// Returns `Err` if the graph cannot be built (e.g., no Cargo.toml found,
    /// cargo metadata fails). Errors are stored as error frames, not fatal.
    fn build(&self, repo_path: &Path) -> Result<MapGraph>;
}

/// Builds dependency graphs from Cargo workspaces.
pub struct CargoGraphBuilder {
    /// Path to Cargo.toml relative to the repository root.
    pub manifest_relative_path: PathBuf,
    /// Whether to run `cargo build --timings` and collect build timing metrics.
    pub collect_timings: bool,
    /// Whether to collect compiled `.rlib` artifact sizes.
    pub collect_sizes: bool,
}

impl CargoGraphBuilder {
    pub fn new(
        manifest_relative_path: PathBuf,
        collect_timings: bool,
        collect_sizes: bool,
    ) -> Self {
        Self {
            manifest_relative_path,
            collect_timings,
            collect_sizes,
        }
    }
}

impl GraphBuilder for CargoGraphBuilder {
    fn name(&self) -> &str {
        "cargo"
    }

    fn build(&self, repo_path: &Path) -> Result<MapGraph> {
        let manifest_path = repo_path.join(&self.manifest_relative_path);
        let cargo_graph = unigraph_cargo::collect_metadata(&manifest_path)?;

        let timings = if self.collect_timings {
            match unigraph_cargo::collect_timings(&manifest_path) {
                Ok(t) => Some(t),
                Err(e) => {
                    eprintln!("  Warning: failed to collect timings: {e:#}");
                    None
                }
            }
        } else {
            None
        };

        let rlib_sizes = if self.collect_sizes {
            // collect_timings runs `cargo +nightly build`, which produces rlib artifacts.
            // If timings weren't collected, we need a plain `cargo build` first.
            if (!self.collect_timings || timings.is_none())
                && let Err(e) = run_cargo_build(&manifest_path)
            {
                eprintln!("  Warning: cargo build failed: {e:#}");
            }
            match unigraph_cargo::collect_rlib_sizes(&cargo_graph.target_directory) {
                Ok(s) => Some(s),
                Err(e) => {
                    eprintln!("  Warning: failed to collect rlib sizes: {e:#}");
                    None
                }
            }
        } else {
            None
        };

        let map_graph =
            unigraph_cargo::build_map_graph(&cargo_graph, timings.as_ref(), rlib_sizes.as_ref());
        Ok(map_graph)
    }
}

fn run_cargo_build(manifest_path: &Path) -> Result<()> {
    let output = Command::new("cargo")
        .arg("build")
        .arg("--manifest-path")
        .arg(manifest_path)
        .output()
        .context("Failed to run cargo build")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("cargo build failed:\n{stderr}");
    }
    Ok(())
}

/// Unified builder: builds from a repo path.
pub enum Builder<'a> {
    /// Builds a graph from a repo checkout (used with Git sources).
    FromRepo(&'a dyn GraphBuilder),
}

impl Builder<'_> {
    pub fn name(&self) -> &str {
        match self {
            Builder::FromRepo(b) => b.name(),
        }
    }
}
