// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use unigraph_core::ArrayGraph;
use unigraph_core::GraphHandle;
use unigraph_core::config_query::GraphQueryConfig;
use unigraph_core::config_query::TraversalOverride;
use unigraph_storage_core::GraphKey;

use crate::UnigraphCLIContext;

/// Shared arguments for resolving a graph handle into a traversed subgraph.
///
/// Flattened into commands that operate on a fetched subgraph (`get`, `cut`),
/// so they all accept the same handle/roots/traversal options and share the
/// fetch logic.
#[derive(Parser, Debug)]
pub struct SubgraphArgs {
    /// Graph handle: `gqc_{hash}`, `timeline_id~graph_id`, or `timeline_id`
    pub handle: String,

    /// Root node names for subgraph extraction (repeatable)
    #[arg(long)]
    pub roots: Option<Vec<String>>,

    /// File containing root node names as JSON.
    /// Accepts either a JSON array `["A", "B"]` or a JSON object
    /// `{"A": ..., "B": ...}` (only keys are used). Merged with `--roots`.
    #[arg(long)]
    pub roots_json: Option<PathBuf>,

    /// Traversal config key (`tvc_{hash}`) to override graph traversal
    #[arg(long)]
    pub traversal: Option<String>,
}

impl SubgraphArgs {
    /// Resolve the handle (applying roots/traversal overrides) into the
    /// traversed [`ArrayGraph`].
    pub async fn fetch(
        &self,
        ctx: &UnigraphCLIContext,
        task: &ll::Task,
    ) -> anyhow::Result<(GraphKey, ArrayGraph)> {
        let gqc = self.build_query_config()?;
        ctx.db.resolve_graph_query_config(&gqc, false, task).await
    }

    fn build_query_config(&self) -> anyhow::Result<GraphQueryConfig> {
        let handle: GraphHandle = self
            .handle
            .parse()
            .context("Failed to parse graph handle")?;

        let roots = self.collect_roots()?;

        let traversal = self
            .traversal
            .as_ref()
            .map(|t| t.parse())
            .transpose()
            .context("Failed to parse traversal config key")?
            .map(TraversalOverride::Key);

        Ok(GraphQueryConfig {
            handle,
            roots,
            traversal,
        })
    }

    fn collect_roots(&self) -> anyhow::Result<Option<BTreeSet<String>>> {
        let roots_from_file = match &self.roots_json {
            Some(path) => parse_names_json(path).context("Failed to read --roots-json file")?,
            None => vec![],
        };

        let all_roots: BTreeSet<String> = roots_from_file
            .into_iter()
            .chain(
                self.roots
                    .as_ref()
                    .into_iter()
                    .flat_map(|r| r.iter().cloned()),
            )
            .collect();

        Ok(if all_roots.is_empty() {
            None
        } else {
            Some(all_roots)
        })
    }
}

/// Parse a JSON file of node names: either an array `["A", "B"]` or an object
/// `{"A": ..., "B": ...}` (only the keys are used).
pub fn parse_names_json(path: &Path) -> anyhow::Result<Vec<String>> {
    let json = std::fs::read_to_string(path).context("Failed to read JSON file")?;

    serde_json::from_str::<Vec<String>>(&json).or_else(|_| {
        serde_json::from_str::<BTreeMap<String, serde_json::Value>>(&json)
            .map(|map| map.into_keys().collect())
            .context("expected a JSON array or a JSON object")
    })
}
