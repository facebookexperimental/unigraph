// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::HashSet;

use super::edge_flags::EdgeFlags;
use crate::types::NodeIDX;

/// DFS iterator that follows only non-excluded edges.
///
/// Works with parallel `targets` + `flags` + `edge_offsets` slices —
/// no dependency on any specific struct.
pub struct DFSConfigured<'a> {
    targets: &'a [NodeIDX],
    flags: &'a [EdgeFlags],
    edge_offsets: &'a [usize],
    stack: Vec<NodeIDX>,
    visited: HashSet<NodeIDX>,
}

impl<'a> DFSConfigured<'a> {
    pub fn new(
        targets: &'a [NodeIDX],
        flags: &'a [EdgeFlags],
        edge_offsets: &'a [usize],
        roots: &[NodeIDX],
    ) -> Self {
        DFSConfigured {
            targets,
            flags,
            edge_offsets,
            stack: roots.to_vec(),
            visited: HashSet::new(),
        }
    }
}

impl<'a> Iterator for DFSConfigured<'a> {
    type Item = NodeIDX;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(node_idx) = self.stack.pop() {
            if !self.visited.contains(&node_idx) {
                self.visited.insert(node_idx);
                let start = self.edge_offsets[node_idx];
                let end = self.edge_offsets[node_idx + 1];
                self.stack.extend(
                    self.targets[start..end]
                        .iter()
                        .zip(&self.flags[start..end])
                        .filter_map(|(&target, &flags)| {
                            (!flags.contains(EdgeFlags::EXCLUDED)).then_some(target)
                        }),
                );
                return Some(node_idx);
            }
        }
        None
    }
}

/// DFS iterator that follows ALL edges regardless of excluded flags.
pub struct DFSUnconfigured<'a> {
    targets: &'a [NodeIDX],
    edge_offsets: &'a [usize],
    stack: Vec<NodeIDX>,
    visited: HashSet<NodeIDX>,
}

impl<'a> DFSUnconfigured<'a> {
    pub fn new(
        targets: &'a [NodeIDX],
        _flags: &'a [EdgeFlags],
        edge_offsets: &'a [usize],
        roots: &[NodeIDX],
    ) -> Self {
        DFSUnconfigured {
            targets,
            edge_offsets,
            stack: roots.to_vec(),
            visited: HashSet::new(),
        }
    }
}

impl<'a> Iterator for DFSUnconfigured<'a> {
    type Item = NodeIDX;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(node_idx) = self.stack.pop() {
            if !self.visited.contains(&node_idx) {
                self.visited.insert(node_idx);
                let start = self.edge_offsets[node_idx];
                let end = self.edge_offsets[node_idx + 1];
                self.stack.extend(self.targets[start..end].iter().copied());
                return Some(node_idx);
            }
        }
        None
    }
}
