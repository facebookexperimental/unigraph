// Copyright (c) Meta Platforms, Inc. and affiliates.

mod barnes_hut;
pub mod graph_state;
pub mod simulation_graph;
pub mod types;

use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::RwLockWriteGuard;

use unigraph_core::ArrayGraph;

use crate::graph_state::GraphState;
use crate::graph_state::SharedGraphState;
use crate::types::SharedSimulationParams;
use crate::types::SimulationParams;

static GLOBAL_GRAPH_STATE: OnceLock<GlobalGraphState> = OnceLock::new();

pub struct GlobalGraphState {
    pub simulation_params: SharedSimulationParams,
    pub graph_state: SharedGraphState,
}

impl GlobalGraphState {
    pub fn get() -> &'static GlobalGraphState {
        GLOBAL_GRAPH_STATE
            .get()
            .expect("global state not initialized")
    }

    pub fn simulation_params() -> Arc<SimulationParams> {
        Self::get().simulation_params.get()
    }

    pub fn graph_state() -> &'static SharedGraphState {
        &Self::get().graph_state
    }

    pub fn graph_state_mut<'a>() -> RwLockWriteGuard<'a, GraphState> {
        Self::get().graph_state.get_mut()
    }

    pub fn init() {
        GLOBAL_GRAPH_STATE
            .set(GlobalGraphState {
                simulation_params: SharedSimulationParams::new(SimulationParams::default()),
                graph_state: SharedGraphState::new(ArrayGraph::empty().unwrap()).unwrap(),
            })
            .map_err(|_| "Global state already initialized")
            .unwrap();
    }
}

pub fn global_graph_state() -> &'static GlobalGraphState {
    GLOBAL_GRAPH_STATE
        .get()
        .expect("global state not initialized")
}
