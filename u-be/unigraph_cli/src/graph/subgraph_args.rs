// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::bail;
use clap::Parser;
use unigraph_core::ArrayGraph;
use unigraph_core::GraphHandle;
use unigraph_core::NameMatch;
use unigraph_core::NameMatchMode;
use unigraph_core::NodeSelection;
use unigraph_core::PropertyValueMatch;
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

/// Shared arguments for the `Matching` explore target.
///
/// Flattened into `explore` and `explore-delta` so both describe a node
/// selection the same way. Producing `None` — no flags given — leaves the
/// command on its normal entry-points / node / all-nodes target.
#[derive(Parser, Debug)]
pub struct NodeMatchArgs {
    /// Match nodes whose name matches this pattern. See `--match-mode`.
    #[arg(long)]
    pub match_name: Option<String>,

    /// How `--match-name` is read.
    #[arg(long, default_value = "substring", requires = "match_name")]
    pub match_mode: NameMatchModeArg,

    /// Match nodes carrying a property (repeatable, ANDed).
    ///
    /// `NAME=VALUE` requires that exact value; a bare `NAME` matches any node
    /// carrying the property at all.
    #[arg(long = "match-property", num_args = 1, value_name = "NAME[=VALUE]")]
    pub match_properties: Vec<String>,

    /// Match nodes with an incoming edge carrying this tag (repeatable, ANDed).
    #[arg(long = "match-incoming-tag", num_args = 1)]
    pub match_incoming_tags: Vec<String>,

    /// Match nodes with an outgoing edge carrying this tag (repeatable, ANDed).
    #[arg(long = "match-outgoing-tag", num_args = 1)]
    pub match_outgoing_tags: Vec<String>,
}

impl NodeMatchArgs {
    /// The selection these flags describe, or `None` when none were given.
    pub fn build(&self) -> anyhow::Result<Option<NodeSelection>> {
        let selection = NodeSelection {
            name: self.match_name.as_ref().map(|pattern| NameMatch {
                pattern: pattern.clone(),
                mode: self.match_mode.into(),
            }),
            properties: self
                .match_properties
                .iter()
                .map(|condition| {
                    let (name, value) = split_property(condition)?;
                    Ok((name, PropertyValueMatch { value }))
                })
                .collect::<anyhow::Result<_>>()?,
            incoming_tags: self.match_incoming_tags.iter().cloned().collect(),
            incoming_dynamic_type_keys: BTreeSet::new(),
            outgoing_tags: self.match_outgoing_tags.iter().cloned().collect(),
            outgoing_dynamic_type_keys: BTreeSet::new(),
        };

        Ok((!selection.is_empty()).then_some(selection))
    }
}

/// `NAME=VALUE` into its parts; a bare `NAME` into "any value".
///
/// Splits on the first `=` so a value may itself contain one.
fn split_property(condition: &str) -> anyhow::Result<(String, Option<String>)> {
    let (name, value) = match condition.split_once('=') {
        Some((name, value)) => (name, Some(value.to_string())),
        None => (condition, None),
    };
    if name.is_empty() {
        bail!("expected NAME or NAME=VALUE in --match-property, got {condition:?}");
    }
    Ok((name.to_string(), value))
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum NameMatchModeArg {
    /// Plain text, matched anywhere in the name, ignoring case.
    Substring,
    /// Rust regex syntax, unanchored and case-sensitive.
    Regex,
    /// Subsequence match, shortest name first. Capped, so the reported total
    /// is a lower bound.
    Fuzzy,
    /// The one node with exactly this name.
    Exact,
}

impl From<NameMatchModeArg> for NameMatchMode {
    fn from(val: NameMatchModeArg) -> Self {
        match val {
            NameMatchModeArg::Substring => NameMatchMode::Substring,
            NameMatchModeArg::Regex => NameMatchMode::Regex,
            NameMatchModeArg::Fuzzy => NameMatchMode::Fuzzy,
            NameMatchModeArg::Exact => NameMatchMode::Exact,
        }
    }
}
