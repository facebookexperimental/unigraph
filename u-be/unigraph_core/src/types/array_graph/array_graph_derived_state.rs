// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::sync::OnceLock;

use crate::NodeIDX;
use crate::types::array_graph::offset_graph::OffsetGraph;

/// State of the graphs that is derived from other state and likely
/// needs to be recomputed when we chantge underlying data (e.g.
/// when we modify the traversal configuration)
pub struct ArrayGraphDerivedState {
    pub edges_reverse: OnceLock<OffsetGraph>,
    /// Dominator tree is pretty expensive to compute and we normally only
    /// need it for when dominator tree views are enabled in the UI. We'll store
    /// it in a OnceLock so that it is computed lazyly and only when needed.
    pub edges_dom: OnceLock<OffsetGraph>,

    pub sccs: OnceLock<Vec<Vec<NodeIDX>>>,
}

impl ArrayGraphDerivedState {
    pub fn new() -> Self {
        Self {
            edges_reverse: OnceLock::new(),
            edges_dom: OnceLock::new(),
            sccs: OnceLock::new(),
        }
    }
}
