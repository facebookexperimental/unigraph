// Copyright (c) Meta Platforms, Inc. and affiliates.

pub(crate) mod array_graph;
pub(crate) mod map_graph;

use std::ops::Add;
use std::ops::Index;
use std::ops::IndexMut;
use std::ops::Sub;

use bytemuck::Pod;
use bytemuck::Zeroable;
pub use map_graph::MapGraph;

pub type NodeName = String;

pub type MetricName = String;
pub type TagSetName = String;
pub type Tag = String;
pub type DynamicBranchName = String;
pub type TierName = String;

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
#[derive(ts_rs::TS, serde::Deserialize, serde::Serialize)]
#[ts(export, as = "u32")]
#[serde(transparent)]
#[repr(transparent)]
pub struct NodeIDX(pub u32);

impl<T> Index<NodeIDX> for Vec<T> {
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
