// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeSet;

use anyhow::Context;
use clap::Parser;
use unigraph_app::ExploreGraphInput;
use unigraph_app::ExploreGraphTarget;
use unigraph_app::RpcExec;
use unigraph_app::Unigraph;
use unigraph_core::GraphHandle;
use unigraph_core::MetricView;
use unigraph_core::config_query::GraphQueryConfig;
use unigraph_core::config_query::TraversalOverride;
use unigraph_core::graph_settings::GraphStructure;
use unigraph_core::graph_settings::SortOrder;

use crate::UnigraphCLIContext;
use crate::graph::subgraph_args::NodeMatchArgs;

/// Explore graph nodes and metrics.
///
/// Shows entry points, a specific node's children, or all reachable nodes
/// with computed metrics and optional sorting/pagination.
///
/// Examples:
///
/// ```sh
/// # Show entry points of a timeline's latest graph
/// unigraph graph explore my_timeline
///
/// # Drill into a specific node
/// unigraph graph explore my_timeline --node my/component
///
/// # Show all nodes sorted by a metric
/// unigraph graph explore my_timeline --all-nodes --sort-by size~transitive
///
/// # Show every node carrying a property, or a specific value of it
/// unigraph graph explore my_timeline --match-property type=budget
/// unigraph graph explore my_timeline --match-property oncall
///
/// # Show every node whose name matches
/// unigraph graph explore my_timeline --match-name comet
/// unigraph graph explore my_timeline --match-name '^ads/' --match-mode regex
///
/// # Specific metrics, limited output
/// unigraph graph explore my_timeline --metric size --metric count~transitive --limit 20
///
/// # Override roots
/// unigraph graph explore my_timeline --root nodeA --root nodeB
///
/// # Output full JSON instead of ASCII table
/// unigraph graph explore my_timeline --json
/// ```
#[derive(Parser, Debug)]
pub struct GraphExplore {
    /// Timeline ID, graph key, or GQC key to explore.
    handle: String,

    /// Explore a specific node's children instead of entry points.
    #[arg(long)]
    node: Option<String>,

    /// Show all reachable nodes instead of entry points.
    #[arg(long, conflicts_with = "node")]
    all_nodes: bool,

    #[command(flatten)]
    match_args: NodeMatchArgs,

    /// Edge structure to follow: forward, reverse, or dominator.
    #[arg(long, default_value = "forward")]
    structure: StructureArg,

    /// Metrics to compute for each arrow (repeatable).
    ///
    /// Format: `name`, `name~transitive`, `name~dominated`,
    /// `name#TIER`, `name#TIER~dominated`, `name#TIER~conjoint`,
    /// or built-ins `parents-count`, `node-count~transitive`,
    /// `node-count~dominated`, `node-count~conjoint`.
    #[arg(long = "metric", num_args = 1)]
    metrics: Vec<String>,

    /// Metric to sort by (same format as --metric).
    #[arg(long)]
    sort_by: Option<String>,

    /// Sort direction.
    #[arg(long, default_value = "desc")]
    sort_order: SortOrderArg,

    /// Skip first N results (pagination).
    #[arg(long)]
    offset: Option<usize>,

    /// Maximum number of arrows to return (default: 50).
    #[arg(long)]
    limit: Option<usize>,

    /// Also show edges the traversal did not follow, flagged in a `status`
    /// column as `excluded` (the node is still reachable another way) or
    /// `unreachable` (it is not reachable at all). Only applies with `--node`.
    #[arg(long)]
    include_excluded: bool,

    /// Override the root nodes of the subgraph (repeatable).
    #[arg(long = "root", num_args = 1)]
    roots: Vec<String>,

    /// Traversal config key (`tvc_{hash}`) to override graph traversal.
    #[arg(long)]
    traversal: Option<String>,

    /// Output full JSON instead of ASCII table.
    #[arg(long)]
    json: bool,
}

impl GraphExplore {
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

impl GraphExplore {
    fn build_input(&self) -> anyhow::Result<ExploreGraphInput> {
        let handle: GraphHandle = self
            .handle
            .parse()
            .context("Failed to parse graph handle")?;
        let query = self.build_query_config(handle)?;
        let target = self.build_target()?;
        let metrics = self.parse_metrics()?;
        let sort_by = self.parse_sort_by()?;

        Ok(ExploreGraphInput {
            query,
            target,
            graph_structure: self.structure.into(),
            metrics: if metrics.is_empty() {
                None
            } else {
                Some(metrics)
            },
            sort_by,
            sort_order: Some(self.sort_order.into()),
            offset: self.offset,
            limit: self.limit,
            include_excluded: Some(self.include_excluded),
            include_ascii: Some(true),
        })
    }

    fn build_query_config(&self, handle: GraphHandle) -> anyhow::Result<GraphQueryConfig> {
        let roots = if self.roots.is_empty() {
            None
        } else {
            Some(self.roots.iter().cloned().collect::<BTreeSet<_>>())
        };

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

    fn build_target(&self) -> anyhow::Result<ExploreGraphTarget> {
        if let Some(selection) = self.match_args.build()? {
            return Ok(ExploreGraphTarget::Matching { selection });
        }
        Ok(if self.all_nodes {
            ExploreGraphTarget::AllNodes {}
        } else if let Some(ref name) = self.node {
            ExploreGraphTarget::Node { name: name.clone() }
        } else {
            ExploreGraphTarget::EntryPoints {}
        })
    }

    fn parse_metrics(&self) -> anyhow::Result<Vec<MetricView>> {
        self.metrics
            .iter()
            .map(|s| s.parse::<MetricView>())
            .collect::<Result<Vec<_>, _>>()
            .context("Failed to parse metric")
    }

    fn parse_sort_by(&self) -> anyhow::Result<Option<MetricView>> {
        self.sort_by
            .as_deref()
            .map(|s| s.parse::<MetricView>())
            .transpose()
            .context("Failed to parse sort-by metric")
    }
}

// -- CLI arg enums ------------------------------------------------------------

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum StructureArg {
    Forward,
    Reverse,
    Dominator,
}

impl From<StructureArg> for GraphStructure {
    fn from(val: StructureArg) -> Self {
        match val {
            StructureArg::Forward => GraphStructure::Forward,
            StructureArg::Reverse => GraphStructure::Reverse,
            StructureArg::Dominator => GraphStructure::Dominator,
        }
    }
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum SortOrderArg {
    Asc,
    Desc,
}

impl From<SortOrderArg> for SortOrder {
    fn from(val: SortOrderArg) -> Self {
        match val {
            SortOrderArg::Asc => SortOrder::Asc,
            SortOrderArg::Desc => SortOrder::Desc,
        }
    }
}
