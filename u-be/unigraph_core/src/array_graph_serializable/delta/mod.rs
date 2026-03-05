// Copyright (c) Meta Platforms, Inc. and affiliates.

mod apply;
mod derive;
pub mod package;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::collections::BTreeSet;

pub use apply::apply_delta;
pub use apply::apply_deltas;
pub use derive::derive_delta;

use crate::TraversalConfig;
use crate::graph_settings::GraphSettings;
use crate::types::DynamicBranchName;
use crate::types::MetricName;
use crate::types::NodeName;
use crate::types::Tag;
use crate::types::TagSetName;

/// A self-contained diff between two `ArrayGraphSerializable` graphs.
///
/// Uses node names (not `NodeIDX`) so the delta can be:
/// - Inspected and debugged independently of the base graph
/// - Serialized and stored in a database
/// - Batched with other deltas for efficient multi-delta application
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GraphDelta {
    /// Nodes added (sorted). These names must not exist in the base graph.
    pub nodes_added: Vec<NodeName>,

    /// Nodes removed (sorted). These names must exist in the base graph.
    /// Removing a node implicitly removes all its edges and metadata.
    pub nodes_removed: Vec<NodeName>,

    /// Edge changes keyed by source node name.
    /// Only nodes whose edges actually changed are present.
    pub edge_changes: BTreeMap<NodeName, NodeEdgeDelta>,

    /// Metric value changes. Each entry maps a metric name to a list of
    /// per-node value changes. Values are full replacements, not arithmetic deltas.
    pub metric_changes: BTreeMap<MetricName, Vec<MetricNodeChange>>,

    /// Tag set changes keyed by node name.
    pub tag_set_changes: BTreeMap<NodeName, TagSetDelta>,

    /// Graph settings change. None = unchanged, Some(None) = cleared,
    /// Some(Some(v)) = replaced with v.
    pub graph_settings: Option<Option<GraphSettings>>,

    /// Traversal config change. Same semantics as graph_settings.
    pub traversal_config: Option<Option<TraversalConfig>>,

    /// Entry points change. Same semantics as graph_settings.
    pub entry_points: Option<Option<BTreeSet<NodeName>>>,
}

impl GraphDelta {
    pub fn is_empty(&self) -> bool {
        self.nodes_added.is_empty()
            && self.nodes_removed.is_empty()
            && self.edge_changes.is_empty()
            && self.metric_changes.is_empty()
            && self.tag_set_changes.is_empty()
            && self.graph_settings.is_none()
            && self.traversal_config.is_none()
            && self.entry_points.is_none()
    }
}

/// Edge changes for a single source node.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NodeEdgeDelta {
    pub directed: Option<DirectedEdgeDelta>,
    pub tagged: Option<TaggedEdgeDelta>,
    pub dynamic: Option<DynamicEdgeDelta>,
}

/// Individual directed edge additions and removals.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DirectedEdgeDelta {
    pub added: BTreeSet<NodeName>,
    pub removed: BTreeSet<NodeName>,
}

/// Tagged edge changes organized by tag name.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TaggedEdgeDelta {
    pub changes: BTreeMap<Tag, TaggedEdgeTagDelta>,
}

/// Individual tagged edge additions and removals for one tag.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TaggedEdgeTagDelta {
    pub added: BTreeSet<NodeName>,
    pub removed: BTreeSet<NodeName>,
}

/// Full replacement of dynamic edges for a source node.
/// Dynamic edges have complex structure (branches + properties) with
/// no stable identity, so we replace the entire set per source node.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DynamicEdgeDelta {
    pub replacement: Vec<DynamicEdgeSerialized>,
}

/// A dynamic edge using node names instead of NodeIDX.
#[derive(
    Debug,
    Clone,
    serde::Serialize,
    serde::Deserialize,
    PartialEq,
    Eq,
    PartialOrd,
    Ord
)]
pub struct DynamicEdgeSerialized {
    pub branches: BTreeMap<DynamicBranchName, BTreeSet<NodeName>>,
    pub properties: BTreeMap<String, String>,
}

/// A single node's metric value change for a specific metric.
/// This is a full replacement, not an arithmetic delta.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MetricNodeChange {
    pub node_name: NodeName,
    pub value: f32,
}

/// Tag set changes for a single node.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TagSetDelta {
    pub changes: BTreeMap<TagSetName, TagSetValueDelta>,
}

/// Individual tag additions and removals within a single tag set.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TagSetValueDelta {
    pub added: BTreeSet<Tag>,
    pub removed: BTreeSet<Tag>,
}
