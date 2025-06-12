// Copyright (c) Meta Platforms, Inc. and affiliates.

mod barnes_hut;
mod basic_uniforms;
mod global_state;
mod graph_state;
mod shared;
pub mod ts_types;
pub mod unigraph_error;
use std::cmp;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use basic_uniforms::BasicUniforms;
use basic_uniforms::UniformsStruct;
use glam::Vec2;
pub use global_state::GlobalState;
pub use global_state::global_state;
use graph_state::WGPUGraphState;
use shared::create_shader;
use ts_types::SelectionType;
use unigraph_core::ArrayGraph;
use unigraph_error::UnigraphError;
use wgpu::TextureFormat;
use winit::application::ApplicationHandler;
use winit::event::*;
use winit::event_loop::ActiveEventLoop;
use winit::event_loop::ControlFlow;
use winit::event_loop::EventLoop;
use winit::window::Window;
use winit::window::WindowAttributes;

#[derive(Debug, Clone, Copy)]
pub enum UserEvent {
    WakeUp,
}

pub struct WGPUApplication {
    state: Option<WGPUState>,

    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    graph_shader: wgpu::ShaderModule,
    window_attributes_factory: Box<dyn WindowAttributesFactory>,
    frame_counter: u64,
}

struct WGPUState {
    device: wgpu::Device,
    queue: wgpu::Queue,
    basic_uniforms: BasicUniforms,

    surface: wgpu::Surface<'static>,
    window: Arc<Window>,
    surface_config: wgpu::SurfaceConfiguration,

    pub wgpu_graph_state: WGPUGraphState,
}

impl WGPUState {
    fn new(
        instance: wgpu::Instance,
        adapter: wgpu::Adapter,
        device: wgpu::Device,
        queue: wgpu::Queue,
        graph_shader: wgpu::ShaderModule,
        window: Arc<Window>,
    ) -> Result<Self> {
        let surface = instance.create_surface(window.clone()).unwrap();

        let swapchain_capabilities = surface.get_capabilities(&adapter);

        // By default it would use srgb color space in native builds but unorm in web builds.
        // This results in different colors in web and native builds.
        // To adjust for this i'll try to default to Bgra8Unorm if available
        // (srgb is not available in wasm. idk why)
        let swapchain_format = if swapchain_capabilities
            .formats
            .contains(&TextureFormat::Bgra8Unorm)
        {
            TextureFormat::Bgra8Unorm
        } else {
            swapchain_capabilities.formats[0]
        };

        let mut size = window.inner_size();
        size.width = size.width.max(1);
        size.height = size.height.max(1);

        let mut surface_config = surface
            .get_default_config(&adapter, size.width, size.height)
            .unwrap();
        surface_config.format = swapchain_format;
        surface.configure(&device, &surface_config);

        let aspect_ratio = GlobalState::surface_size()
            .set(size.width, size.height)
            .aspect_ratio();

        let simulation_params = global_state().simulation_params.get();
        let uniforms = UniformsStruct {
            aspect_ratio,
            node_size_scale: simulation_params.node_size_scale as f32,

            selection_from_point: Vec2::ZERO,
            selection_to_point: Vec2::ZERO,
            selection_type: SelectionType::None,

            background_color: simulation_params.colors.background,
            node_main_color: simulation_params.colors.node_main,
            node_selected_color: simulation_params.colors.node_selected,
            _padding1: Default::default(),
            _padding2: Default::default(),
            _padding3: Default::default(),
            _padding4: Default::default(),
        };
        let basic_uniforms = BasicUniforms::new(uniforms, &device);

        let wgpu_graph_state =
            WGPUGraphState::new(&device, &basic_uniforms, graph_shader, swapchain_format);

        let state = WGPUState {
            basic_uniforms,
            wgpu_graph_state,
            surface,
            window: window.clone(),
            surface_config,
            device,
            queue,
        };
        Ok(state)
    }

    fn render(&self) {
        let frame = self
            .surface
            .get_current_texture()
            .expect("Failed to acquire next swap chain texture");
        let descriptor = wgpu::TextureViewDescriptor {
            format: Some(self.surface_config.format),
            ..Default::default()
        };
        let view = frame.texture.create_view(&descriptor);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let simulation_params = global_state().simulation_params.get();
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: simulation_params.colors.background[0] as f64,
                            g: simulation_params.colors.background[1] as f64,
                            b: simulation_params.colors.background[2] as f64,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            rpass.set_bind_group(0, &self.basic_uniforms.uniforms_bind_group, &[]);

            let (node_ct, edge_ct) = {
                let graph_state = global_state().graph_state.get();
                (
                    graph_state.node_attributes.len() as u32,
                    graph_state.array_graph.edges_forward.edges_len() as u32,
                )
            };
            if global_state().simulation_params.get().render_edges {
                rpass.set_pipeline(&self.wgpu_graph_state.edge_pipeline);
                rpass.set_bind_group(1, &self.wgpu_graph_state.graph_bind_group, &[]);
                rpass.draw(0..2, 0..edge_ct);
            }

            rpass.set_pipeline(&self.wgpu_graph_state.node_pipeline);
            rpass.set_bind_group(1, &self.wgpu_graph_state.graph_bind_group, &[]);
            rpass.draw(0..6, 0..node_ct);

            let selection = global_state().simulation_params.get().selection;
            if selection.selection_type == SelectionType::Box {
                rpass.set_pipeline(&self.wgpu_graph_state.box_selection_pipeline);
                rpass.draw(0..6, 0..1);
            }
        }

        self.queue.submit(Some(encoder.finish()));
        self.window.pre_present_notify();
        frame.present();
    }

    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        // Reconfigure the surface with the new size
        self.surface_config.width = new_size.width.max(1);
        self.surface_config.height = new_size.height.max(1);
        self.surface.configure(&self.device, &self.surface_config);
        let new_aspect_ratio = GlobalState::surface_size()
            .set(new_size.width, new_size.height)
            .aspect_ratio();

        self.basic_uniforms.uniforms.aspect_ratio = new_aspect_ratio;

        self.queue.write_buffer(
            &self.basic_uniforms.uniforms_buffer,
            0,
            self.basic_uniforms.as_bytes(),
        );
        self.window.request_redraw();
    }

    fn update_uniforms(&mut self) {
        let simulation_params = global_state().simulation_params.get();

        self.basic_uniforms.uniforms.node_size_scale = simulation_params.node_size_scale as f32;

        let selection = simulation_params.selection;
        self.basic_uniforms.uniforms.selection_from_point = selection.selection_from_point;
        self.basic_uniforms.uniforms.selection_to_point = selection.selection_to_point;
        self.basic_uniforms.uniforms.selection_type = selection.selection_type;

        self.basic_uniforms.uniforms.background_color = simulation_params.colors.background;
        self.basic_uniforms.uniforms.node_main_color = simulation_params.colors.node_main;

        self.queue.write_buffer(
            &self.basic_uniforms.uniforms_buffer,
            0,
            self.basic_uniforms.as_bytes(),
        );
    }
}

pub trait WindowAttributesFactory {
    fn create_attributes(&self) -> Result<Option<WindowAttributes>>;
}

struct NativeWindowAttributesFactory;
impl WindowAttributesFactory for NativeWindowAttributesFactory {
    fn create_attributes(&self) -> Result<Option<WindowAttributes>> {
        Ok(Some(Window::default_attributes()))
    }
}
impl WGPUApplication {
    fn init_state(&mut self, event_loop: &ActiveEventLoop) {
        let attributes = match self.window_attributes_factory.create_attributes().unwrap() {
            Some(attributes) => attributes,
            None => {
                log::trace!("Could not create window attributes");
                return;
            }
        };

        let window = Arc::new(event_loop.create_window(attributes).unwrap());

        let state = WGPUState::new(
            self.instance.clone(),
            self.adapter.clone(),
            self.device.clone(),
            self.queue.clone(),
            self.graph_shader.clone(),
            window.clone(),
        )
        .expect("Failed to create WGPUState");

        state.render();
        self.state = Some(state);

        // Set the event loop to run continuously
        event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

        window.request_redraw();
    }
}

impl ApplicationHandler<UserEvent> for WGPUApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        log::trace!("Resumed");
        self.init_state(event_loop);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::WakeUp => {
                log::trace!("Waking up");
                self.init_state(event_loop);
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let loop_active = global_state()
            .event_loop_active
            .load(std::sync::atomic::Ordering::SeqCst);

        if !loop_active {
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        } else {
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                web_time::Instant::now() + web_time::Duration::from_millis(40),
            ));
        }

        let state = if let Some(state) = self.state.as_mut() {
            state
        } else {
            return;
        };

        match event {
            WindowEvent::Resized(new_size) => {
                log::trace!("Resized to {new_size:?}");
                state.resize(new_size);
            }
            WindowEvent::RedrawRequested => {
                state.update_uniforms();
                state.render();

                let params = global_state().simulation_params.get();
                if params.active {
                    let update_forces = self.frame_counter
                        % cmp::max(params.compute_forces_every_n_frames as u64, 1)
                        == 0;
                    // NOTE: this requires a write lock on the graph state
                    global_state().graph_state.compute_next_frame(update_forces);

                    let graph_state = global_state().graph_state.get();
                    state.queue.write_buffer(
                        &state.wgpu_graph_state.nodes_buffer,
                        0,
                        graph_state.nodes_bytes(),
                    );

                    // NOTE: we don't modify the edges buffer, because
                    // they are normally immutable. If we start modifying them
                    // (including/excluding edges) we will need to update the buffer
                    // here as well

                    // state.queue.write_buffer(
                    //     &state.graph_state.edges_buffer,
                    //     0,
                    //     state.graph_state.attributes.edges_bytes(),
                    // );
                }
                state.window.request_redraw();
                self.frame_counter += 1;
            }
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            _ => {}
        }
    }
}

pub async fn create_application(
    window_attributes_factory: Box<dyn WindowAttributesFactory>,
) -> Result<WGPUApplication> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::from_env_or_default());

    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
        })
        .await
        .expect("Failed to find an appropriate adapter");

    let mut required_limits =
        wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits());
    required_limits.max_storage_buffers_per_shader_stage = 8;
    required_limits.max_storage_buffer_binding_size = 1000 * 1000 * 100;

    // Create the logical device and command queue
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: None,
            required_features: wgpu::Features::empty(),
            required_limits,
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            trace: wgpu::Trace::Off,
        })
        .await
        .expect("Failed to create device");

    let graph_shader = create_shader(include_str!("graph_shader.wgsl"), &device).await?;

    Ok(WGPUApplication {
        state: None,
        instance,
        adapter,
        device,
        queue,
        graph_shader,
        window_attributes_factory,
        frame_counter: 0,
    })
}

async fn run(event_loop: EventLoop<UserEvent>) {
    let mut app = create_application(Box::new(NativeWindowAttributesFactory))
        .await
        .expect("Failed to create application");

    event_loop.run_app(&mut app).unwrap();
}

pub async fn start(array_graph: ArrayGraph) -> Result<(), UnigraphError> {
    GlobalState::init();
    let event_loop = EventLoop::<UserEvent>::with_user_event().build().unwrap();
    GlobalState::graph_state().replace_graph(array_graph);
    GlobalState::set_event_loop_proxy(event_loop.create_proxy());
    run(event_loop).await;
    Ok(())
}

pub fn get_selected_node_idxs() -> Result<Vec<usize>> {
    let selection = GlobalState::simulation_params().selection;
    GlobalState::graph_state_mut().mark_nodes_as_selected(&selection)
}

pub fn set_event_loop_active(active: bool) -> Result<()> {
    GlobalState::get()
        .event_loop_active
        .store(active, std::sync::atomic::Ordering::SeqCst);

    if active {
        // wake up the event loop if it was waiting
        global_state()
            .event_loop_proxy
            .read()
            .unwrap()
            .as_ref()
            .context("EventLoopProxy is missing")?
            .send_event(UserEvent::WakeUp)?;
    }
    Ok(())
}
