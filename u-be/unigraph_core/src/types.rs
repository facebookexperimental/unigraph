// Copyright (c) Meta Platforms, Inc. and affiliates.

pub(crate) mod array_graph;
pub mod explorer_url_params;
pub mod map_graph;
pub(crate) mod twin_graph;
pub mod ui_types;

use std::fmt::Display;
use std::ops::Add;
use std::ops::Index;
use std::ops::IndexMut;
use std::ops::Sub;

use bytemuck::Pod;
use bytemuck::Zeroable;
pub use map_graph::MapGraph;

pub type NodeName = String;

pub type MetricName = String;
pub type LabelName = String;
pub type LabelValue = String;
pub type PropertyName = String;
pub type PropertyValue = String;
pub type Tag = String;
pub type DynamicBranchName = String;
pub type DynamicTypeKey = String;
pub type DynamicEdgeName = String;
pub type TierName = String;
pub type TierIDX = usize;

/// Why is NodeIDX a u32?
/// We pass this across the WASM boundary in batch (as Vec<u64>) where
/// the NodeIDX is packed together with other data.
/// This needs NodeIDX to be consistent across all platforms. Since WASM
/// is always 32-bit we use u32 even on native platforms with usize == u64.
/// There's technically no runtime overhead of doing this and it also saves
/// memory.
///
/// Do we actually need 64-bit indices?
/// if this software is able to scale to graph with 18,446,744,073,709,551,615 nodes
/// i will personally rewrite the entire codebase to support u64.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Pod, Zeroable
)]
#[derive(serde::Deserialize, serde::Serialize, typegen::TypeGen)]
#[serde(transparent)]
#[repr(transparent)]
pub struct NodeIDX(pub u32);

impl Display for NodeIDX {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl<T> Index<NodeIDX> for Vec<T> {
    type Output = T;

    fn index(&self, idx: NodeIDX) -> &Self::Output {
        &self[idx.0 as usize]
    }
}

impl<T> Index<NodeIDX> for [T] {
    type Output = T;

    fn index(&self, idx: NodeIDX) -> &Self::Output {
        &self[idx.0 as usize]
    }
}

impl<T> IndexMut<NodeIDX> for Vec<T> {
    fn index_mut(&mut self, idx: NodeIDX) -> &mut Self::Output {
        &mut self[idx.0 as usize]
    }
}

impl<T> IndexMut<NodeIDX> for [T] {
    fn index_mut(&mut self, idx: NodeIDX) -> &mut Self::Output {
        &mut self[idx.0 as usize]
    }
}

impl From<usize> for NodeIDX {
    fn from(idx: usize) -> Self {
        NodeIDX(idx as u32)
    }
}

impl From<&usize> for NodeIDX {
    fn from(idx: &usize) -> Self {
        NodeIDX(*idx as u32)
    }
}

impl From<u32> for NodeIDX {
    fn from(idx: u32) -> Self {
        NodeIDX(idx)
    }
}

impl From<&u32> for NodeIDX {
    fn from(idx: &u32) -> Self {
        NodeIDX(*idx)
    }
}

impl Add<u32> for NodeIDX {
    type Output = NodeIDX;

    fn add(self, rhs: u32) -> Self::Output {
        NodeIDX(self.0 + rhs)
    }
}

impl Sub<u32> for NodeIDX {
    type Output = NodeIDX;

    fn sub(self, rhs: u32) -> Self::Output {
        NodeIDX(self.0 - rhs)
    }
}

impl From<NodeIDX> for usize {
    fn from(val: NodeIDX) -> Self {
        val.0 as usize
    }
}

/// Index of an edge in a CSR's edges vec (global position in the flat targets array).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(transparent)]
#[repr(transparent)]
pub struct EdgeIDX(pub u32);

impl From<usize> for EdgeIDX {
    fn from(idx: usize) -> Self {
        EdgeIDX(idx as u32)
    }
}

impl From<EdgeIDX> for usize {
    fn from(val: EdgeIDX) -> Self {
        val.0 as usize
    }
}

/// Index into a flat `Vec<EdgeMeta>` table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(transparent)]
#[repr(transparent)]
pub struct EdgeMetaIDX(pub u32);

impl From<usize> for EdgeMetaIDX {
    fn from(idx: usize) -> Self {
        EdgeMetaIDX(idx as u32)
    }
}

impl From<EdgeMetaIDX> for usize {
    fn from(val: EdgeMetaIDX) -> Self {
        val.0 as usize
    }
}
