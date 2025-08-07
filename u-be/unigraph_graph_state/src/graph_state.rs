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
use unigraph_core::types::NodeIDX;

use crate::simulation_graph::SimulationGraph;

#[non_exhaustive]
pub struct GraphState {
    pub array_graph: ArrayGraph,
    pub selected_metric: Option<String>,
    pub simulation_graph: SimulationGraph,
}

pub struct SharedGraphState {
    inner: Arc<RwLock<GraphState>>,
}

impl SharedGraphState {
    pub fn new(array_graph: ArrayGraph) -> Result<Self> {
        Ok(Self {
            inner: Arc::new(RwLock::new(GraphState::new(array_graph)?)),
        })
    }

    pub fn get(&self) -> RwLockReadGuard<GraphState> {
        self.inner.read().unwrap()
    }

    pub fn get_mut(&self) -> RwLockWriteGuard<GraphState> {
        self.inner.write().unwrap()
    }

    pub fn replace_graph(&self, new_graph: ArrayGraph) -> Result<()> {
        let new_state = GraphState::new(new_graph)?;
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
    pub fn new(array_graph: ArrayGraph) -> Result<Self> {
        // by default we'll grab whatever metric is first in the list
        let selected_metric = array_graph.metrics.keys().next().cloned();
        let simulation_graph = SimulationGraph::new(&array_graph, &selected_metric, None)?;

        let result = Self {
            array_graph,
            selected_metric,
            simulation_graph,
        };
        Ok(result)
    }

    pub fn get_selected_metrics_vec(&self) -> Option<&Vec<f32>> {
        if let Some(selected_metric) = &self.selected_metric {
            self.array_graph.metrics.get(selected_metric)
        } else {
            None
        }
    }

    pub fn sync_node_attributes(&mut self) -> Result<()> {
        self.simulation_graph = SimulationGraph::new(
            &self.array_graph,
            &self.selected_metric,
            Some(&self.simulation_graph),
        )?;
        Ok(())
    }
}
