// Copyright (c) Meta Platforms, Inc. and affiliates.

mod merge;

use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;

use crate::ArrayGraph;
use crate::ArrayGraphNodes;
use crate::ArrayGraphSerializable;
use crate::types::array_graph::array_graph_nodes::GraphSide;

const MISSING_RIGHT_ERROR: &str = "TwinGraph: You are trying to access the right graph, but it is not present. \
     Please ensure that the TwinGraph was initialized with a right graph.";

/// TwinGraph is a struct that represent a pair of graph that we normally
/// compare to each other.
/// The most common use case is to compare dependency graphs of the same codebase
/// at two different points in time, e.g. before and after a refactor/pull request/commit.
/// We use this struct as a first class citizen even if we only have one graph.
#[readonly::make]
pub struct TwinGraph {
    /// Left graph must always be present.
    #[readonly]
    pub l: ArrayGraph,
    pub r: Option<ArrayGraph>,
    #[readonly]
    pub node_names: Arc<ArrayGraphNodes>,
}

impl TwinGraph {
    pub fn from_one(l: ArrayGraph) -> Result<Self> {
        Ok(Self {
            node_names: Arc::clone(&l.nodes.node_names),
            l,
            r: None,
        })
    }

    pub fn from_two(l: ArrayGraphSerializable, r: ArrayGraphSerializable) -> Result<Self> {
        merge::merge_into_twin(l, r)
    }

    pub fn graph(&self, side: GraphSide) -> Result<&ArrayGraph> {
        match side {
            GraphSide::Left => Ok(&self.l),
            GraphSide::Right => self.r.as_ref().context(MISSING_RIGHT_ERROR),
        }
    }

    pub fn graph_mut(&mut self, side: GraphSide) -> Result<&mut ArrayGraph> {
        match side {
            GraphSide::Left => Ok(&mut self.l),
            GraphSide::Right => self.r.as_mut().context(MISSING_RIGHT_ERROR),
        }
    }

    pub fn graph_u32(&self, side: u32) -> Result<&ArrayGraph> {
        GraphSide::from_u32(side)
            .and_then(|s| self.graph(s))
            .context("graph_u32: Invalid GraphSide value")
    }

    pub fn graph_u32_mut(&mut self, side: u32) -> Result<&mut ArrayGraph> {
        GraphSide::from_u32(side)
            .and_then(|s| self.graph_mut(s))
            .context("graph_u32: Invalid GraphSide value")
    }
}
