// Copyright (c) Meta Platforms, Inc. and affiliates.

use anyhow::Result;
use bytemuck::Pod;
use bytemuck::Zeroable;
use glam::Vec2;
use rand::Rng;
use unigraph_core::ArrayGraph;
use unigraph_core::NodeIDX;
use unigraph_core::remap_utils::RemapContext;

use crate::barnes_hut::BHGraphNode;
use crate::barnes_hut::QuadTree;
use crate::global_state;
use crate::graph_state::NodeAttributesFlags;

/// Stripped version of the ArrayGraph that is used for running simulations.
/// It contains all the needed node/edges attributes and logic for how to
/// convert that into WebGPU data.
/// It also operates on a stripped down version of the ArrayGraph that excludes
/// anything unreachable/excluded.
///
/// One of the issues when working with large graph where most nodes are unreachable
/// (e.g. 1M nodes graph, but only 10k are reachable) is that if we sync the entire
/// graph to the GPU and only render the reachable nodes, we end up sending
/// hundreds of MBs of data per second which can mess up the GPU pipeline.
pub(crate) struct SimulationGraph {
    /// Local node data lives on the CPU and we only access it here
    nodes_local: Vec<SimulationNodeLocal>,
    /// GPU data gets serialized and sent to a GPU buffer, ideally we want to
    /// keep it as small as possible.
    nodes_gpu: Vec<SimulationNodeGPU>,

    edges: Vec<SimulationEdge>,

    remap_ctx: RemapContext,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SimulationNodeGPU {
    position: Vec2,
    adjusted_size: f32,
    flags: NodeAttributesFlags,
}

#[derive(Default)]
struct SimulationNodeLocal {
    velocity: Vec2,
    force: Vec2,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SimulationEdge {
    from: NodeIDX,
    to: NodeIDX,
}

impl SimulationGraph {
    pub fn new(array_graph: &ArrayGraph, selected_metric: &Option<String>) -> Result<Self> {
        let mut nodes_local = vec![];
        let mut nodes_gpu = vec![];
        let mut edges = vec![];
        let mut original_positions = vec![];
        let mut mappings = vec![];

        for node_idx in array_graph.node_idx_iter() {
            if array_graph.node_flags[node_idx].is_node_unreachable() {
                mappings.push(None);
            } else {
                mappings.push(Some(nodes_gpu.len().into()));
                original_positions.push(node_idx);
                nodes_local.push(SimulationNodeLocal::default());
                nodes_gpu.push(SimulationNodeGPU::random());
            }
        }

        for (from_idx, edge, _metadata) in array_graph.edges_forward.iter_edges() {
            if edge.is_excluded() {
                continue;
            }
            if array_graph.node_flags[from_idx].is_node_unreachable() {
                continue;
            }
            if array_graph.node_flags[edge.points_to].is_node_unreachable() {
                continue;
            }

            let from = mappings[from_idx];
            let to = mappings[edge.points_to];

            if let (Some(from), Some(to)) = (from, to) {
                edges.push(SimulationEdge { from, to });
            }
        }

        let mut graph = SimulationGraph {
            nodes_local,
            nodes_gpu,
            edges,
            remap_ctx: RemapContext {
                original_positions,
                mappings,
            },
        };

        if let Some(selected_metrics) = selected_metric
            .as_ref()
            .and_then(|m| array_graph.metrics.get(m))
        {
            graph.recalculate_adjusted_sizes(selected_metrics);
        }

        Ok(graph)
    }

    pub fn nodes_len(&self) -> usize {
        self.nodes_local.len()
    }

    pub fn edges_len(&self) -> usize {
        self.edges.len()
    }

    pub fn node_idx_iter(&self) -> std::iter::Map<std::ops::Range<usize>, fn(usize) -> NodeIDX> {
        (0..self.nodes_local.len()).map(NodeIDX::from)
    }

    pub fn nodes_bytes(&'_ self) -> &'_ [u8] {
        bytemuck::cast_slice(&self.nodes_gpu)
    }

    pub fn edges_bytes(&'_ self) -> &'_ [u8] {
        bytemuck::cast_slice(&self.edges)
    }

    // Process the next iteration of the simulation, which involves calculating all
    // forces and adjusting node velocities and positions accordingly.
    pub fn compute_next_frame(&mut self, compute_forces: bool) -> Result<()> {
        const TERMINAL_VELOCITY: f32 = 0.01;
        let params = global_state().simulation_params.get();

        if compute_forces {
            self.recompute_forces()?;
        }

        // update node positions based on forces
        for node_idx in self.node_idx_iter() {
            let local = &mut self.nodes_local[node_idx];
            let gpu = &mut self.nodes_gpu[node_idx];

            let force = local.force;

            local.velocity += force * params.max_velocity_multiplier;
            local.velocity = local.velocity.clamp_length_max(TERMINAL_VELOCITY);

            // Add some friction. This will slow down the nodes over time.
            const SLOW_DOWN: f32 = 0.9;
            local.velocity *= SLOW_DOWN;

            // Update the node's position based on its velocity
            gpu.position += local.velocity;

            gpu.position = gpu.position.clamp(Vec2::splat(-0.95), Vec2::splat(0.95));
        }
        Ok(())
    }

    fn recompute_forces(&mut self) -> Result<()> {
        // !!! Reset forces to zero before recomputing
        self.nodes_local.iter_mut().for_each(|node| {
            node.force = Vec2::ZERO;
        });

        self.compute_gravity_forces()?;
        self.compute_edge_forces()?;
        self.forces_pull_towards_center()?;

        Ok(())
    }

    fn compute_gravity_forces(&mut self) -> Result<()> {
        let mut quad_tree = QuadTree::new(300);
        for idx in self.node_idx_iter() {
            let gpu = &self.nodes_gpu[idx];
            quad_tree.add_body(BHGraphNode {
                position: gpu.position,
                idx: idx.0 as usize,
                mass: gpu.adjusted_size,
            });
        }
        let gravity_forces = quad_tree.compute_forces(self.nodes_local.len());
        let params = global_state().simulation_params.get();
        for (idx, force) in gravity_forces.iter().enumerate() {
            self.nodes_local[idx].force -= *force * params.gravity_force_multiplier;
        }

        Ok(())
    }

    pub fn compute_edge_forces(&mut self) -> Result<()> {
        let params = global_state().simulation_params.get();

        for edge in &self.edges {
            let SimulationEdge { from, to } = *edge;

            let dx = self.nodes_gpu[to].position.x - self.nodes_gpu[from].position.x;
            let dy = self.nodes_gpu[to].position.y - self.nodes_gpu[from].position.y;

            let distance_squared = dx * dx + dy * dy + 0.0001; // Avoid division by zero

            let distance = distance_squared.sqrt();
            let force_magnitude = 0.0009 * distance.ln_1p(); // Use natural log (ln(1 + x)) for linlog

            // Calculate components of the force
            let fx = (dx / distance) * force_magnitude;
            let fy = (dy / distance) * force_magnitude;

            let force = Vec2 { x: fx, y: fy };

            self.nodes_local[from].force += force * params.edge_force_multiplier;
            self.nodes_local[to].force -= force * params.edge_force_multiplier;
        }
        Ok(())
    }

    fn forces_pull_towards_center(&mut self) -> Result<()> {
        const CENTER_PULL_STRENGTH: f32 = 0.0007;
        // Calculate forces pulling nodes towards the center (0, 0)

        for node_idx in self.node_idx_iter() {
            let gpu = &self.nodes_gpu[node_idx];
            let dx = -gpu.position.x;
            let dy = -gpu.position.y;

            let distance_squared = dx * dx + dy * dy + 0.001; // Avoid division by zero
            let distance = distance_squared.sqrt();
            let force_magnitude = CENTER_PULL_STRENGTH * distance * distance;

            // Calculate components of the force
            let fx = dx / distance * force_magnitude;
            let fy = dy / distance * force_magnitude;

            self.nodes_local[node_idx].force += Vec2 { x: fx, y: fy };
        }
        Ok(())
    }

    pub fn recalculate_adjusted_sizes(&mut self, selected_metrics: &[f32]) {
        let nodes_len = self.nodes_len();
        if nodes_len == 0 {
            return;
        }
        let mut all_sizes = Vec::with_capacity(nodes_len);

        for sim_node_idx in self.node_idx_iter() {
            let original_node_idx = self.remap_ctx.original_positions[sim_node_idx];
            all_sizes.push(selected_metrics[original_node_idx]);
        }

        if all_sizes.is_empty() {
            return;
        }

        let mut sorted = all_sizes.clone();
        sort_vec_f32(&mut sorted);

        let min_size = sorted[0];
        let max_size = sorted[sorted.len() - 1];

        for sim_node_idx in self.node_idx_iter() {
            let size = all_sizes[sim_node_idx];
            let adjusted_size = if size == 0.0 {
                1.0
            } else {
                // Normalize the size to be between 0 and 1
                let normalized_size = (size - min_size) / (max_size - min_size);
                // Scale it to be between 1 and 100
                normalized_size * 99.0 + 1.0
            };
            self.nodes_gpu[sim_node_idx].adjusted_size = adjusted_size;
        }
    }
}

impl SimulationNodeGPU {
    pub fn random() -> Self {
        let mut rng = rand::rng();
        Self {
            position: Vec2 {
                x: rng.random_range(-1.0..1.0),
                y: rng.random_range(-1.0..1.0),
            }
            .clamp_length_max(0.01),
            adjusted_size: 1.0,
            flags: NodeAttributesFlags::empty(),
        }
    }
}

fn sort_vec_f32(vec: &mut [f32]) {
    vec.sort_by(|a, b| a.partial_cmp(b).unwrap());
}
