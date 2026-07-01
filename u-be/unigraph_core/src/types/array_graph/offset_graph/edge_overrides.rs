// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Per-edge include/exclude decisions applied *on top of* a graph's real
//! [`EdgeFlags`] during traversal.
//!
//! # Why
//!
//! Some features want to run a "what-if" traversal — e.g. force-include a
//! normally-excluded edge to measure what it would pull in, or force-exclude
//! an included edge to measure what dropping it would save — WITHOUT mutating
//! the graph's edge flags. Mutating `runtime.edge_flags` in place (clear a
//! flag, traverse, restore it) is error-prone: it requires `&mut`, corrupts
//! shared state if a restore is missed, and only handles one edge at a time.
//!
//! `EdgeOverrides` is a read-only overlay the traversal iterators consult
//! *first*, falling back to the edge's real flags when no override exists.
//! It holds any number of `(parent, child, follow?)` decisions at once.
//!
//! # Performance
//!
//! Overrides are stored as `parent -> child -> follow?`. Traversal code calls
//! [`EdgeOverrides::for_parent`] ONCE per popped node, then reuses the result
//! for that node's whole edge loop. For the (overwhelmingly common) case of a
//! node with no overrides, `for_parent` returns `None` and the per-edge check
//! collapses to a single flag test — no per-edge map lookup. This keeps the
//! overlay essentially free even on graphs with billions of edges.

use std::collections::BTreeMap;

use crate::types::NodeIDX;
use crate::types::array_graph::offset_graph::edge_flags::EdgeFlags;

/// A set of per-edge follow/skip decisions that take precedence over an
/// edge's [`EdgeFlags::EXCLUDED`] flag during traversal.
#[derive(Debug, Default, Clone)]
pub struct EdgeOverrides {
    /// parent -> child -> follow?  (`true` = force-include, `false` = force-exclude)
    by_parent: BTreeMap<NodeIDX, BTreeMap<NodeIDX, bool>>,
}

impl EdgeOverrides {
    /// Build from `(from, to, follow?)` triplets. Later triplets for the same
    /// edge win.
    pub fn from_triplets(triplets: impl IntoIterator<Item = (NodeIDX, NodeIDX, bool)>) -> Self {
        let mut by_parent: BTreeMap<NodeIDX, BTreeMap<NodeIDX, bool>> = BTreeMap::new();
        for (from, to, follow) in triplets {
            by_parent.entry(from).or_default().insert(to, follow);
        }
        EdgeOverrides { by_parent }
    }

    pub fn is_empty(&self) -> bool {
        self.by_parent.is_empty()
    }

    /// Overrides for edges leaving `parent`, or `None` if it has none.
    ///
    /// Call ONCE per popped node, then reuse the result across that node's
    /// edge loop with [`edge_should_be_followed`].
    #[inline]
    pub fn for_parent(&self, parent: NodeIDX) -> Option<&BTreeMap<NodeIDX, bool>> {
        self.by_parent.get(&parent)
    }
}

/// Resolve whether a single edge should be followed, given the override submap
/// for its parent (from [`EdgeOverrides::for_parent`]) and the edge's flags.
///
/// An override always wins; absent an override we follow the edge unless it is
/// marked [`EdgeFlags::EXCLUDED`].
#[inline]
pub fn edge_should_be_followed(
    parent_overrides: Option<&BTreeMap<NodeIDX, bool>>,
    target: NodeIDX,
    flags: EdgeFlags,
) -> bool {
    match parent_overrides.and_then(|m| m.get(&target)) {
        Some(&follow) => follow,
        None => !flags.contains(EdgeFlags::EXCLUDED),
    }
}
