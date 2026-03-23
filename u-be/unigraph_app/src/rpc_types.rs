// Copyright (c) Meta Platforms, Inc. and affiliates.

//! RPC input/output type definitions for Unigraph.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde::Serialize;
use typegen::TypeGen;
use unigraph_core::ArrayGraphSerializablePackageBase64;
use unigraph_core::DynamicEdgeInfo;
use unigraph_core::TraversalConfig;
use unigraph_core::config_key::GraphQueryConfigKey;
use unigraph_core::config_key::TraversalConfigKey;
use unigraph_core::config_query::GraphQueryConfig;
use unigraph_core::graph_settings::GraphStructure;
use unigraph_core::graph_settings::SortOrder;
use unigraph_storage_core::TimelineID;

// ── PutConfigs ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct PutConfigsInput {
    pub traversal_configs: Vec<TraversalConfig>,
    pub graph_query_configs: Vec<GraphQueryConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct PutConfigsOutput {
    pub traversal_configs: Vec<TraversalConfigKey>,
    pub graph_query_configs: Vec<GraphQueryConfigKey>,
}

// ── GetConfigs ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct GetConfigsInput {
    pub traversal_configs: Vec<TraversalConfigKey>,
    pub graph_query_configs: Vec<GraphQueryConfigKey>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct GetConfigsOutput {
    pub traversal_configs: Vec<TraversalConfig>,
    pub graph_query_configs: Vec<GraphQueryConfig>,
}

// ── GraphQuery ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct GraphQueryInput {
    /// Inline graph query config. Either this or `graph_query_config_key` must be set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph_query_config: Option<GraphQueryConfig>,
    /// Key referencing a stored graph query config. Resolved server-side.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph_query_config_key: Option<GraphQueryConfigKey>,
}

#[derive(Debug, Serialize, Deserialize, TypeGen)]
pub struct GraphQueryOutput {
    pub package: ArrayGraphSerializablePackageBase64,
    pub graph_query_config: GraphQueryConfig,
}

// ── ListTimelines ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct ListTimelinesInput {}

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct ListTimelinesOutput {
    pub timeline_ids: Vec<TimelineID>,
}

// ── SelectFrames ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct SelectFramesInput {
    pub timeline_id: TimelineID,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame_types: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct SelectFramesOutput {
    pub frames: Vec<FrameInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct FrameInfo {
    pub graph_id: i64,
    pub timestamp: String,
    pub frame_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
}

// ── ExploreGraph ──────────────────────────────────────────────

/// Specifies which metric to compute for each arrow.
/// Each variant maps to a key in the flat `metrics` map on `ExploreGraphArrow`.
#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub enum NodeMetric {
    /// Self metric value. Key: `"{name}"`.
    Metric { name: String },
    /// Transitive metric sum (DFS over forward edges). Key: `"{name}_transitive"`.
    MetricTransitive { name: String },
    /// Dominated metric sum (DFS over dominator tree). Key: `"{name}_dominated"`.
    MetricDominated { name: String },
    /// Tiered transitive metric (cumulative at tier). Key: `"{name}_{tier}"`.
    MetricTiered { name: String, tier: String },
    /// Number of configured parents. Key: `"parents_count"`.
    ParentsCount {},
    /// Number of children in current graph structure. Key: `"children_count"`.
    ChildrenCount {},
    /// Transitive dependency count (forward DFS). Key: `"count_transitive"`.
    CountTransitive {},
    /// Dominated dependency count (dominator tree DFS). Key: `"count_dominated"`.
    CountDominated {},
}

impl NodeMetric {
    /// Returns the flat map key for this metric variant.
    pub fn key(&self) -> String {
        match self {
            NodeMetric::Metric { name } => name.clone(),
            NodeMetric::MetricTransitive { name } => format!("{name}_transitive"),
            NodeMetric::MetricDominated { name } => format!("{name}_dominated"),
            NodeMetric::MetricTiered { name, tier } => format!("{name}_{tier}"),
            NodeMetric::ParentsCount {} => "parents_count".to_string(),
            NodeMetric::ChildrenCount {} => "children_count".to_string(),
            NodeMetric::CountTransitive {} => "count_transitive".to_string(),
            NodeMetric::CountDominated {} => "count_dominated".to_string(),
        }
    }
}

/// What to explore.
#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub enum ExploreGraphTarget {
    /// Auto-detected entry points (nodes with no parents).
    EntryPoints {},
    /// Drill into a specific node's children.
    Node { name: String },
    /// Flat list of all reachable nodes.
    AllNodes {},
}

impl Default for ExploreGraphTarget {
    fn default() -> Self {
        Self::EntryPoints {}
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct ExploreGraphInput {
    /// Inline graph query config. Either this or `graph_query_config_key` must be set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph_query_config: Option<GraphQueryConfig>,
    /// Key referencing a stored graph query config. Resolved server-side.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph_query_config_key: Option<GraphQueryConfigKey>,

    /// What to explore: entry points, a specific node, or all nodes.
    #[serde(default)]
    pub target: ExploreGraphTarget,

    /// Which edge structure to follow.
    #[serde(default)]
    pub graph_structure: GraphStructure,

    /// Which metrics to compute for each arrow.
    #[serde(default)]
    pub metrics: Vec<NodeMetric>,

    /// Metric to sort arrows by. Computed for all children (even beyond limit).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort_by: Option<NodeMetric>,

    /// Sort order. Defaults to Desc.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<SortOrder>,

    /// Skip first N results (for pagination).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<usize>,

    /// Maximum number of arrows to return. Defaults to 50.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,

    /// When true, populate the `ascii` field in the response with a human-readable
    /// ASCII table of the results (optimized for agent / LLM consumption).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_ascii: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct ExploreGraphOutput {
    /// The node being explored, with its own metrics. None when showing entry points.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<ExploreGraphArrow>,
    /// Arrows to children (or to entry points when node is None).
    pub arrows: Vec<ExploreGraphArrow>,
    /// Available metric names in this graph.
    pub metric_names: Vec<String>,
    /// Tier names if tiered traversal is configured.
    pub tier_names: Vec<String>,
    /// Total number of arrows before offset/limit.
    pub total_arrows_count: usize,
    /// Human-readable ASCII table of the results. Only populated when
    /// `include_ascii` is set to true in the request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ascii: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct ExploreGraphArrow {
    /// Node name.
    pub name: String,
    /// Flat metrics map. Keys follow naming conventions:
    /// - "{metric}" — self value
    /// - "{metric}_transitive" — transitive sum
    /// - "{metric}_dominated" — dominated sum
    /// - "{metric}_{tier}" — tiered transitive (if tiers configured)
    /// - "parents_count" — number of configured parents
    /// - "children_count" — number of children in current graph structure
    pub metrics: BTreeMap<String, f32>,
    /// Edge tag (e.g. "lazy"), if this is a tagged edge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// Dynamic edge info, if this is a dynamic edge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dynamic: Option<DynamicEdgeInfo>,
}
