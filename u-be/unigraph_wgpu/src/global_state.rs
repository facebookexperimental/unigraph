// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::RwLock;
use std::sync::RwLockWriteGuard;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU32;

use unigraph_core::ArrayGraph;

use crate::UserEvent;
use crate::graph_state::GraphState;
use crate::graph_state::SharedGraphState;
use crate::ts_types::SharedSimulationParams;
use crate::ts_types::SimulationParams;

static GLOBAL_STATE: OnceLock<GlobalState> = OnceLock::new();

pub struct GlobalState {
    pub simulation_params: SharedSimulationParams,
    pub graph_state: SharedGraphState,
    pub surface_size: Arc<AtomicPhysicalSize>,
    pub event_loop_proxy: Arc<RwLock<Option<winit::event_loop::EventLoopProxy<UserEvent>>>>,
    pub event_loop_active: AtomicBool,
}

impl GlobalState {
    pub fn get() -> &'static GlobalState {
        GLOBAL_STATE.get().expect("global state not initialized")
    }

    pub fn simulation_params() -> Arc<SimulationParams> {
        Self::get().simulation_params.get()
    }

    pub fn surface_size() -> Arc<AtomicPhysicalSize> {
        Self::get().surface_size.clone()
    }

    pub fn graph_state() -> &'static SharedGraphState {
        &Self::get().graph_state
    }

    pub fn graph_state_mut<'a>() -> RwLockWriteGuard<'a, GraphState> {
        Self::get().graph_state.get_mut()
    }

    pub fn set_event_loop_proxy(event_loop_proxy: winit::event_loop::EventLoopProxy<UserEvent>) {
        Self::get()
            .event_loop_proxy
            .write()
            .unwrap()
            .replace(event_loop_proxy);
    }

    pub fn init() {
        GLOBAL_STATE
            .set(GlobalState {
                simulation_params: SharedSimulationParams::new(SimulationParams::default()),
                graph_state: SharedGraphState::new(ArrayGraph::empty().unwrap()),
                surface_size: Arc::new(AtomicPhysicalSize::new(0, 0)),
                event_loop_proxy: Default::default(),
                event_loop_active: AtomicBool::new(false),
            })
            .map_err(|_| "Global state already initialized")
            .unwrap();
    }
}

pub fn global_state() -> &'static GlobalState {
    GLOBAL_STATE.get().expect("global state not initialized")
}

pub struct AtomicPhysicalSize {
    pub width: AtomicU32,
    pub height: AtomicU32,
}

impl AtomicPhysicalSize {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width: AtomicU32::new(width),
            height: AtomicU32::new(height),
        }
    }

    pub fn set(&self, width: u32, height: u32) -> &Self {
        self.width.store(width, std::sync::atomic::Ordering::SeqCst);
        self.height
            .store(height, std::sync::atomic::Ordering::SeqCst);
        self
    }

    pub fn aspect_ratio(&self) -> f32 {
        let width = self.width.load(std::sync::atomic::Ordering::SeqCst);
        let height = self.height.load(std::sync::atomic::Ordering::SeqCst);
        if height == 0 {
            return 1.0;
        }
        width as f32 / height as f32
    }
}
