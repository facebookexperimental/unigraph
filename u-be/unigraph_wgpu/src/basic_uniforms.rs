// Copyright (c) Meta Platforms, Inc. and affiliates.

use bytemuck::Pod;
use bytemuck::Zeroable;
use glam::Vec2;
use unigraph_graph_state::types::SelectionType;
use wgpu::util::DeviceExt;

#[derive(Copy, Clone, Debug, Pod, Zeroable)]
#[repr(C)]
pub struct UniformsStruct {
    pub aspect_ratio: f32,
    pub node_size_scale: f32, // 1 to 100

    pub selection_from_point: Vec2,
    pub selection_to_point: Vec2,
    pub selection_type: SelectionType,

    pub _padding1: [u32; 1],

    pub background_color: [f32; 3],
    pub _padding2: [u32; 1],
    pub node_main_color: [f32; 3],
    pub _padding3: [u32; 1],
    pub node_selected_color: [f32; 3],
    pub _padding4: [u32; 1],
}

#[derive(Clone)]
pub struct BasicUniforms {
    pub uniforms: UniformsStruct,
    pub uniforms_bind_group: wgpu::BindGroup,
    pub uniforms_buffer: wgpu::Buffer,
    pub uniforms_bind_group_layout: wgpu::BindGroupLayout,
}

impl BasicUniforms {
    pub fn as_bytes(&'_ self) -> &'_ [u8] {
        bytemuck::bytes_of(&self.uniforms)
    }

    pub fn new(uniforms: UniformsStruct, device: &wgpu::Device) -> Self {
        let uniforms_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Basic Uniforms Buffer"),
            contents: bytemuck::bytes_of(&uniforms),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let uniforms_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Basic Uniforms Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let uniforms_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Basic Uniforms Bind Group"),
            layout: &uniforms_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms_buffer.as_entire_binding(),
            }],
        });

        Self {
            uniforms,
            uniforms_bind_group,
            uniforms_buffer,
            uniforms_bind_group_layout,
        }
    }
}
