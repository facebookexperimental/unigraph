// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Query configuration types for graph exploration.
//!
//! `GraphQueryConfig` bundles roots, traversal settings, and a graph handle
//! into a single config for querying a graph.

use std::collections::BTreeSet;

use crate::traversal::TraversalConfig;
use crate::types::NodeName;

/// Configuration for querying a graph — roots to start from, traversal rules,
/// and which graph to query.
#[derive(
    serde::Serialize,
    serde::Deserialize,
    Debug,
    Default,
    Clone,
    PartialEq,
    typegen::TypeGen,
    unigraph_delta::Deltable
)]
pub struct GraphQueryConfig {
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    #[serde(default)]
    pub roots: BTreeSet<NodeName>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub traversal_config: Option<TraversalConfig>,

    /// Graph target: timeline ID (`"my_timeline"`) for latest, or
    /// `"my_timeline~123"` for a specific snapshot.
    /// Uses the same format as `GraphKeyOrTimelineID`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handle: Option<String>,
}
