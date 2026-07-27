// Copyright (c) Meta Platforms, Inc. and affiliates.

mod apply;
mod derive;
pub mod package;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_randomized;

pub use apply::apply_delta;
pub use apply::apply_deltas;
pub use derive::derive_delta;

pub use crate::graph_settings::GraphSettingsDelta;
pub use crate::traversal::TraversalConfigDelta;
// Re-export the auto-derived delta types from MapGraph.
// These replace the old hand-written GraphDelta, NodeEdgeDelta, etc.
pub use crate::types::map_graph::MapGraphDelta;

/// Type alias: the MapGraphDelta is our graph delta format.
pub type GraphDelta = MapGraphDelta;
