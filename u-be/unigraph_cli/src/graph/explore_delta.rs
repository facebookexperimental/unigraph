// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeSet;

use anyhow::Context;
use clap::Parser;
use unigraph_app::ExploreDeltaInput;
use unigraph_app::ExploreGraphTarget;
use unigraph_app::RpcExec;
use unigraph_app::Unigraph;
use unigraph_core::GraphHandle;
use unigraph_core::MetricColumn;
use unigraph_core::config_query::GraphQueryConfig;
use unigraph_core::config_query::TraversalOverride;

use crate::UnigraphCLIContext;
use crate::graph::explore::SortOrderArg;
use crate::graph::explore::StructureArg;

/// Compare two graphs — the CLI equivalent of the UI's delta view.
///
/// Shows entry points, a node's children, or all nodes, with `∆` metric
/// columns you can sort by. `--changed-only` collapses stretches of the graph
/// that are identical on both sides and reports how many nodes it skipped.
///
/// Metric columns use the `--metric` syntax of `graph explore` plus a side
/// suffix: bare is the right (after) graph, `@left` is the before graph, and
/// `@delta` is the difference.
///
/// Examples:
///
/// ```sh
/// # Entry points, default columns (right value + ∆ for every visible metric)
/// unigraph graph explore-delta before_timeline after_timeline
///
/// # Drill into a node, hiding everything that didn't change
/// unigraph graph explore-delta before after --node app --changed-only
///
/// # Biggest movers across the whole graph
/// unigraph graph explore-delta before after --all-nodes \
///     --sort-by 'size~transitive@delta' --limit 30
///
/// # Show before, after, and the difference side by side
/// unigraph graph explore-delta before after --node app \
///     --metric 'size~transitive@left' \
///     --metric 'size~transitive' \
///     --metric 'size~transitive@delta'
///
/// # Output full JSON instead of the ASCII table
/// unigraph graph explore-delta before after --json
/// ```
#[derive(Parser, Debug)]
pub struct GraphExploreDelta {
    /// The "before" graph: timeline ID, graph key, or GQC key.
    left: String,

    /// The "after" graph. Deltas are `right - left`.
    right: String,

    /// Explore a specific node's children instead of entry points.
    #[arg(long)]
    node: Option<String>,

    /// Show all reachable nodes instead of entry points.
    #[arg(long, conflicts_with = "node")]
    all_nodes: bool,

    /// Edge structure to follow: forward, reverse, or dominator.
    #[arg(long, default_value = "forward")]
    structure: StructureArg,

    /// Collapse nodes that are identical in both graphs.
    #[arg(long)]
    changed_only: bool,

    /// Columns to compute (repeatable).
    ///
    /// Same format as `graph explore --metric`, optionally suffixed with
    /// `@left` or `@delta`. Defaults to the right value plus `∆` for every
    /// visible metric view.
    #[arg(long = "metric", num_args = 1)]
    metrics: Vec<String>,

    /// Column to sort by (same format as --metric).
    #[arg(long)]
    sort_by: Option<String>,

    /// Sort direction.
    #[arg(long, default_value = "desc")]
    sort_order: SortOrderArg,

    /// Sort `@delta` columns by their signed value instead of their magnitude.
    /// By default the biggest change wins regardless of direction.
    #[arg(long)]
    signed_delta_sort: bool,

    /// Skip first N results (pagination).
    #[arg(long)]
    offset: Option<usize>,

    /// Maximum number of arrows to return (default: 50).
    #[arg(long)]
    limit: Option<usize>,

    /// Override the root nodes of both subgraphs (repeatable).
    #[arg(long = "root", num_args = 1)]
    roots: Vec<String>,

    /// Override the root nodes of the left subgraph only (repeatable).
    #[arg(long = "left-root", num_args = 1)]
    left_roots: Vec<String>,

    /// Override the root nodes of the right subgraph only (repeatable).
    #[arg(long = "right-root", num_args = 1)]
    right_roots: Vec<String>,

    /// Traversal config key (`tvc_{hash}`) applied to both graphs.
    #[arg(long)]
    traversal: Option<String>,

    /// Traversal config key for the left graph only.
    #[arg(long)]
    left_traversal: Option<String>,

    /// Traversal config key for the right graph only.
    #[arg(long)]
    right_traversal: Option<String>,

    /// Output full JSON instead of ASCII table.
    #[arg(long)]
    json: bool,
}

impl GraphExploreDelta {
    pub async fn run(&self, ctx: &UnigraphCLIContext, task: &ll::Task) -> anyhow::Result<()> {
        let input = self.build_input()?;
        let unigraph = Unigraph::new(ctx.db.clone());
        let result = input.exec(&unigraph, task).await?;

        if self.json {
            let text = serde_json::to_string_pretty(&result)?;
            ctx.println_after_done(&text)?;
        } else {
            let ascii = result
                .ascii
                .as_deref()
                .unwrap_or("(no ASCII output — set include_ascii)");
            ctx.println_after_done(ascii)?;
        }

        Ok(())
    }
}

// -- Private helpers ----------------------------------------------------------

impl GraphExploreDelta {
    fn build_input(&self) -> anyhow::Result<ExploreDeltaInput> {
        Ok(ExploreDeltaInput {
            left: self.build_query_config(&self.left, &self.left_roots, &self.left_traversal)?,
            right: self.build_query_config(
                &self.right,
                &self.right_roots,
                &self.right_traversal,
            )?,
            target: self.build_target(),
            graph_structure: self.structure.into(),
            changed_nodes_only: self.changed_only,
            metrics: self.parse_metrics()?,
            sort_by: self.parse_sort_by()?,
            sort_order: Some(self.sort_order.into()),
            sort_delta_by_magnitude: Some(!self.signed_delta_sort),
            offset: self.offset,
            limit: self.limit,
            include_ascii: Some(true),
        })
    }

    /// Side-specific overrides are merged with the shared `--root` /
    /// `--traversal` ones, so `--root x --right-root y` roots the right graph
    /// at both.
    fn build_query_config(
        &self,
        handle: &str,
        side_roots: &[String],
        side_traversal: &Option<String>,
    ) -> anyhow::Result<GraphQueryConfig> {
        let handle: GraphHandle = handle.parse().context("Failed to parse graph handle")?;

        let roots: BTreeSet<String> = self
            .roots
            .iter()
            .chain(side_roots)
            .cloned()
            .collect::<BTreeSet<_>>();

        let traversal = side_traversal
            .as_ref()
            .or(self.traversal.as_ref())
            .map(|t| t.parse())
            .transpose()
            .context("Failed to parse traversal config key")?
            .map(TraversalOverride::Key);

        Ok(GraphQueryConfig {
            handle,
            roots: (!roots.is_empty()).then_some(roots),
            traversal,
        })
    }

    fn build_target(&self) -> ExploreGraphTarget {
        if self.all_nodes {
            ExploreGraphTarget::AllNodes {}
        } else if let Some(ref name) = self.node {
            ExploreGraphTarget::Node { name: name.clone() }
        } else {
            ExploreGraphTarget::EntryPoints {}
        }
    }

    fn parse_metrics(&self) -> anyhow::Result<Option<Vec<MetricColumn>>> {
        if self.metrics.is_empty() {
            return Ok(None);
        }
        let metrics = self
            .metrics
            .iter()
            .map(|s| s.parse::<MetricColumn>())
            .collect::<Result<Vec<_>, _>>()
            .context("Failed to parse metric")?;
        Ok(Some(metrics))
    }

    fn parse_sort_by(&self) -> anyhow::Result<Option<MetricColumn>> {
        self.sort_by
            .as_deref()
            .map(|s| s.parse::<MetricColumn>())
            .transpose()
            .context("Failed to parse sort-by metric")
    }
}
