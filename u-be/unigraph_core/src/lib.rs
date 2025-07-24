// Copyright (c) Meta Platforms, Inc. and affiliates.

#![allow(clippy::collapsible_if)]

pub mod map_graph_builder;
mod test_graph;
pub(crate) mod traversal;
pub mod types;

pub use map_graph_builder::GraphBuilder;
pub use test_graph::make_test_graph;
pub use traversal::Decision;
pub use traversal::ForceDynamic;
pub use traversal::NodeTagSetsPredicate;
pub use traversal::TraversalConfig;
pub use traversal::tiered_traversal::AscendingTier;
pub use traversal::tiered_traversal::AscendingTiersConfig;
pub use traversal::tiered_traversal::TieredTraversalConfig;
pub use types::NodeIDX;
pub use types::array_graph::ArrayGraph;
pub use types::array_graph::ArrayGraphDynamicEdge;
pub use types::array_graph::array_graph_debug_utils::ArrayGraphDebugUtils;
pub use types::array_graph::array_graph_nodes::ArrayGraphNodes;
pub use types::array_graph::array_graph_serializable::ArrayGraphSerializable;
pub use types::array_graph::array_graph_serializable::ArrayGraphSerializableEdges;
pub use types::array_graph::array_graph_serializable::ArrayGraphSerializableNodeMetadata;
pub use types::array_graph::graph_settings;
pub use types::array_graph::remap_utils;
pub use types::map_graph::MapGraph;

#[cfg(test)]
mod tests;
