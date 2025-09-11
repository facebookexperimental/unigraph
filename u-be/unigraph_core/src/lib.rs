// Copyright (c) Meta Platforms, Inc. and affiliates.

#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_else_if)]
#![feature(once_cell_try)]
mod array_graph_serializable;
pub mod map_graph_builder;
mod test_graph;
pub(crate) mod traversal;
pub mod types;

pub use crate::array_graph_serializable::ArrayGraphSerializable;
pub use crate::array_graph_serializable::ArrayGraphSerializableEdges;
pub use crate::array_graph_serializable::ArrayGraphSerializableNodeMetadata;
pub use crate::array_graph_serializable::package::ArrayGraphSerializableManifest;
pub use crate::array_graph_serializable::package::ArrayGraphSerializablePackage;
pub use crate::array_graph_serializable::package::ArrayGraphSerializablePackageBase64;
pub use crate::array_graph_serializable::package::ArrayGraphSerializablePackageConfig;
pub use crate::array_graph_serializable::package::BlobID;
pub use crate::array_graph_serializable::package::ManifestBlobs;
pub use crate::array_graph_serializable::package::ManifestStats;
pub use crate::array_graph_serializable::package::into_blobs;
pub use crate::map_graph_builder::GraphBuilder;
pub use crate::test_graph::make_test_graph;
pub use crate::traversal::Decision;
pub use crate::traversal::ForceDynamic;
pub use crate::traversal::NodeTagSetsPredicate;
pub use crate::traversal::TraversalConfig;
pub use crate::traversal::tiered_traversal::AscendingTier;
pub use crate::traversal::tiered_traversal::AscendingTiersConfig;
pub use crate::traversal::tiered_traversal::TieredTraversalConfig;
pub use crate::types::NodeIDX;
pub use crate::types::array_graph::ArrayGraph;
pub use crate::types::array_graph::ArrayGraphDynamicEdge;
pub use crate::types::array_graph::Arrow;
pub use crate::types::array_graph::array_graph_debug_utils::ArrayGraphDebugUtils;
pub use crate::types::array_graph::array_graph_nodes::ArrayGraphNodes;
pub use crate::types::array_graph::array_graph_nodes::GraphSide;
pub use crate::types::array_graph::graph_settings;
pub use crate::types::array_graph::graph_settings::GraphSettings;
pub use crate::types::array_graph::offset_graph::TraversalType;
pub use crate::types::array_graph::remap_utils;
pub use crate::types::map_graph::MapGraph;
pub use crate::types::twin_graph::TwinGraph;
pub use crate::types::twin_graph::get_arrows::TwinArrow;
pub use crate::types::ui_types;
#[cfg(test)]
mod tests;
