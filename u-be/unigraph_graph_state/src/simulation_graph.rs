// Copyright (c) Meta Platforms, Inc. and affiliates.

use anyhow::Context;
use anyhow::Result;
use bytemuck::Pod;
use bytemuck::Zeroable;
use glam::Vec2;
use unigraph_core::ArrayGraph;
use unigraph_core::NodeIDX;
use unigraph_core::remap_utils::RemapContext;

use crate::barnes_hut::BHGraphNode;
use crate::barnes_hut::QuadTree;
use crate::global_graph_state;
use crate::graph_state::NodeAttributesFlags;
use crate::lfsr::Lfsr32;
use crate::types::Selection;
use crate::types::SelectionType;

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
pub struct SimulationGraph {
    /// Local node data lives on the CPU and we only access it here
    nodes_local: Vec<SimulationNodeLocal>,
    /// GPU data gets serialized and sent to a GPU buffer, ideally we want to
    /// keep it as small as possible.
    nodes_gpu: Vec<SimulationNodeGPU>,

    edges: Vec<SimulationEdge>,

    remap_ctx: RemapContext,

    /// value that specifies the boundaries of the MAX position of the nodes.
    /// to make sure they fit given the current aspect ratio.
    ///
    /// values are in the range of -1.0 to 1.0.
    ///
    /// if the boundaries (-0.5, -0.5) and (0.5, 0.5) the positions of the nodes
    /// will be clamped to that range.
    boundaries: (Vec2, Vec2),
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SimulationNodeGPU {
    position: Vec2,
    adjusted_size: f32,
    flags: NodeAttributesFlags,
}

#[derive(Default, Clone, Copy)]
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
    pub fn new(
        array_graph: &ArrayGraph,
        selected_metric: &Option<String>,
        // optionally pass the previous graph if we want to preserve
        // existing node postitions/velocities/etc. Otherwise the simulation
        // will reset from the start positions every time we modify the graph/traversal config.
        previous_graph: Option<&SimulationGraph>,
    ) -> Result<Self> {
        let mut nodes_local = vec![];
        let mut nodes_gpu = vec![];
        let mut edges = vec![];
        let mut original_positions = vec![];
        let mut mappings = vec![];
        let mut lfsr = Lfsr32::new(84848484);

        for node_idx in array_graph.node_idx_iter() {
            if array_graph.node_flags[node_idx].is_node_unreachable() {
                mappings.push(None);
            } else {
                let (mut local, mut gpu) = (
                    SimulationNodeLocal::default(),
                    SimulationNodeGPU::random(&mut lfsr),
                );

                if let Some(prev) = &previous_graph {
                    // If we have a previous graph, we can reuse the existing node data
                    if let Some(prev_idx) = prev.remap_ctx.mappings[node_idx] {
                        local = prev.nodes_local[prev_idx];
                        gpu = prev.nodes_gpu[prev_idx];
                    }
                }

                mappings.push(Some(nodes_gpu.len().into()));
                original_positions.push(Some(node_idx));
                nodes_local.push(local);
                nodes_gpu.push(gpu);
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
            boundaries: (Vec2::splat(-0.95), Vec2::splat(0.95)),
        };

        if let Some(selected_metrics) = selected_metric
            .as_ref()
            .and_then(|m| array_graph.metrics.get(m))
        {
            graph.recalculate_adjusted_sizes(selected_metrics)?;
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
        let params = global_graph_state().simulation_params.get();

        if compute_forces {
            self.recompute_forces()?;
        }

        // update node positions based on forces
        for node_idx in self.node_idx_iter() {
            let local = &mut self.nodes_local[node_idx];
            let gpu = &mut self.nodes_gpu[node_idx];

            let force = local.force;
            let strength = (force.length() * 10.0).ln_1p() / 10.0;
            let direction = force.normalize();

            let force = strength * direction * params.total_force_multiplier;
            local.velocity += force;
            local.velocity = local
                .velocity
                .clamp_length_max(params.max_velocity_multiplier);
            if !local.velocity.is_finite() || local.velocity.is_nan() {
                local.velocity = Vec2::ZERO; // Reset to zero if the velocity is not valid
            }
            if !local.force.is_finite() || local.force.is_nan() {
                local.force = Vec2::ZERO; // Reset to zero if the force is not valid
            }

            // Add some friction. This will slow down the nodes over time.
            local.velocity *= params.slowdown;

            // Update the node's position based on its velocity
            gpu.position += local.velocity;

            gpu.position = gpu.position.clamp(self.boundaries.0, self.boundaries.1);
        }

        Ok(())
    }

    fn recompute_forces(&mut self) -> Result<()> {
        // !!! Reset forces to zero before recomputing
        self.nodes_local.iter_mut().for_each(|node| {
            node.force = Vec2::ZERO;
        });
        let params = global_graph_state().simulation_params.get();

        if !params.disable_gravity {
            self.compute_gravity_forces()?;
        }

        if !params.disable_edge_forces {
            self.compute_edge_forces()?;
        }

        if !params.disable_center_pull {
            self.forces_pull_towards_center()?;
        }

        Ok(())
    }

    fn compute_gravity_forces(&mut self) -> Result<()> {
        let params = global_graph_state().simulation_params.get();
        let mut quad_tree = QuadTree::new(300);
        for idx in self.node_idx_iter() {
            let gpu = &self.nodes_gpu[idx];
            quad_tree.add_body(BHGraphNode {
                position: gpu.position,
                idx: idx.0 as usize,
                mass: gpu.adjusted_size,
            });
        }
        let gravity_forces = quad_tree.compute_forces(self.nodes_local.len(), &params);
        for (idx, force) in gravity_forces.iter().enumerate() {
            self.nodes_local[idx].force -= force;
        }

        Ok(())
    }

    pub fn compute_edge_forces(&mut self) -> Result<()> {
        let params = global_graph_state().simulation_params.get();
        const EPSILON: f32 = 0.0001;

        for edge in &self.edges {
            let SimulationEdge { from, to } = *edge;

            let diff = self.nodes_gpu[to].position - self.nodes_gpu[from].position;

            let distance = diff.length() + EPSILON; // Avoid division by zero
            let force_magnitude =
                params.edge_force_a * 0.0009 * (distance * params.edge_force_b + EPSILON).ln_1p(); // Use natural log (ln(1 + x)) for linlog

            let force = (diff / distance) * force_magnitude;

            self.nodes_local[from].force += force;
            self.nodes_local[to].force -= force;
        }
        Ok(())
    }

    fn forces_pull_towards_center(&mut self) -> Result<()> {
        const CENTER_PULL_STRENGTH_COEFFICIENT: f32 = 0.0007;

        let params = global_graph_state().simulation_params.get();

        let multiplier = params.center_pull_force_multiplier * CENTER_PULL_STRENGTH_COEFFICIENT;

        // Calculate forces pulling nodes towards the center (0, 0)

        for node_idx in self.node_idx_iter() {
            let gpu = &self.nodes_gpu[node_idx];
            let dx = -gpu.position.x;
            let dy = -gpu.position.y;

            let distance_squared = dx * dx + dy * dy + 0.001; // Avoid division by zero
            let distance = distance_squared.sqrt();
            let force_magnitude = multiplier * distance * distance;

            // Calculate components of the force
            let fx = dx / distance * force_magnitude;
            let fy = dy / distance * force_magnitude;

            self.nodes_local[node_idx].force += Vec2 { x: fx, y: fy };
        }
        Ok(())
    }

    pub fn recalculate_adjusted_sizes(&mut self, selected_metrics: &[f32]) -> Result<()> {
        let nodes_len = self.nodes_len();
        if nodes_len == 0 {
            return Ok(());
        }
        let mut all_sizes = Vec::with_capacity(nodes_len);

        for sim_node_idx in self.node_idx_iter() {
            let original_node_idx = self
                .remap_ctx
                .get_original_position_assert(sim_node_idx)
                .context("calc sizes")?;
            all_sizes.push(selected_metrics[original_node_idx]);
        }

        if all_sizes.is_empty() {
            return Ok(());
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
        Ok(())
    }

    /// Returns ORIGINAL node indexes from the ArrayGraph
    pub fn mark_nodes_as_selected(
        &mut self,
        selection: &Selection,
        aspect_ratio: f32,
    ) -> Result<Vec<NodeIDX>> {
        match selection.selection_type {
            SelectionType::None => {
                for sim_node_idx in self.node_idx_iter() {
                    self.nodes_gpu[sim_node_idx]
                        .flags
                        .remove(NodeAttributesFlags::SELECTED);
                }
                Ok(vec![])
            }
            SelectionType::Box => {
                let mut selected_nodes = vec![];
                for sim_node_idx in self.node_idx_iter() {
                    if selection
                        .within_box_bounds(self.nodes_gpu[sim_node_idx].position, aspect_ratio)
                    {
                        self.nodes_gpu[sim_node_idx]
                            .flags
                            .insert(NodeAttributesFlags::SELECTED);
                        selected_nodes
                            .push(self.remap_ctx.get_original_position_assert(sim_node_idx)?);
                    } else {
                        self.nodes_gpu[sim_node_idx]
                            .flags
                            .remove(NodeAttributesFlags::SELECTED);
                    }
                }
                Ok(selected_nodes)
            }
            SelectionType::Line => {
                anyhow::bail!("Line selection not implemented yet");
            }
        }
    }

    pub fn set_boundaries(&mut self, aspect_ratio: f32) {
        const PADDING: f32 = 0.02; // Padding to avoid nodes being too close to the edges

        let value = if aspect_ratio < 1.0 {
            Vec2::new(aspect_ratio, 1.0)
        } else {
            Vec2::new(1.0, 1.0 / aspect_ratio)
        } - Vec2::splat(PADDING);

        self.boundaries = (-value, value);
    }
}

impl SimulationNodeGPU {
    pub fn random(lfsr: &mut Lfsr32) -> Self {
        Self {
            position: Vec2 {
                x: lfsr.next(),
                y: lfsr.next(),
            }
            .clamp_length_max(0.9),
            adjusted_size: 1.0,
            flags: NodeAttributesFlags::empty(),
        }
    }
}

fn sort_vec_f32(vec: &mut [f32]) {
    vec.sort_by(|a, b| a.partial_cmp(b).unwrap());
}

#[cfg(test)]
mod tests {
    use k9::assert_equal;

    use super::*;

    #[test]
    fn test_sort_vec_f32() -> Result<()> {
        assert_equal!(Vec2::new(0.5, -0.5).signum(), Vec2::new(1.0, -1.0));

        assert_equal!(
            Vec2::new(0.1, 0.1).copysign(Vec2::new(0.5, -0.5)),
            Vec2::new(0.1, -0.1)
        );

        let diff = Vec2::new(1.0, 1.0) - Vec2::new(-1.0, -1.0);
        assert_equal!(diff, Vec2::new(2.0, 2.0));

        assert_equal!(diff.length(), 2.828427);

        Ok(())
    }
}
