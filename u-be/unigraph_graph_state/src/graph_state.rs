// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::sync::Arc;
use std::sync::RwLock;
use std::sync::RwLockReadGuard;
use std::sync::RwLockWriteGuard;

use anyhow::Context;
use anyhow::Result;
use bytemuck::Pod;
use bytemuck::Zeroable;
use glam::Vec2;
use unigraph_core::ArrayGraph;
use unigraph_core::TwinGraph;
use unigraph_core::types::NodeIDX;

use crate::simulation_graph::SimulationGraph;

/// The top-level state: either a single graph or a twin (comparison) graph.
pub enum GraphMode {
    Single(ArrayGraph),
    Twin(TwinGraph),
}

impl GraphMode {
    /// The "right" (or only) graph — used for simulation, metrics, etc.
    pub fn r(&self) -> &ArrayGraph {
        match self {
            GraphMode::Single(ag) => ag,
            GraphMode::Twin(tg) => &tg.r,
        }
    }

    pub fn r_mut(&mut self) -> &mut ArrayGraph {
        match self {
            GraphMode::Single(ag) => ag,
            GraphMode::Twin(tg) => tg.graph_mut(unigraph_core::GraphSide::Right),
        }
    }

    /// Get the array graph for a given side.
    /// In Single mode, returns the single graph regardless of side.
    pub fn graph(&self, side: u32) -> Result<&ArrayGraph> {
        match self {
            GraphMode::Single(ag) => Ok(ag),
            GraphMode::Twin(tg) => Ok(tg.graph(unigraph_core::GraphSide::from_u32(side)?)),
        }
    }

    pub fn graph_mut(&mut self, side: u32) -> Result<&mut ArrayGraph> {
        match self {
            GraphMode::Single(ag) => Ok(ag),
            GraphMode::Twin(tg) => Ok(tg.graph_mut(unigraph_core::GraphSide::from_u32(side)?)),
        }
    }

    /// Translate a UI-facing node index to a local ArrayGraph index.
    /// In Single mode, returns the index as-is.
    /// In Twin mode, returns None if the node doesn't exist on that side.
    pub fn to_local(&self, side: u32, idx: NodeIDX) -> Result<Option<NodeIDX>> {
        match self {
            GraphMode::Single(_) => Ok(Some(idx)),
            GraphMode::Twin(tg) => tg.to_local_u32(side, idx),
        }
    }

    /// Number of nodes in the UI-facing namespace.
    pub fn node_count(&self) -> usize {
        match self {
            GraphMode::Single(ag) => ag.nodes_len(),
            GraphMode::Twin(tg) => tg.merged_len(),
        }
    }

    /// Resolve a UI-facing node index to a name.
    pub fn idx_to_name(&self, idx: NodeIDX) -> &str {
        match self {
            GraphMode::Single(ag) => ag.idx_to_name(idx),
            GraphMode::Twin(tg) => tg.merged_idx_to_name(idx),
        }
    }

    /// Translate a local index back to UI-facing index.
    pub fn to_ui(&self, side: u32, local_idx: NodeIDX) -> Result<NodeIDX> {
        match self {
            GraphMode::Single(_) => Ok(local_idx),
            GraphMode::Twin(tg) => {
                Ok(tg.to_merged(unigraph_core::GraphSide::from_u32(side)?, local_idx))
            }
        }
    }
}

#[non_exhaustive]
pub struct GraphState {
    pub mode: GraphMode,
    pub selected_metric: Option<String>,
    pub simulation_graph: SimulationGraph,
}

pub struct SharedGraphState {
    inner: Arc<RwLock<GraphState>>,
}

impl SharedGraphState {
    pub fn new(mode: GraphMode) -> Result<Self> {
        Ok(Self {
            inner: Arc::new(RwLock::new(GraphState::new(mode)?)),
        })
    }

    pub fn get(&self) -> RwLockReadGuard<'_, GraphState> {
        self.inner.read().unwrap()
    }

    pub fn get_mut(&self) -> RwLockWriteGuard<'_, GraphState> {
        self.inner.write().unwrap()
    }

    pub fn replace_graph(&self, mode: GraphMode) -> Result<()> {
        let new_state = GraphState::new(mode)?;
        let mut inner = self.get_mut();
        *inner = new_state;
        Ok(())
    }

    pub fn compute_next_frame(&self, update_forces: bool) -> Result<()> {
        let mut graph_state = self.get_mut();
        graph_state
            .simulation_graph
            .compute_next_frame(update_forces)
            .context("Failed to compute next frame")
    }
}

// This is mutable state that would normally change during the simulation.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct NodeAttributes {
    pub position: Vec2,
    pub velocity: Vec2,
    // value between 0 and 100 that is proportional to
    // the node size but fits between 0 and 100.
    pub adjusted_size: f32,
    pub flags: NodeAttributesFlags,
    // padding to make the struct size a multiple of 16 bytes
    pub _padding: (),
}

bitflags::bitflags! {
    #[repr(transparent)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
    pub struct NodeAttributesFlags: u32 {
        const UNREACHABLE = 256;    // 0b0001_0000_0000;
        const SELECTED    = 512;    // 0b0010_0000_0000;
        const FOCUSED     = 1024;   // 0b0100_0000_0000;
    }
}

unsafe impl Zeroable for NodeAttributesFlags {}
unsafe impl Pod for NodeAttributesFlags {}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct EdgeAttributes {
    pub from: NodeIDX,
    pub to: NodeIDX,
}

impl GraphState {
    pub fn new(mode: GraphMode) -> Result<Self> {
        let selected_metric = mode.r().data.node_metadata.metrics.keys().next().cloned();
        let simulation_graph = SimulationGraph::new(mode.r(), &selected_metric, None)?;

        Ok(Self {
            mode,
            selected_metric,
            simulation_graph,
        })
    }

    pub fn get_selected_metrics_vec(&self) -> Option<&Vec<f32>> {
        if let Some(selected_metric) = &self.selected_metric {
            self.mode
                .r()
                .data
                .node_metadata
                .metrics
                .get(selected_metric)
        } else {
            None
        }
    }

    pub fn sync_node_attributes(&mut self) -> Result<()> {
        self.simulation_graph = SimulationGraph::new(
            self.mode.r(),
            &self.selected_metric,
            Some(&self.simulation_graph),
        )?;
        Ok(())
    }
}
