// Copyright (c) Meta Platforms, Inc. and affiliates.

mod apply;
mod derive;
pub mod package;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_randomized;

use std::collections::BTreeMap;
use std::collections::BTreeSet;

pub use apply::apply_delta;
pub use apply::apply_deltas;
pub use derive::derive_delta;
use unigraph_delta::Deltable;
use unigraph_delta::OptionDelta;
use unigraph_delta::SetDelta;

use crate::TraversalConfig;
use crate::graph_settings::GraphSettings;
pub use crate::graph_settings::GraphSettingsDelta;
pub use crate::traversal::TraversalConfigDelta;
use crate::types::DynamicBranchName;
use crate::types::DynamicEdgeName;
use crate::types::DynamicTypeKey;
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

    /// Field-level delta for graph settings.
    /// Unchanged = no change, Cleared = cleared, Set = full value, Changed = field-level delta.
    #[serde(default, skip_serializing_if = "OptionDelta::is_unchanged")]
    pub graph_settings: OptionDelta<GraphSettings, GraphSettingsDelta>,

    /// Field-level delta for traversal config.
    /// Unchanged = no change, Cleared = cleared, Set = full value, Changed = field-level delta.
    #[serde(default, skip_serializing_if = "OptionDelta::is_unchanged")]
    pub traversal_config: OptionDelta<TraversalConfig, TraversalConfigDelta>,

    /// Entry points change. Unchanged = no change, Cleared = cleared,
    /// Set(v) = replaced.
    #[serde(default, skip_serializing_if = "OptionDelta::is_unchanged")]
    pub entry_points: OptionDelta<BTreeSet<NodeName>>,
}

impl GraphDelta {
    pub fn is_empty(&self) -> bool {
        self.nodes_added.is_empty()
            && self.nodes_removed.is_empty()
            && self.edge_changes.is_empty()
            && self.metric_changes.is_empty()
            && self.tag_set_changes.is_empty()
            && self.graph_settings.is_unchanged()
            && self.traversal_config.is_unchanged()
            && self.entry_points.is_unchanged()
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
pub type DirectedEdgeDelta = SetDelta<NodeName>;

/// The full tagged edge map type for a single source node (serialized with node names).
pub type TaggedEdgesMap = BTreeMap<Tag, BTreeSet<NodeName>>;

/// Recursive delta for tagged edges of a single source node.
///
/// Maps operate at the tag level (added/removed/changed tags), and within each
/// changed tag, a `SetDelta<NodeName>` tracks individual target additions/removals.
pub type TaggedEdgeDelta = <TaggedEdgesMap as Deltable>::Delta;

/// The full dynamic edge map type for a single source node (serialized with node names).
pub type DynamicEdgesMap =
    BTreeMap<DynamicTypeKey, BTreeMap<DynamicEdgeName, DynamicEdgeSerialized>>;

/// Recursive delta for dynamic edges of a single source node.
///
/// Diffs all the way down through nested BTreeMaps and BTreeSets:
/// - Outer map: added/removed/changed `DynamicTypeKey` entries
/// - Inner map: added/removed/changed `DynamicEdgeName` entries
/// - Per-edge: field-level delta (branches, metadata)
pub type DynamicEdgeDelta = <DynamicEdgesMap as Deltable>::Delta;

/// A dynamic edge using node names instead of NodeIDX.
#[derive(
    Debug,
    Default,
    Clone,
    serde::Serialize,
    serde::Deserialize,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    unigraph_delta::Deltable
)]
pub struct DynamicEdgeSerialized {
    pub branches: BTreeMap<DynamicBranchName, BTreeSet<NodeName>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<BTreeMap<String, String>>,
}

/// A single node's metric value change for a specific metric.
/// This is a full replacement, not an arithmetic delta.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MetricNodeChange {
    pub node_name: NodeName,
    pub value: f32,
}

/// The full tag set map type for a single node.
pub type TagSetsMap = BTreeMap<TagSetName, BTreeSet<Tag>>;

/// Tag set changes for a single node.
///
/// Maps operate at the tag set name level, and within each changed tag set,
/// a `SetDelta<Tag>` tracks individual tag additions/removals.
pub type TagSetDelta = <TagSetsMap as Deltable>::Delta;
