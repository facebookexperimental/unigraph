// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use unigraph_core::ArrayGraphSerializable;
use unigraph_core::BudgetConfig;
use unigraph_core::MapGraph;
use unigraph_core::build_budget_graph;

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

/// Transforms a source graph into a budget graph.
///
/// Used with `AnotherTimeline` sources: reads a graph from an existing
/// timeline and computes aggregated budget metrics via `build_budget_graph`.
pub struct BudgetGraphBuilder {
    /// Name for this builder (used in timeline IDs and logging).
    pub name: String,
    /// Budget query config. Defines which metrics to aggregate transitively,
    /// whether to count nodes, etc.
    pub budget_config: Option<BudgetConfig>,
}

impl BudgetGraphBuilder {
    pub fn build(&self, source: ArrayGraphSerializable) -> Result<ArrayGraphSerializable> {
        let config = self
            .budget_config
            .as_ref()
            .context("BudgetGraphBuilder requires a budget_config")?;
        let array_graph = source.into_array_graph();
        let (_source, budget) = build_budget_graph(array_graph, config)?;
        Ok(budget.into_serializable())
    }
}

/// Unified builder: either builds from a repo path or transforms an existing graph.
pub enum Builder<'a> {
    /// Builds a graph from a repo checkout (used with Git sources).
    FromRepo(&'a dyn GraphBuilder),
    /// Transforms an existing graph into a budget graph (used with AnotherTimeline sources).
    BudgetGraph(&'a BudgetGraphBuilder),
}

impl Builder<'_> {
    pub fn name(&self) -> &str {
        match self {
            Builder::FromRepo(b) => b.name(),
            Builder::BudgetGraph(b) => &b.name,
        }
    }
}
