// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::sync::Arc;
use std::sync::RwLock;

use anyhow::Context;
use anyhow::Result;
use bytemuck::Pod;
use bytemuck::Zeroable;
use glam::Vec2;
use serde::Deserialize;
use serde::Serialize;
use ts_rs::TS;

#[derive(TS)]
#[ts(export)]
#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]

pub struct TsVec2 {
    pub x: f32,
    pub y: f32,
}

impl From<glam::Vec2> for TsVec2 {
    fn from(vec: glam::Vec2) -> Self {
        Self { x: vec.x, y: vec.y }
    }
}
impl From<TsVec2> for glam::Vec2 {
    fn from(val: TsVec2) -> Self {
        glam::Vec2::new(val.x, val.y)
    }
}

pub(crate) mod ts_vec2_serde {
    use serde::Deserialize;

    use super::TsVec2;

    pub fn serialize<S>(vec: &glam::Vec2, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_newtype_struct("TsVec2", &TsVec2::from(*vec))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<glam::Vec2, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let vec = TsVec2::deserialize(deserializer)?;
        Ok(glam::Vec2::new(vec.x, vec.y))
    }
}

#[derive(TS)]
#[ts(export)]
#[derive(Deserialize, Serialize, Debug)]
pub struct SimulationColors {
    pub background: [f32; 3],
    pub node_main: [f32; 3],
    pub node_selected: [f32; 3],
}

#[derive(TS)]
#[ts(export)]
#[derive(Deserialize, Serialize, Debug)]
pub struct SimulationParams {
    /// This flag is used to enable or disable the simulation, but not the rendering.
    /// The simulation part is the computation of the positions, forces, etc.
    /// If the simulation is not active we'd still run the rendering part
    /// to show other things (like box selection, etc) but the thing will just not
    /// be moving.
    pub active: bool,
    pub render_edges: bool,
    /// scale of the sizes of the nodes. from 1 to 100;
    pub node_size_scale: usize,
    /// value increase or decrease the anti-gravity force
    /// that pushes nodes away from each other
    pub gravity_force_multiplier: f32,
    pub gravity_force_scale: ScaleType,

    /// value increase or decrease the force
    /// that edges pull nodes together
    pub edge_force_multiplier: f32,
    pub edge_force_scale: ScaleType,

    /// How much the nodes are pulled towards the center of the space.
    pub center_pull_force_multiplier: f32,

    /// value increase or decrease the maximum velocity that the nodes
    /// can reach.
    pub max_velocity_multiplier: f32,

    pub selection: Selection,
    pub colors: SimulationColors,

    /// Calculating gravity forces is the most expensive part of the simulation.
    /// Ideally we would want to compute forces every frame, but on large graphs
    /// it becomes extremely expensive. To optimize this we can skip some frames
    /// and only update the forces every n frames. In the skipped frames we would
    /// compute the positions based on the previous forces/velocities.
    /// This make the visualization much more jittery but it's better then
    /// having 0.5 fps.
    pub compute_forces_every_n_frames: u32,
}

#[derive(TS)]
#[ts(export)]
#[derive(Deserialize, Serialize, Debug, Clone, Copy)]
pub enum ScaleType {
    Linear,
    Logarithmic,
    Quadratic,
}

impl SimulationParams {
    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json)
            .with_context(|| format!("Failed to parse simulation params JSON: {json}"))
    }

    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(self).context("Failed to serialize simulation params")
    }
}

#[derive(Clone)]
pub struct SharedSimulationParams {
    inner: Arc<RwLock<Arc<SimulationParams>>>,
}

impl SharedSimulationParams {
    pub fn new(simulation_params: SimulationParams) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Arc::new(simulation_params))),
        }
    }

    pub fn set(&self, params: SimulationParams) {
        let mut inner = self.inner.write().unwrap();
        *inner = Arc::new(params);
    }

    pub fn get(&self) -> Arc<SimulationParams> {
        self.inner.read().unwrap().clone()
    }
}

impl Default for SimulationParams {
    fn default() -> Self {
        Self {
            active: true,
            render_edges: true,
            node_size_scale: 15,
            selection: Selection::default(),
            colors: SimulationColors {
                background: [0.033, 0.030, 0.027],
                node_main: [0.4654, 0.0091, 0.0480],
                node_selected: [0.0480, 0.0091, 0.4654],
            },
            gravity_force_multiplier: 200.0,
            gravity_force_scale: ScaleType::Linear,
            edge_force_multiplier: 1.0,
            edge_force_scale: ScaleType::Linear,
            center_pull_force_multiplier: 50.0,
            max_velocity_multiplier: 1.0,
            compute_forces_every_n_frames: 1,
        }
    }
}

#[derive(TS)]
#[ts(export)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[repr(C)]
pub enum SelectionType {
    #[default]
    None = 0,
    Box = 1,
    Line = 2,
}

// Current selection from the mouse.
// Can be a box (selectiong nodes) a line (selectiong edges) or none.
#[derive(TS)]
#[ts(export)]
#[derive(Copy, Clone, Debug, Pod, Zeroable, Serialize, Deserialize, Default)]
#[repr(C)]
pub struct Selection {
    #[ts(as = "TsVec2")]
    #[serde(with = "ts_vec2_serde")]
    pub selection_from_point: Vec2,
    #[ts(as = "TsVec2")]
    #[serde(with = "ts_vec2_serde")]
    pub selection_to_point: Vec2,
    pub selection_type: SelectionType,
}

unsafe impl bytemuck::Zeroable for SelectionType {}
unsafe impl bytemuck::Pod for SelectionType {}

impl Selection {
    pub fn within_box_bounds(
        &self,
        point: Vec2,
        // we adjust node positions by the aspect ratio in wgpu shaders, so
        // we'll need to do the same here to make sure the positions of
        // nodes match the positions in the selection box.
        // Otherwise the selection will be off on non-square surfaces.
        aspect_ratio: f32,
    ) -> bool {
        let point_x = point.x / aspect_ratio;
        let min_x = self.selection_from_point.x.min(self.selection_to_point.x);
        let max_x = self.selection_from_point.x.max(self.selection_to_point.x);
        let min_y = self.selection_from_point.y.min(self.selection_to_point.y);
        let max_y = self.selection_from_point.y.max(self.selection_to_point.y);
        point_x >= min_x && point_x <= max_x && point.y >= min_y && point.y <= max_y
    }
}
