// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::path::Path;
use std::path::PathBuf;

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
}

impl CargoGraphBuilder {
    pub fn new(manifest_relative_path: PathBuf) -> Self {
        Self {
            manifest_relative_path,
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
        let map_graph = unigraph_cargo::build_map_graph(&cargo_graph, None, None);
        Ok(map_graph)
    }
}
