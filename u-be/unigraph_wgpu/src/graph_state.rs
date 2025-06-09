// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::sync::Arc;
use std::sync::RwLock;
use std::sync::RwLockReadGuard;
use std::sync::RwLockWriteGuard;

use anyhow::Result;
use bytemuck::Pod;
use bytemuck::Zeroable;
use glam::Vec2;
use rand::Rng;
use unigraph_core::ArrayGraph;
use unigraph_core::types::NodeIDX;
use wgpu::util::DeviceExt;

use crate::barnes_hut::BHGraphNode;
use crate::barnes_hut::QuadTree;
use crate::basic_uniforms::BasicUniforms;
use crate::global_state;
use crate::global_state::GlobalState;
use crate::ts_types::Selection;
use crate::ts_types::SelectionType;

pub struct GraphState {
    pub array_graph: ArrayGraph,
    pub node_attributes: Vec<NodeAttributes>,
    pub selected_metric: Option<String>,
    // since this requires some initialization logic to run let's
    // make sure it can't be created outside of this module
    _phantom: (),
}

pub struct SharedGraphState {
    inner: Arc<RwLock<GraphState>>,
}

impl SharedGraphState {
    pub fn new(array_graph: ArrayGraph) -> Self {
        Self {
            inner: Arc::new(RwLock::new(GraphState::new(array_graph))),
        }
    }

    pub fn get(&self) -> RwLockReadGuard<GraphState> {
        self.inner.read().unwrap()
    }

    pub fn get_mut(&self) -> RwLockWriteGuard<GraphState> {
        self.inner.write().unwrap()
    }

    pub fn replace_graph(&self, new_graph: ArrayGraph) {
        let new_state = GraphState::new(new_graph);
        let mut inner = self.get_mut();
        *inner = new_state;
    }

    pub fn compute_next_frame(&self) {
        let mut graph_state = self.get_mut();
        graph_state.compute_next_frame();
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
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

pub struct WGPUGraphState {
    pub graph_bind_group: wgpu::BindGroup,

    pub nodes_buffer: wgpu::Buffer,
    #[allow(dead_code)]
    pub edges_buffer: wgpu::Buffer,

    pub node_pipeline: wgpu::RenderPipeline,
    pub edge_pipeline: wgpu::RenderPipeline,
    pub box_selection_pipeline: wgpu::RenderPipeline,
}

impl WGPUGraphState {
    pub fn new(
        device: &wgpu::Device,
        basic_uniforms: &BasicUniforms,
        graph_shader: wgpu::ShaderModule,
        swapchain_format: wgpu::TextureFormat,
    ) -> Self {
        let graph_state = global_state().graph_state.get();
        let nodes_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Graph Buffer"),
            contents: graph_state.nodes_bytes(),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });

        let edge_bytes = graph_state.edges_buffer();
        let edges_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Edges Buffer"),
            contents: bytemuck::cast_slice(&edge_bytes),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });

        let graph_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Graph Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<
                                NodeAttributes,
                            >()
                                as _),
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<
                                EdgeAttributes,
                            >()
                                as _),
                        },
                        count: None,
                    },
                ],
            });

        let graph_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Graph Bind Group"),
            layout: &graph_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: nodes_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: edges_buffer.as_entire_binding(),
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Graph Pipeline Layout"),
            bind_group_layouts: &[
                &basic_uniforms.uniforms_bind_group_layout,
                &graph_bind_group_layout,
            ],
            push_constant_ranges: &[],
        });

        let node_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Graph Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &graph_shader,
                entry_point: Some("vs_node"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &graph_shader,
                entry_point: Some("fs_node"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: swapchain_format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent::OVER,
                        alpha: wgpu::BlendComponent::OVER,
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let edge_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Edge Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &graph_shader,
                entry_point: Some("vs_edge"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &graph_shader,
                entry_point: Some("fs_edge"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: swapchain_format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::SrcAlpha,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let box_selection_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Box Selection Pipeline Layout"),
                bind_group_layouts: &[&basic_uniforms.uniforms_bind_group_layout],
                push_constant_ranges: &[],
            });

        let box_selection_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Box Selection Pipeline"),
                layout: Some(&box_selection_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &graph_shader,
                    entry_point: Some("vs_box_selection"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &graph_shader,
                    entry_point: Some("fs_box_selection"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: swapchain_format,
                        blend: Some(wgpu::BlendState {
                            color: wgpu::BlendComponent {
                                src_factor: wgpu::BlendFactor::SrcAlpha,
                                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                                operation: wgpu::BlendOperation::Add,
                            },
                            alpha: wgpu::BlendComponent {
                                src_factor: wgpu::BlendFactor::One,
                                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                                operation: wgpu::BlendOperation::Add,
                            },
                        }),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });

        Self {
            nodes_buffer,
            graph_bind_group,
            edges_buffer,
            node_pipeline,
            edge_pipeline,
            box_selection_pipeline,
        }
    }
}

impl GraphState {
    pub fn new(array_graph: ArrayGraph) -> Self {
        // by default we'll grab whatever metric is first in the list
        let selected_metric = array_graph.metrics.keys().next().cloned();
        let node_attributes = Self::initialize_node_attributes(&array_graph);

        let mut result = Self {
            array_graph,
            node_attributes,
            selected_metric,
            _phantom: (),
        };
        result.recalculate_adjusted_sizes();
        result
    }

    pub fn get_selected_metrics_vec(&self) -> Option<&Vec<f32>> {
        if let Some(selected_metric) = &self.selected_metric {
            self.array_graph.metrics.get(selected_metric)
        } else {
            None
        }
    }

    pub fn edges_buffer(&self) -> Vec<EdgeAttributes> {
        let mut edges: Vec<EdgeAttributes> = vec![];

        for idx in self.array_graph.node_idx_iter() {
            for edge in self.array_graph.edges_forward.edges(idx) {
                edges.push(EdgeAttributes {
                    from: idx,
                    to: edge.points_to,
                });
            }
        }
        edges
    }

    pub fn recalculate_adjusted_sizes(&mut self) {
        let nodes_len = self.array_graph.nodes_len();
        if nodes_len == 0 {
            return;
        }
        let mut all_sizes = Vec::with_capacity(nodes_len);

        if let Some(selected_metrics) = self.get_selected_metrics_vec() {
            for idx in self.array_graph.node_idx_iter() {
                all_sizes.push(selected_metrics[idx]);
            }
        }

        if all_sizes.is_empty() {
            return;
        }

        let mut sorted = all_sizes.clone();
        sort_vec_f32(&mut sorted);

        let min_size = sorted[0];
        let max_size = sorted[sorted.len() - 1];

        for idx in self.array_graph.node_idx_iter() {
            let size = all_sizes[idx];
            let adjusted_size = if size == 0.0 {
                1.0
            } else {
                // Normalize the size to be between 0 and 1
                let normalized_size = (size - min_size) / (max_size - min_size);
                // Scale it to be between 1 and 100
                normalized_size * 99.0 + 1.0
            };
            self.node_attributes[idx].adjusted_size = adjusted_size;
        }
    }

    pub fn initialize_node_attributes(array_graph: &ArrayGraph) -> Vec<NodeAttributes> {
        let mut rng = rand::rng();
        let mut nodes = Vec::with_capacity(array_graph.nodes_len());
        if array_graph.is_empty() {
            return nodes;
        }

        for _idx in array_graph.node_idx_iter() {
            nodes.push(NodeAttributes {
                position: Vec2 {
                    x: rng.random_range(-1.0..1.0),
                    y: rng.random_range(-1.0..1.0),
                }
                .clamp_length_max(0.01),
                velocity: Vec2 {
                    x: rng.random_range(-1.0..1.0),
                    y: rng.random_range(-1.0..1.0),
                }
                .clamp_length_max(0.0001),
                adjusted_size: 1.0,
                flags: NodeAttributesFlags::empty(),
                _padding: Default::default(),
            });
        }
        nodes
    }

    pub fn nodes_bytes(&'_ self) -> &'_ [u8] {
        bytemuck::cast_slice(&self.node_attributes)
    }

    // Process the next iteration of the simulation, which involves calculating all
    // forces and adjusting node velocities and positions accordingly.
    pub fn compute_next_frame(&mut self) {
        let mut quad_tree = QuadTree::new(300);
        for (idx, node) in self.node_attributes.iter().enumerate() {
            if node.flags.contains(NodeAttributesFlags::UNREACHABLE) {
                continue;
            }

            quad_tree.add_body(BHGraphNode {
                position: node.position,
                idx,
                mass: node.adjusted_size,
            });
        }

        const TERMINAL_VELOCITY: f32 = 0.01;
        let params = global_state().simulation_params.get();

        let gravity_forces = quad_tree.compute_forces(self.array_graph.nodes_len());
        let edge_forces = self.get_edge_forces();
        let center_pull_forces = self.forces_pull_towards_center();
        // update node positions based on forces
        for (idx, node) in self.node_attributes.iter_mut().enumerate() {
            // Apply the force to the node's velocity
            let force = edge_forces[idx] * params.edge_force_multiplier + center_pull_forces[idx]
                - gravity_forces[idx] * params.gravity_force_multiplier;

            node.velocity += force * params.max_velocity_multiplier;
            node.velocity = node.velocity.clamp_length_max(TERMINAL_VELOCITY);

            // Add some friction. This will slow down the nodes over time.
            const SLOW_DOWN: f32 = 0.9;
            node.velocity *= SLOW_DOWN;

            // Update the node's position based on its velocity
            node.position += node.velocity;

            node.position = node.position.clamp(Vec2::splat(-0.95), Vec2::splat(0.95));
        }
    }

    pub fn get_edge_forces(&self) -> Vec<Vec2> {
        // Calculate forces based on edges
        // These edges will act as springs and try to pull the nodes together.
        let mut forces = vec![Vec2::ZERO; self.node_attributes.len()];

        for from in self.array_graph.node_idx_iter() {
            for to in self.array_graph.edges_forward.edges(from) {
                let dx = self.node_attributes[to.points_to].position.x
                    - self.node_attributes[from].position.x;
                let dy = self.node_attributes[to.points_to].position.y
                    - self.node_attributes[from].position.y;

                let distance_squared = dx * dx + dy * dy + 0.0001; // Avoid division by zero

                let distance = distance_squared.sqrt();
                let force_magnitude = 0.0009 * distance.ln_1p(); // Use natural log (ln(1 + x)) for linlog

                // Calculate components of the force
                let fx = (dx / distance) * force_magnitude;
                let fy = (dy / distance) * force_magnitude;

                let force = Vec2 { x: fx, y: fy };
                forces[from] += force;
                forces[to.points_to] -= force;
            }
        }
        forces
    }

    fn forces_pull_towards_center(&self) -> Vec<Vec2> {
        const CENTER_PULL_STRENGTH: f32 = 0.0007;
        // Calculate forces pulling nodes towards the center (0, 0)
        let mut forces = vec![Vec2::ZERO; self.node_attributes.len()];
        for (idx, node) in self.node_attributes.iter().enumerate() {
            let dx = -node.position.x;
            let dy = -node.position.y;

            let distance_squared = dx * dx + dy * dy + 0.001; // Avoid division by zero
            let distance = distance_squared.sqrt();
            let force_magnitude = CENTER_PULL_STRENGTH * distance * distance;

            // Calculate components of the force
            let fx = dx / distance * force_magnitude;
            let fy = dy / distance * force_magnitude;

            forces[idx] += Vec2 { x: fx, y: fy };
        }
        forces
    }

    pub fn mark_nodes_as_selected(&mut self, selection: &Selection) -> Result<Vec<usize>> {
        let aspect_ratio = GlobalState::surface_size().aspect_ratio();

        match selection.selection_type {
            SelectionType::None => Ok(vec![]),
            SelectionType::Box => {
                let mut selected_nodes = vec![];
                for (idx, node) in self.node_attributes.iter_mut().enumerate() {
                    if selection.within_box_bounds(node.position, aspect_ratio) {
                        node.flags.insert(NodeAttributesFlags::SELECTED);
                        selected_nodes.push(idx);
                    } else {
                        node.flags.remove(NodeAttributesFlags::SELECTED);
                    }
                }
                Ok(selected_nodes)
            }
            SelectionType::Line => {
                anyhow::bail!("Line selection not implemented yet");
            }
        }
    }

    // syncs flags from the array graph to the node attributes that we'll
    // then be able to pass to the shader.
    pub fn sync_node_attributes(&mut self) -> Result<()> {
        for node_idx in self.array_graph.node_idx_iter() {
            let unreachable = self.array_graph.node_flags[node_idx].is_node_unreachable();
            if unreachable {
                self.node_attributes[node_idx]
                    .flags
                    .insert(NodeAttributesFlags::UNREACHABLE);
            } else {
                self.node_attributes[node_idx]
                    .flags
                    .remove(NodeAttributesFlags::UNREACHABLE);
            }
        }

        Ok(())
    }
}

fn sort_vec_f32(vec: &mut [f32]) {
    vec.sort_by(|a, b| a.partial_cmp(b).unwrap());
}
