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
use wgpu::util::DeviceExt;

use crate::basic_uniforms::BasicUniforms;
use crate::global_state;
use crate::simulation_graph::SimulationGraph;

pub struct GraphState {
    pub array_graph: ArrayGraph,
    pub selected_metric: Option<String>,
    pub(crate) simulation_graph: SimulationGraph,
    // since this requires some initialization logic to run let's
    // make sure it can't be created outside of this module
    _phantom: (),
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
            contents: graph_state.simulation_graph.nodes_bytes(),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });

        let edges_bytes = graph_state.simulation_graph.edges_bytes();
        let edges_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Edges Buffer"),
            contents: edges_bytes,
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
    pub fn new(array_graph: ArrayGraph) -> Result<Self> {
        // by default we'll grab whatever metric is first in the list
        let selected_metric = array_graph.metrics.keys().next().cloned();
        let simulation_graph = SimulationGraph::new(&array_graph, &selected_metric)?;

        let result = Self {
            array_graph,
            selected_metric,
            simulation_graph,
            _phantom: (),
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
        self.simulation_graph = SimulationGraph::new(&self.array_graph, &self.selected_metric)?;
        Ok(())
    }
}
