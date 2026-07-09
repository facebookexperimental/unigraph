// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Serializable representation of an [`ArrayGraph`].
//!
//! This module defines [`ArrayGraphSerializable`], a format-agnostic snapshot
//! of the graph that can be round-tripped through JSON (or any serde format)
//! and converted back into a live [`ArrayGraph`].
//!
//! ## Sub-modules
//!
//! - [`delta`] — incremental graph updates (add/remove nodes and edges).
//! - [`package`] — chunked, ZSTD-compressed blob packaging for efficient
//!   storage and transport (see [`ArrayGraphSerializablePackage`]).

pub mod delta;
pub mod error_package;
pub(crate) mod package;

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;

use super::ArrayGraph;
use crate::ArrayGraphNodes;
use crate::ArrayGraphSerializablePackage;
use crate::ArrayGraphSerializablePackageConfig;
use crate::TraversalConfig;
use crate::graph_settings::GraphSettings;
use crate::remap_utils::RemapContext;
use crate::remap_utils::remap_edges;
use crate::remap_utils::remap_node_metadata;
use crate::remap_utils::remap_node_names_ordered;
use crate::types::DynamicBranchName;
use crate::types::DynamicEdgeName;
use crate::types::DynamicTypeKey;
use crate::types::EdgeIDX;
use crate::types::EdgeMetaIDX;
use crate::types::LabelName;
use crate::types::LabelValue;
use crate::types::MetricName;
use crate::types::NodeIDX;
use crate::types::NodeName;
use crate::types::PropertyName;
use crate::types::PropertyValue;
use crate::types::Tag;
use crate::types::array_graph::ArrayGraphRuntime;
use crate::types::array_graph::NodeFlags;
use crate::types::array_graph::array_graph_derived_state::ArrayGraphDerivedState;
use crate::types::array_graph::array_graph_state::ArrayGraphState;
use crate::types::array_graph::offset_graph::edge_flags::EdgeFlags;

/// A serializable representation of an array graph, which can be used for
/// storing or transmitting the graph structure.
///
/// IMPORTANT: When adding or removing fields here, you MUST update ALL of
/// the following to maintain field parity:
///   - `ArrayGraph` struct (array_graph.rs)
///   - `From<ArrayGraph> for ArrayGraphSerializable`
///   - `From<ArrayGraphSerializable> for ArrayGraph`
///   - `ArrayGraphSerializable::remap()`
///   - `ManifestBlobs` struct, `pack()`, and `unpack()` (package.rs)
///   - `ManifestBlobs::get_all_blob_ids()`
///   - `apply_deltas()` (delta/apply.rs)
///   - `remap_with_nodes()` (twin_graph/merge.rs)
///   - `MapGraph::to_array_graph_serializable()` (map_graph.rs)
///   - `super_root::append_super_root()` destructure + reconstruction
#[derive(serde::Serialize, serde::Deserialize, typegen::TypeGen, Clone)]
pub struct ArrayGraphSerializable {
    pub node_names_ordered: ArrayGraphNodes,
    pub edges: ArrayGraphSerializableEdges,
    pub node_metadata: ArrayGraphSerializableNodeMetadata,

    pub graph_settings: Option<GraphSettings>,
    pub traversal_config: Option<TraversalConfig>,

    /// If present, these graph will use these entrypoints instead
    /// of automatically determining them.
    pub entry_points: Option<BTreeSet<NodeName>>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: BTreeMap<PropertyName, PropertyValue>,
}

/// Metadata for a single tagged or dynamic edge. Stored in a flat table,
/// referenced by sparse `BTreeMap<EdgeIDX, EdgeMetaIDX>` per graph.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub enum EdgeMeta {
    Tagged {
        tag: Tag,
    },
    Dynamic {
        type_key: DynamicTypeKey,
        edge_name: DynamicEdgeName,
        branch: DynamicBranchName,
        #[serde(skip_serializing_if = "Option::is_none")]
        metadata: Option<BTreeMap<String, String>>,
    },
}

/// Serializable edge data for an array graph.
///
/// ALL edges (directed, tagged, dynamic) are stored in a single CSR layout.
/// Tagged/dynamic edges have entries in `edge_metadata` + `edge_metadata_map`
/// that describe their type and properties. Directed edges have no metadata entry.
///
/// This design enables zero-cost conversion to ArrayGraph — just move the data
/// and allocate runtime flags.
#[derive(serde::Serialize, serde::Deserialize, typegen::TypeGen, Clone)]
pub struct ArrayGraphSerializableEdges {
    /// CSR targets for ALL edges (directed + tagged + dynamic).
    /// Within each node's range: directed edges first, then tagged (sorted by tag + target),
    /// then dynamic (sorted by type_key + edge_name + branch + target).
    pub edges: Vec<NodeIDX>,
    /// CSR offsets: `edges[edge_offsets[i]..edge_offsets[i+1]]` gives targets for source node `i`.
    pub edge_offsets: Vec<usize>,

    /// Flat metadata table for tagged/dynamic edges.
    /// Shared across forward/reverse/dominator graphs — derived graphs build their own
    /// sparse map but point into this same table.
    #[typegen(skip_all)]
    pub edge_metadata: Vec<EdgeMeta>,
    /// Sparse map: forward edge index → metadata table index.
    /// Only populated for tagged/dynamic edges. Directed edges have no entry.
    #[typegen(skip_all)]
    pub edge_metadata_map: BTreeMap<EdgeIDX, EdgeMetaIDX>,
}

impl ArrayGraphSerializableEdges {
    /// Number of edges in the CSR.
    pub fn edges_len(&self) -> usize {
        self.edges.len()
    }

    /// Number of nodes (derived from offsets).
    pub fn node_count(&self) -> usize {
        self.edge_offsets.len() - 1
    }

    /// Get edge targets for a given node.
    pub fn edges_for_node(&self, node_idx: NodeIDX) -> &[NodeIDX] {
        let start = self.edge_offsets[node_idx];
        let end = self.edge_offsets[node_idx + 1];
        &self.edges[start..end]
    }

    /// Get the global edge index range for a node.
    pub fn edge_range(&self, node_idx: NodeIDX) -> std::ops::Range<usize> {
        self.edge_offsets[node_idx]..self.edge_offsets[node_idx + 1]
    }

    /// Look up metadata for an edge by its global index.
    /// Returns None for directed edges.
    pub fn edge_meta(&self, edge_idx: EdgeIDX) -> Option<&EdgeMeta> {
        self.edge_metadata_map
            .get(&edge_idx)
            .map(|&meta_idx| &self.edge_metadata[usize::from(meta_idx)])
    }

    /// Reconstruct grouped tagged edges for a single node.
    /// Used by delta derive, to_map_graph, get_arrows, etc.
    pub fn tagged_edges_for_node(&self, node_idx: NodeIDX) -> BTreeMap<&str, BTreeSet<NodeIDX>> {
        let range = self.edge_range(node_idx);
        let mut result: BTreeMap<&str, BTreeSet<NodeIDX>> = BTreeMap::new();
        for edge_idx in range {
            if let Some(EdgeMeta::Tagged { tag }) = self
                .edge_metadata_map
                .get(&EdgeIDX::from(edge_idx))
                .map(|&meta_idx| &self.edge_metadata[usize::from(meta_idx)])
            {
                result.entry(tag).or_default().insert(self.edges[edge_idx]);
            }
        }
        result
    }

    /// Reconstruct grouped dynamic edges for a single node.
    pub fn dynamic_edges_for_node(
        &self,
        node_idx: NodeIDX,
    ) -> BTreeMap<&str, BTreeMap<&str, DynamicEdgeView<'_>>> {
        let range = self.edge_range(node_idx);
        let mut result: BTreeMap<&str, BTreeMap<&str, DynamicEdgeView<'_>>> = BTreeMap::new();
        for edge_idx in range {
            if let Some(EdgeMeta::Dynamic {
                type_key,
                edge_name,
                branch,
                metadata,
            }) = self
                .edge_metadata_map
                .get(&EdgeIDX::from(edge_idx))
                .map(|&meta_idx| &self.edge_metadata[usize::from(meta_idx)])
            {
                let target = self.edges[edge_idx];
                result
                    .entry(type_key)
                    .or_default()
                    .entry(edge_name)
                    .or_insert_with(|| DynamicEdgeView {
                        branches: BTreeMap::new(),
                        metadata: metadata.as_ref(),
                    })
                    .branches
                    .entry(branch)
                    .or_default()
                    .insert(target);
            }
        }
        result
    }

    /// Remaps all node indices in this edge set according to the given context.
    pub fn remap(&self, ctx: &RemapContext) -> Result<Self> {
        remap_edges(self, ctx).context("Failed to remap ArrayGraphSerializableEdges")
    }
}

/// Borrowed view of a dynamic edge's branches and metadata,
/// reconstructed on-the-fly from the flat metadata table.
#[derive(Debug)]
pub struct DynamicEdgeView<'a> {
    pub branches: BTreeMap<&'a str, BTreeSet<NodeIDX>>,
    pub metadata: Option<&'a BTreeMap<String, String>>,
}

/// Serializable per-node metadata: numeric metrics, categorical labels, and string properties.
#[derive(serde::Serialize, serde::Deserialize, typegen::TypeGen, Clone)]
pub struct ArrayGraphSerializableNodeMetadata {
    /// Named metrics — each entry maps a metric name to a `Vec<f32>` with one
    /// value per node (indexed by [`NodeIDX`]).
    pub metrics: BTreeMap<MetricName, Vec<f32>>,
    /// Per-label-name index — maps a label name to the set of nodes that have it,
    /// and for each node the set of values for that label.
    pub labels: BTreeMap<LabelName, BTreeMap<NodeIDX, BTreeSet<LabelValue>>>,
    /// Per-property-name index — maps a property name to the set of nodes that have it,
    /// and for each node the single value for that property.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: BTreeMap<PropertyName, BTreeMap<NodeIDX, PropertyValue>>,
}

impl ArrayGraphSerializableNodeMetadata {
    /// Remaps all node indices in the metadata according to the given context.
    pub fn remap(&self, ctx: &RemapContext) -> Result<Self> {
        remap_node_metadata(self, ctx).context("Failed to remap ArrayGraphSerializableNodeMetadata")
    }

    /// Collect all labels for a specific node from the inverted labels index.
    pub fn labels_for_node(&self, node_idx: NodeIDX) -> BTreeMap<&str, &BTreeSet<LabelValue>> {
        self.labels
            .iter()
            .filter_map(|(label_name, node_map)| {
                node_map
                    .get(&node_idx)
                    .map(|values| (label_name.as_str(), values))
            })
            .collect()
    }

    /// Collect all properties for a specific node from the inverted properties index.
    pub fn properties_for_node(&self, node_idx: NodeIDX) -> BTreeMap<&str, &str> {
        self.properties
            .iter()
            .filter_map(|(prop_name, node_map)| {
                node_map
                    .get(&node_idx)
                    .map(|value| (prop_name.as_str(), value.as_str()))
            })
            .collect()
    }
}

impl ArrayGraphSerializable {
    /// Returns an iterator over all valid node indices in this graph.
    pub fn node_idx_iter(&self) -> impl Iterator<Item = NodeIDX> {
        (0..self.node_names_ordered.len()).map(NodeIDX::from)
    }

    /// Converts this serializable representation back into an `ArrayGraph`.
    ///
    /// Zero-cost move: the CSR data stays in `data`, and we allocate only
    /// the runtime edge flags (populated from the sparse metadata map) and
    /// the OffsetGraph forward-edge view.
    pub fn into_array_graph(self, task: &ll::Task) -> Result<ArrayGraph> {
        Self::array_graph_from_shared(Arc::new(self), task)
    }

    /// Builds an `ArrayGraph` that SHARES this data, without consuming it.
    ///
    /// The runtime state is derived purely from reads of `data`, so many
    /// `ArrayGraph`s can be built from one `Arc<ArrayGraphSerializable>`
    /// (e.g. one per thread) — each gets its own mutable `runtime` while the
    /// heavy payload is shared via a cheap refcount clone, no deep copy.
    pub fn array_graph_from_shared(
        data: Arc<ArrayGraphSerializable>,
        _task: &ll::Task,
    ) -> Result<ArrayGraph> {
        let node_count = data.node_names_ordered.len();
        let edge_count = data.edges.edges.len();

        let tiers = data
            .traversal_config
            .as_ref()
            .map_or_else(Default::default, |config| config.get_tiers());

        // Build per-edge flags from the sparse metadata map.
        let mut edge_flags = vec![EdgeFlags::empty(); edge_count];
        for (&edge_idx, &meta_idx) in &data.edges.edge_metadata_map {
            let flag = match &data.edges.edge_metadata[usize::from(meta_idx)] {
                EdgeMeta::Tagged { .. } => EdgeFlags::IS_TAGGED,
                EdgeMeta::Dynamic { .. } => EdgeFlags::IS_DYNAMIC,
            };
            edge_flags[usize::from(edge_idx)] = flag;
        }

        Ok(ArrayGraph {
            runtime: ArrayGraphRuntime {
                edge_flags,
                node_flags: vec![NodeFlags::empty(); node_count],
                derived_state: ArrayGraphDerivedState::new(),
                state: ArrayGraphState {
                    traversal_config: data.traversal_config.clone(),
                    indexed_messages: Default::default(),
                    tiers,
                },
            },
            data,
        })
    }

    /// Serializes this graph to a JSON string.
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(self).context("Failed to serialize ArrayGraphSerializable to JSON")
    }

    /// Deserializes a graph from a JSON string.
    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json).context("Failed to deserialize ArrayGraphSerializable from JSON")
    }

    /// Deserializes a graph from raw JSON bytes.
    pub fn from_json_bytes(json: &[u8]) -> Result<Self> {
        serde_json::from_slice(json)
            .context("Failed to deserialize ArrayGraphSerializable from JSON bytes")
    }

    /// Packs this graph into compressed, chunked blobs for storage/transport.
    ///
    /// Each field is serialized, compressed, and chunked in parallel via rayon
    /// (degrades to sequential on WASM).
    pub fn pack(
        &self,
        config: &ArrayGraphSerializablePackageConfig,
        task: &ll::Task,
    ) -> Result<ArrayGraphSerializablePackage> {
        package::pack(self, config, task).context("Failed to pack graph")
    }

    pub fn unpack(
        package: &ArrayGraphSerializablePackage,
        task: &ll::Task,
    ) -> Result<ArrayGraphSerializable> {
        package::unpack(package, task).context("Failed to unpack graph")
    }

    /// Remaps node indices throughout the entire graph (names, edges, metadata)
    /// according to the given [`RemapContext`].
    pub fn remap(self, ctx: &RemapContext) -> Result<Self> {
        Ok(ArrayGraphSerializable {
            node_names_ordered: remap_node_names_ordered(&self.node_names_ordered, ctx)?,
            edges: self.edges.remap(ctx)?,
            node_metadata: self.node_metadata.remap(ctx)?,
            graph_settings: self.graph_settings,
            traversal_config: self.traversal_config,
            entry_points: self.entry_points,
            properties: self.properties,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use anyhow::Result;

    use super::ArrayGraphSerializable;
    use crate::GraphBuilder;

    /// Fan-out: many `ArrayGraph`s built from one shared `Arc` share the payload
    /// instead of deep-copying it (provable via the strong count), each carrying
    /// its own runtime — and the shared data crosses a thread boundary.
    #[test]
    fn array_graph_from_shared_shares_without_copy() -> Result<()> {
        let task = ll::Task::create_new("test");
        let mut b = GraphBuilder::new();
        b.add_edge("A", "B")?;
        b.add_edge("B", "C")?;
        let shared = Arc::new(b.build().to_array_graph(&task)?.into_serializable());

        let graphs: Vec<_> = (0..8)
            .map(|_| ArrayGraphSerializable::array_graph_from_shared(shared.clone(), &task))
            .collect::<Result<_>>()?;

        // No deep copy: strong count == the shared handles held by each graph
        // plus our own `shared`.
        assert_eq!(Arc::strong_count(&shared), graphs.len() + 1);
        assert!(graphs.iter().all(|g| g.nodes_len() == 3));

        // Send + Sync: the shared data moves into another thread.
        let data = shared.clone();
        let n = std::thread::spawn(move || {
            let t = ll::Task::create_new("test");
            ArrayGraphSerializable::array_graph_from_shared(data, &t)
                .unwrap()
                .nodes_len()
        })
        .join()
        .unwrap();
        assert_eq!(n, 3);

        Ok(())
    }
}
