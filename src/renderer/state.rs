use crate::renderer::feature_uniform::FeatureUniform;
use crate::renderer::image_buffer::ImageBuffer;
use crate::renderer::rectangle_buffer::RectangleBuffer;
use crate::renderer::Vertex;
use crate::{
    renderer::{Texture, TextureVertex},
    Png,
};
use anyhow::{anyhow, Result};
use std::iter;
use wgpu::{
    util::{BufferInitDescriptor, DeviceExt},
    Backends, BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, BlendComponent, BlendFactor,
    BlendOperation, BlendState, Buffer, BufferBindingType, BufferUsages, Color, ColorTargetState,
    ColorWrites, CommandEncoderDescriptor, Device, DeviceDescriptor, Features, FragmentState,
    FrontFace, IndexFormat, Instance, InstanceDescriptor, Limits, LoadOp, MultisampleState,
    Operations, PipelineLayoutDescriptor, PolygonMode, PowerPreference, PrimitiveState,
    PrimitiveTopology, Queue, RenderPassColorAttachment, RenderPassDescriptor, RenderPipeline,
    RenderPipelineDescriptor, RequestAdapterOptions, SamplerBindingType, ShaderModuleDescriptor,
    ShaderSource, ShaderStages, StoreOp, Surface, SurfaceConfiguration, SurfaceError,
    TextureSampleType, TextureUsages, TextureViewDescriptor, TextureViewDimension, VertexState,
};
use winit::{
    dpi::PhysicalSize,
    event::*,
    event_loop::EventLoop,
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowBuilder},
};

struct State<'a> {
    surface: Surface<'a>,
    device: Device,
    queue: Queue,
    config: SurfaceConfiguration,
    size: PhysicalSize<u32>,
    render_pipeline: RenderPipeline,
    quad_render_pipeline: RenderPipeline,
    #[allow(dead_code)]
    diffuse_texture: Texture,
    diffuse_bind_group: BindGroup,
    window: &'a Window,
    image_buffer: ImageBuffer,
    rectangle_buffer: RectangleBuffer,
    feature_uniform: FeatureUniform,
    feature_buffer: Buffer,
    feature_bind_group: BindGroup,
}

impl<'a> State<'a> {
    async fn new(window: &'a Window, png: &'a Png) -> Result<State<'a>> {
        let size = window.inner_size();

        // The instance is a handle to our GPU
        // BackendBit::PRIMARY => Vulkan + Metal + DX12 + Browser WebGPU
        let instance = Instance::new(InstanceDescriptor {
            #[cfg(not(target_arch = "wasm32"))]
            backends: Backends::PRIMARY,
            #[cfg(target_arch = "wasm32")]
            backends: Backends::GL,
            ..Default::default()
        });

        let surface = instance.create_surface(window)?;

        let adapter = instance
            .request_adapter(&RequestAdapterOptions {
                power_preference: PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| anyhow!("Failed to get adapter"))?;

        let (device, queue) = adapter
            .request_device(
                &DeviceDescriptor {
                    label: None,
                    required_features: Features::empty(),
                    // WebGL doesn't support all of wgpu's features, so if
                    // we're building for the web we'll have to disable some.
                    required_limits: if cfg!(target_arch = "wasm32") {
                        Limits::downlevel_webgl2_defaults()
                    } else {
                        Limits::default()
                    },
                    memory_hints: Default::default(),
                },
                None, // Trace path
            )
            .await?;

        let surface_caps = surface.get_capabilities(&adapter);
        // Shader code in this tutorial assumes an Srgb surface texture. Using a different
        // one will result all the colors coming out darker. If you want to support non
        // Srgb surfaces, you'll need to account for that when drawing to the frame.
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        let config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        let diffuse_texture = Texture::from_bytes(&device, &queue, png)?;

        let texture_bind_group_layout =
            device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                entries: &[
                    BindGroupLayoutEntry {
                        binding: 0,
                        visibility: ShaderStages::FRAGMENT,
                        ty: BindingType::Texture {
                            multisampled: false,
                            view_dimension: TextureViewDimension::D2,
                            sample_type: TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    BindGroupLayoutEntry {
                        binding: 1,
                        visibility: ShaderStages::FRAGMENT,
                        ty: BindingType::Sampler(SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
                label: Some("texture_bind_group_layout"),
            });

        let diffuse_bind_group = device.create_bind_group(&BindGroupDescriptor {
            layout: &texture_bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(&diffuse_texture.view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Sampler(&diffuse_texture.sampler),
                },
            ],
            label: Some("diffuse_bind_group"),
        });

        let feature_uniform = FeatureUniform::new(config.width, config.height, png.gamma);

        let feature_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("Color Tone Buffer"),
            contents: bytemuck::cast_slice(&[feature_uniform]),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });

        let feature_bind_group_layout =
            device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                entries: &[BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
                label: Some("feature_bind_group_layout"),
            });

        let feature_bind_group = device.create_bind_group(&BindGroupDescriptor {
            layout: &feature_bind_group_layout,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: feature_buffer.as_entire_binding(),
            }],
            label: Some("feature_bind_group"),
        });

        let quad_shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Quad Shader"),
            source: ShaderSource::Wgsl(include_str!("quad_shader.wgsl").into()),
        });

        let quad_render_pipeline_layout =
            device.create_pipeline_layout(&PipelineLayoutDescriptor {
                label: Some("Quad Render Pipeline Layout"),
                bind_group_layouts: &[],
                push_constant_ranges: &[],
            });

        let quad_render_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("Quad Render Pipeline"),
            layout: Some(&quad_render_pipeline_layout),
            vertex: VertexState {
                module: &quad_shader,
                entry_point: "vs_main",
                buffers: &[Vertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(FragmentState {
                module: &quad_shader,
                entry_point: "fs_main",
                targets: &[Some(ColorTargetState {
                    format: config.format,
                    blend: Some(BlendState {
                        color: BlendComponent {
                            src_factor: BlendFactor::SrcAlpha,
                            dst_factor: BlendFactor::OneMinusSrcAlpha,
                            operation: BlendOperation::Add,
                        },
                        alpha: BlendComponent::OVER,
                    }),
                    write_mask: ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Shader"),
            source: ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        let render_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("Render Pipeline Layout"),
            bind_group_layouts: &[&texture_bind_group_layout, &feature_bind_group_layout],
            push_constant_ranges: &[],
        });

        let render_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[TextureVertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(ColorTargetState {
                    format: config.format,
                    blend: Some(BlendState {
                        color: BlendComponent::REPLACE,
                        alpha: BlendComponent::REPLACE,
                    }),
                    write_mask: ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: FrontFace::Ccw,
                cull_mode: None,
                // Setting this to anything other than Fill requires Features::POLYGON_MODE_LINE
                // or Features::POLYGON_MODE_POINT
                polygon_mode: PolygonMode::Fill,
                // Requires Features::DEPTH_CLIP_CONTROL
                unclipped_depth: false,
                // Requires Features::CONSERVATIVE_RASTERIZATION
                conservative: false,
            },
            depth_stencil: None,
            multisample: MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            // If the pipeline will be used with a multiview renderer pass, this
            // indicates how many array layers the attachments will have.
            multiview: None,
            // Useful for optimizing shader compilation on Android
            cache: None,
        });

        let image_buffer = ImageBuffer::new(&device);
        let rectangle_buffer = RectangleBuffer::new(&device);

        Ok(Self {
            surface,
            device,
            queue,
            config,
            size,
            render_pipeline,
            quad_render_pipeline,
            diffuse_texture,
            diffuse_bind_group,
            window,
            feature_uniform,
            feature_buffer,
            feature_bind_group,
            image_buffer,
            rectangle_buffer,
        })
    }

    pub const fn window(&self) -> &Window {
        self.window
    }

    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    fn input(&mut self, event: &WindowEvent) -> bool {
        let feature_uniform = &mut self.feature_uniform;

        match event {
            WindowEvent::CursorMoved { position, .. } => {
                let (tex_x, tex_y) = {
                    let x = position.x as f32;
                    let y = position.y as f32;
                    let width = self.config.width as f32;
                    let height = self.config.height as f32;

                    (2.0 * (x / width) - 1.0, 1.0 - 2.0 * (y / height))
                };

                let is_inside_quad =
                    { -0.5 <= tex_x && tex_x <= 0.5 && -0.5 <= tex_y && tex_y <= 0.5 };

                true
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        state,
                        physical_key: PhysicalKey::Code(keycode),
                        ..
                    },
                ..
            } => match (keycode, state) {
                (KeyCode::KeyC, ElementState::Pressed) => {
                    feature_uniform.reset_features();
                    true
                }
                (KeyCode::KeyB, ElementState::Pressed) => {
                    feature_uniform.toggle_blur();
                    true
                }
                (KeyCode::ArrowUp, ElementState::Pressed) => {
                    if feature_uniform.blur() {
                        feature_uniform.increase_blur_radius();
                    }

                    if feature_uniform.sharpen() {
                        feature_uniform.increase_sharpen_factor();
                    }

                    true
                }
                (KeyCode::ArrowDown, ElementState::Pressed) => {
                    if feature_uniform.blur() {
                        feature_uniform.decrease_blur_radius();
                    }

                    if feature_uniform.sharpen() {
                        feature_uniform.decrease_sharpen_factor();
                    }

                    true
                }
                (KeyCode::KeyG, ElementState::Pressed) => {
                    feature_uniform.toggle_grayscale();
                    true
                }
                (KeyCode::KeyS, ElementState::Pressed) => {
                    feature_uniform.toggle_sharpen();
                    true
                }
                (KeyCode::KeyI, ElementState::Pressed) => {
                    feature_uniform.toggle_invert();
                    true
                }
                (KeyCode::KeyE, ElementState::Pressed) => {
                    feature_uniform.toggle_edge_detect();
                    true
                }
                _ => false,
            },
            _ => false,
        }
    }

    fn update(&self) {
        self.queue.write_buffer(
            &self.feature_buffer,
            0,
            bytemuck::cast_slice(&[self.feature_uniform]),
        );
    }

    fn render(&self) -> Result<(), SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color {
                            r: 0.1,
                            g: 0.2,
                            b: 0.3,
                            a: 1.0,
                        }),
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.diffuse_bind_group, &[]);
            render_pass.set_bind_group(1, &self.feature_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.image_buffer.vertex_buffer().slice(..));
            render_pass.set_index_buffer(
                self.image_buffer.index_buffer().slice(..),
                IndexFormat::Uint16,
            );

            render_pass.draw_indexed(0..self.image_buffer.num_indices(), 0, 0..1);

            render_pass.set_pipeline(&self.quad_render_pipeline);
            render_pass.set_vertex_buffer(0, self.rectangle_buffer.vertex_buffer().slice(..));
            render_pass.set_index_buffer(
                self.rectangle_buffer.index_buffer().slice(..),
                IndexFormat::Uint16,
            );
            render_pass.draw_indexed(0..self.rectangle_buffer.num_indices(), 0, 0..1);
        }

        self.queue.submit(iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}

#[allow(clippy::future_not_send)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen(start))]
pub async fn run(png: Png) -> anyhow::Result<()> {
    cfg_if::cfg_if! {
        if #[cfg(target_arch = "wasm32")] {
            std::panic::set_hook(Box::new(console_error_panic_hook::hook));
            console_log::init_with_level(log::Level::Warn).expect("Couldn't initialize logger");
        } else {
            env_logger::init();
        }
    }

    let event_loop = EventLoop::new()?;

    let (width, height) = png.dimensions();

    let window = WindowBuilder::new()
        .with_inner_size(PhysicalSize::new(width, height))
        .with_title("friendlymatthew/png")
        .build(&event_loop)?;

    #[cfg(target_arch = "wasm32")]
    {
        // Winit prevents sizing with CSS, so we have to set
        // the size manually when on web.
        use winit::dpi::PhysicalSize;
        let _ = window.request_inner_size(PhysicalSize::new(450, 400));

        use winit::platform::web::WindowExtWebSys;
        web_sys::window()
            .and_then(|win| win.document())
            .and_then(|doc| {
                let dst = doc.get_element_by_id("wasm-example")?;
                let canvas = web_sys::Element::from(window.canvas()?);
                dst.append_child(&canvas).ok()?;
                Some(())
            })
            .expect("Couldn't append canvas to document body.");
    }

    // State::new uses async code, so we're going to wait for it to finish
    let mut state = State::new(&window, &png).await?;
    let mut surface_configured = false;

    event_loop.run(move |event, control_flow| {
        match event {
            Event::WindowEvent {
                ref event,
                window_id,
            } if window_id == state.window().id() => {
                if !state.input(event) {
                    match event {
                        WindowEvent::CloseRequested
                        | WindowEvent::KeyboardInput {
                            event:
                                KeyEvent {
                                    state: ElementState::Pressed,
                                    physical_key: PhysicalKey::Code(KeyCode::Escape),
                                    ..
                                },
                            ..
                        } => control_flow.exit(),
                        WindowEvent::Resized(physical_size) => {
                            surface_configured = true;
                            state.resize(*physical_size);
                        }
                        WindowEvent::RedrawRequested => {
                            // This tells winit that we want another frame after this one
                            state.window().request_redraw();

                            if !surface_configured {
                                return;
                            }

                            state.update();
                            match state.render() {
                                Ok(_) => {}
                                // Reconfigure the surface if it's lost or outdated
                                Err(SurfaceError::Lost | SurfaceError::Outdated) => {
                                    state.resize(state.size)
                                }
                                // The system is out of memory, we should probably quit
                                Err(SurfaceError::OutOfMemory) => {
                                    log::error!("OutOfMemory");
                                    control_flow.exit();
                                }

                                // This happens when a frame takes too long to present
                                Err(SurfaceError::Timeout) => {
                                    log::warn!("Surface timeout")
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    })?;

    Ok(())
}
