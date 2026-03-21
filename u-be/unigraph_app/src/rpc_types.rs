// Copyright (c) Meta Platforms, Inc. and affiliates.

//! RPC input/output type definitions for Unigraph.

use serde::Deserialize;
use serde::Serialize;
use typegen::TypeGen;
use unigraph_core::ArrayGraphSerializablePackageBase64;
use unigraph_core::TraversalConfig;
use unigraph_core::config_key::GraphQueryConfigKey;
use unigraph_core::config_key::TraversalConfigKey;
use unigraph_core::config_query::GraphQueryConfig;
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
