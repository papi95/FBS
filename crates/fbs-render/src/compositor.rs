//! Scene compositing and transitions.
//!
//! One full-screen triangle per layer, no vertex buffers, so the cost per frame
//! is one draw call per visible source plus one draw for the transition.
//!
//! Blend strategy:
//! * Normal and Add use fixed-function GPU blending - the fast path, no copies.
//! * Multiply and Screen need the destination colour, which a fragment shader
//!   cannot read from its own attachment. Those two use a scratch layer plus a
//!   combine pass. They are rare in live work, so paying for a copy only when a
//!   scene actually uses them keeps the common case cheap on an HD 5500.


use fbs_core::ids::SourceId;
use fbs_core::source::{BlendMode, Rect};

use crate::gpu::Gpu;
use crate::sources::SourceTextures;
use crate::TransitionPass;

/// Shader-side blend selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendModeGpu {
    Normal,
    Add,
    Multiply,
    Screen,
}

impl BlendModeGpu {
    pub fn from_core(mode: BlendMode) -> Self {
        match mode {
            BlendMode::Normal => Self::Normal,
            BlendMode::Add => Self::Add,
            BlendMode::Multiply => Self::Multiply,
            BlendMode::Screen => Self::Screen,
        }
    }

    /// Multiply and Screen cannot be expressed as a fixed-function blend.
    fn needs_readback(self) -> bool {
        matches!(self, Self::Multiply | Self::Screen)
    }

    fn combine_id(self) -> f32 {
        match self {
            Self::Multiply => 2.0,
            // Screen shares the readback path; Add does not.
            Self::Screen => 3.0,
            _ => 0.0,
        }
    }
}

/// A render target plus its sampling view.
pub struct RenderTarget {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub width: u32,
    pub height: u32,
}

impl RenderTarget {
    fn new(gpu: &Gpu, width: u32, height: u32, label: &str) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self { texture, view, width, height }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct LayerUniform {
    rect: [f32; 4],
    crop: [f32; 4],
    opacity: f32,
    blend: f32,
    rotation: f32,
    pad: f32,
    target_px: [f32; 2],
    pad2: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct TransitionUniform {
    progress: f32,
    kind: f32,
    target_w: f32,
    target_h: f32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CombineUniform {
    mode: f32,
    // WGSL `vec3<f32>` is 16-byte aligned, so the struct is 32 bytes, not 16.
    // Getting this wrong makes wgpu reject the draw at validation time.
    pad: [f32; 3],
    pad2: [f32; 4],
}

/// Shared vertex stage and the layer fragment stage.
const LAYER_WGSL: &str = r#"
struct Uniforms {
    rect: vec4<f32>,
    crop: vec4<f32>,
    opacity: f32,
    blend: f32,
    rotation: f32,
    pad: f32,
    target_px: vec2<f32>,
    pad2: vec2<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var tex: texture_2d<f32>;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VsOut {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var out: VsOut;
    out.pos = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
    out.uv = uv;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Rotate about the layer centre, then reject anything outside its rect.
    var p = in.uv - vec2<f32>(u.rect.x + u.rect.z * 0.5, u.rect.y + u.rect.w * 0.5);
    let c = cos(-u.rotation);
    let s = sin(-u.rotation);
    p = vec2<f32>(p.x * c - p.y * s, p.x * s + p.y * c);
    let q = p + vec2<f32>(u.rect.z * 0.5, u.rect.w * 0.5);

    if (q.x < 0.0 || q.y < 0.0 || q.x > u.rect.z || q.y > u.rect.w) {
        discard;
    }

    var uv = vec2<f32>(q.x / max(u.rect.z, 1e-5), q.y / max(u.rect.w, 1e-5));
    uv = vec2<f32>(
        mix(u.crop.x, 1.0 - u.crop.z, uv.x),
        mix(u.crop.y, 1.0 - u.crop.w, uv.y)
    );

    let src = textureSample(tex, samp, uv);
    return vec4<f32>(src.rgb * src.a, src.a) * u.opacity;
}
"#;

const COMBINE_WGSL: &str = r#"
struct Uniforms {
    mode: f32,
    pad: vec3<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var dst_tex: texture_2d<f32>;
@group(0) @binding(3) var src_tex: texture_2d<f32>;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VsOut {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var out: VsOut;
    out.pos = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
    out.uv = uv;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let dst = textureSample(dst_tex, samp, in.uv);
    let src = textureSample(src_tex, samp, in.uv);

    var blended = src.rgb;
    if (u.mode < 2.5) {
        blended = dst.rgb * src.rgb;
    } else {
        blended = 1.0 - (1.0 - dst.rgb) * (1.0 - src.rgb);
    }
    // src.rgb arrives premultiplied by alpha from the layer pass.
    let rgb = mix(dst.rgb, blended, src.a);
    return vec4<f32>(rgb, 1.0);
}
"#;

const TRANSITION_WGSL: &str = r#"
struct Uniforms {
    progress: f32,
    kind: f32,
    target_w: f32,
    target_h: f32,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var from_tex: texture_2d<f32>;
@group(0) @binding(3) var to_tex: texture_2d<f32>;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VsOut {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var out: VsOut;
    out.pos = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
    out.uv = uv;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let p = clamp(u.progress, 0.0, 1.0);
    let from_c = textureSample(from_tex, samp, in.uv);
    let to_c = textureSample(to_tex, samp, in.uv);

    // kind: 0 fade, 1 slide left, 2 wipe down, 3 stinger (fade until wired up)
    if (u.kind < 0.5 || u.kind > 2.5) {
        return mix(from_c, to_c, p);
    }
    if (u.kind < 1.5) {
        if (in.uv.x >= p) {
            return textureSample(to_tex, samp, vec2<f32>(in.uv.x - p, in.uv.y));
        }
        return textureSample(from_tex, samp, vec2<f32>(in.uv.x + (1.0 - p), in.uv.y));
    }
    if (in.uv.y >= p) {
        return to_c;
    }
    return from_c;
}
"#;

/// A reusable draw slot: uniform buffer plus the bind group pointing at it.
/// Reusing slots each frame means no allocation churn during a show.
struct LayerSlot {
    uniform: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    /// What the bind group currently points at.
    key: Option<(SourceId, u64, u32, u32)>,
}

pub struct Compositor {
    gpu: Gpu,
    layer_pipeline: wgpu::RenderPipeline,
    layer_bgl: wgpu::BindGroupLayout,
    combine_pipeline: wgpu::RenderPipeline,
    combine_bgl: wgpu::BindGroupLayout,
    transition_pipeline: wgpu::RenderPipeline,
    transition_bgl: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    slots: Vec<LayerSlot>,
    scratch: Option<RenderTarget>,
    saved: Option<RenderTarget>,
}

impl Compositor {
    pub fn new(gpu: Gpu) -> Self {
        let device = &gpu.device;

        let layer_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fbs-layer"),
            source: wgpu::ShaderSource::Wgsl(LAYER_WGSL.into()),
        });

        let layer_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("fbs-layer-bgl"),
            entries: &[
                uniform_entry(0),
                sampler_entry(1),
                texture_entry(2),
            ],
        });

        let layer_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("fbs-layer-layout"),
            bind_group_layouts: &[&layer_bgl],
            push_constant_ranges: &[],
        });

        let layer_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("fbs-layer-pipeline"),
            layout: Some(&layer_layout),
            vertex: wgpu::VertexState {
                module: &layer_shader,
                entry_point: "vs_main",
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &layer_shader,
                entry_point: "fs_main",
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    // Premultiplied source-alpha blending.
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
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
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let combine_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fbs-combine"),
            source: wgpu::ShaderSource::Wgsl(COMBINE_WGSL.into()),
        });

        let combine_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("fbs-combine-bgl"),
            entries: &[
                uniform_entry(0),
                sampler_entry(1),
                texture_entry(2),
                texture_entry(3),
            ],
        });

        let combine_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("fbs-combine-layout"),
            bind_group_layouts: &[&combine_bgl],
            push_constant_ranges: &[],
        });

        let combine_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("fbs-combine-pipeline"),
            layout: Some(&combine_layout),
            vertex: wgpu::VertexState {
                module: &combine_shader,
                entry_point: "vs_main",
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &combine_shader,
                entry_point: "fs_main",
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let transition_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fbs-transition"),
            source: wgpu::ShaderSource::Wgsl(TRANSITION_WGSL.into()),
        });

        let transition_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("fbs-transition-bgl"),
            entries: &[
                uniform_entry(0),
                sampler_entry(1),
                texture_entry(2),
                texture_entry(3),
            ],
        });

        let transition_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("fbs-transition-layout"),
            bind_group_layouts: &[&transition_bgl],
            push_constant_ranges: &[],
        });

        let transition_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("fbs-transition-pipeline"),
            layout: Some(&transition_layout),
            vertex: wgpu::VertexState {
                module: &transition_shader,
                entry_point: "vs_main",
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &transition_shader,
                entry_point: "fs_main",
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("fbs-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        Self {
            gpu,
            layer_pipeline,
            layer_bgl,
            combine_pipeline,
            combine_bgl,
            transition_pipeline,
            transition_bgl,
            sampler,
            slots: Vec::new(),
            scratch: None,
            saved: None,
        }
    }

    pub fn gpu(&self) -> &Gpu {
        &self.gpu
    }

    pub fn report(&self) -> &crate::AdapterReport {
        &self.gpu.report
    }

    pub fn make_target(&self, width: u32, height: u32) -> RenderTarget {
        RenderTarget::new(&self.gpu, width, height, "fbs-target")
    }

    /// Composite one scene into `target`, back to front.
    pub fn render_scene(
        &mut self,
        target: &RenderTarget,
        scene: &fbs_core::scene::Scene,
        textures: &SourceTextures,
        clear: [f64; 4],
    ) {
        let mut encoder = self
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("fbs-scene") });

        // Collect the fast-path layers first so the borrow checker is happy
        // while the pass is open.
        let fast: Vec<(usize, SourceId, u64)> = scene
            .draw_list()
            .filter(|s| !BlendModeGpu::from_core(s.blend).needs_readback())
            .filter(|s| textures.get(s.id).is_some())
            .enumerate()
            .map(|(i, s)| (i, s.id, textures.generation(s.id)))
            .collect();

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("fbs-composite"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: clear[0],
                            g: clear[1],
                            b: clear[2],
                            a: clear[3],
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            let sw = scene.width.max(1) as f32;
            let sh = scene.height.max(1) as f32;

            for (slot_index, id, gen) in fast {
                let Some(src) = scene.get(id) else { continue };
                let Some(view) = textures.get(id) else { continue };
                let slot_index =
                    self.slot_for(slot_index, id, gen, view, target.width, target.height);
                let r: Rect = src.transform.rect;
                let u = LayerUniform {
                    rect: [r.x / sw, r.y / sh, r.w / sw, r.h / sh],
                    crop: src.transform.crop,
                    opacity: src.opacity.clamp(0.0, 1.0),
                    blend: 0.0,
                    rotation: src.transform.rotation_deg.to_radians(),
                    pad: 0.0,
                    target_px: [target.width as f32, target.height as f32],
                    pad2: [0.0, 0.0],
                };
                self.gpu
                    .queue
                    .write_buffer(&self.slots[slot_index].uniform, 0, bytemuck::bytes_of(&u));
                pass.set_pipeline(&self.layer_pipeline);
                pass.set_bind_group(0, &self.slots[slot_index].bind_group, &[]);
                pass.draw(0..3, 0..1);
            }
        }
        self.gpu.queue.submit(Some(encoder.finish()));

        // Readback blends in a second pass, preserving stack order.
        let deferred: Vec<(SourceId, BlendModeGpu)> = scene
            .draw_list()
            .filter(|s| BlendModeGpu::from_core(s.blend).needs_readback())
            .map(|s| (s.id, BlendModeGpu::from_core(s.blend)))
            .collect();

        for (id, mode) in deferred {
            self.composite_readback_layer(target, scene, textures, id, mode);
        }
    }

    /// Multiply/Screen: draw the layer alone into a scratch target, snapshot the
    /// backdrop, then combine the two full-screen. Correct, and only paid for when
    /// a scene actually uses those modes.
    fn composite_readback_layer(
        &mut self,
        target: &RenderTarget,
        scene: &fbs_core::scene::Scene,
        textures: &SourceTextures,
        id: SourceId,
        mode: BlendModeGpu,
    ) {
        let Some(src) = scene.get(id) else {
            return;
        };
        let Some(view) = textures.get(id) else {
            return;
        };

        // Take the scratch targets out so they can be borrowed mutably while the
        // render passes are open; they go straight back at the end.
        let need_new = self
            .scratch
            .as_ref()
            .map(|s| (s.width, s.height) != (target.width, target.height))
            .unwrap_or(true);
        if need_new {
            self.scratch = Some(RenderTarget::new(&self.gpu, target.width, target.height, "fbs-scratch"));
            self.saved = Some(RenderTarget::new(&self.gpu, target.width, target.height, "fbs-saved"));
        }
        let scratch = self.scratch.take().expect("scratch target");
        let saved = self.saved.take().expect("saved target");

        // Slot 0 is reserved for the combine uniform below, so the readback
        // layer gets its own slot and never rebinds slot 0's uniform.
        let gen = textures.generation(id);
        let slot_index = self.slot_for(1, id, gen, view, target.width, target.height);
        let sw = scene.width.max(1) as f32;
        let sh = scene.height.max(1) as f32;
        let r = src.transform.rect;
        let u = LayerUniform {
            rect: [r.x / sw, r.y / sh, r.w / sw, r.h / sh],
            crop: src.transform.crop,
            opacity: src.opacity.clamp(0.0, 1.0),
            blend: 0.0,
            rotation: src.transform.rotation_deg.to_radians(),
            pad: 0.0,
            target_px: [target.width as f32, target.height as f32],
            pad2: [0.0, 0.0],
        };
        self.gpu
            .queue
            .write_buffer(&self.slots[slot_index].uniform, 0, bytemuck::bytes_of(&u));

        let combine_uniform = self.gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("fbs-combine-uniform"),
            size: std::mem::size_of::<CombineUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let cu = CombineUniform { mode: mode.combine_id(), pad: [0.0; 3], pad2: [0.0; 4] };
        self.gpu
            .queue
            .write_buffer(&combine_uniform, 0, bytemuck::bytes_of(&cu));

        let combine_bind = self.gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("fbs-combine-bind"),
            layout: &self.combine_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: combine_uniform.as_entire_binding() },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&saved.view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&scratch.view),
                },
            ],
        });

        let mut encoder = self
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("fbs-blend") });

        // 1. Layer alone on a transparent background.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("fbs-scratch"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &scratch.view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.layer_pipeline);
            pass.set_bind_group(0, &self.slots[slot_index].bind_group, &[]);
            pass.draw(0..3, 0..1);
        }

        // 2. Snapshot the backdrop so the combine pass can sample it.
        encoder.copy_texture_to_texture(
            wgpu::ImageCopyTexture {
                texture: &target.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyTexture {
                texture: &saved.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: target.width,
                height: target.height,
                depth_or_array_layers: 1,
            },
        );

        // 3. Combine into the target.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("fbs-combine"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.combine_pipeline);
            pass.set_bind_group(0, &combine_bind, &[]);
            pass.draw(0..3, 0..1);
        }

        self.gpu.queue.submit(Some(encoder.finish()));
        self.scratch = Some(scratch);
        self.saved = Some(saved);
    }

    /// Mix two bus targets into `out`.
    pub fn render_transition(
        &mut self,
        out: &RenderTarget,
        from: &RenderTarget,
        to: &RenderTarget,
        kind: TransitionPass,
        progress: f32,
    ) {
        if kind == TransitionPass::Cut || progress >= 1.0 {
            self.blit(out, to);
            return;
        }
        if progress <= 0.0 {
            self.blit(out, from);
            return;
        }

        let uniform = self.gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("fbs-transition-uniform"),
            size: std::mem::size_of::<TransitionUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let u = TransitionUniform {
            progress: progress.clamp(0.0, 1.0),
            kind: kind.shader_id(),
            target_w: out.width as f32,
            target_h: out.height as f32,
        };
        self.gpu.queue.write_buffer(&uniform, 0, bytemuck::bytes_of(&u));

        let bind = self.gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("fbs-transition-bind"),
            layout: &self.transition_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&from.view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&to.view),
                },
            ],
        });

        let mut encoder = self
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("fbs-transition") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("fbs-transition"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &out.view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.transition_pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.draw(0..3, 0..1);
        }
        self.gpu.queue.submit(Some(encoder.finish()));
    }

    /// Copy one target to another (used for cuts and preview mirroring).
    pub fn blit(&self, out: &RenderTarget, src: &RenderTarget) {
        let mut encoder = self
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("fbs-blit") });
        encoder.copy_texture_to_texture(
            wgpu::ImageCopyTexture {
                texture: &src.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyTexture {
                texture: &out.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: out.width.min(src.width),
                height: out.height.min(src.height),
                depth_or_array_layers: 1,
            },
        );
        self.gpu.queue.submit(Some(encoder.finish()));
    }

    /// Get (or build) the slot for layer `index`, pointing at the given texture.
    fn slot_for(
        &mut self,
        index: usize,
        id: SourceId,
        gen: u64,
        view: &wgpu::TextureView,
        width: u32,
        height: u32,
    ) -> usize {
        let key = (id, gen, width, height);

        while self.slots.len() <= index {
            let uniform = self.gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("fbs-layer-uniform"),
                size: std::mem::size_of::<LayerUniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            // Placeholder bind group so the slot is always valid.
            let bind_group = self.gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("fbs-layer-bind"),
                layout: &self.layer_bgl,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(view),
                    },
                ],
            });
            self.slots.push(LayerSlot { uniform, bind_group, key: None });
        }

        if self.slots[index].key != Some(key) {
            let bind_group =
                self.gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("fbs-layer-bind"),
                    layout: &self.layer_bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.slots[index].uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(view),
                    },
                ],
            });
            self.slots[index].bind_group = bind_group;
            self.slots[index].key = Some(key);
        }
        index
    }

    /// Read a target back to RGBA bytes. Tests and the still-frame export use this.
    pub fn read_target(&self, target: &RenderTarget) -> Vec<u8> {
        read_texture(&self.gpu, &target.texture, target.width, target.height)
            .unwrap_or_default()
    }

    /// Sample one pixel as RGBA, which makes assertions readable.
    pub fn pixel(&self, target: &RenderTarget, x: u32, y: u32) -> [u8; 4] {
        let data = self.read_target(target);
        let idx = ((y * target.width + x) * 4) as usize;
        if idx + 3 >= data.len() {
            return [0, 0, 0, 0];
        }
        [data[idx], data[idx + 1], data[idx + 2], data[idx + 3]]
    }
}

fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn sampler_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}

fn texture_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

/// Read an RGBA8 texture back to the CPU.
pub fn read_texture(
    gpu: &Gpu,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
) -> Option<Vec<u8>> {
    let padded_row = ((width * 4).div_ceil(256)) * 256;
    let buffer_size = (padded_row as u64) * (height as u64);

    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("fbs-readback"),
        size: buffer_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("fbs-readback") });
    encoder.copy_texture_to_buffer(
        wgpu::ImageCopyTexture {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::ImageCopyBuffer {
            buffer: &buffer,
            layout: wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(padded_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
    gpu.queue.submit(Some(encoder.finish()));

    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |res| {
        let _ = tx.send(res);
    });
    let _ = gpu.device.poll(wgpu::Maintain::Wait);
    match rx.recv() {
        Ok(Ok(())) => {}
        _ => return None,
    }

    let data = slice.get_mapped_range();
    let mut out = Vec::with_capacity((width * height * 4) as usize);
    for row in 0..height {
        let start = (row * padded_row) as usize;
        out.extend_from_slice(&data[start..start + (width * 4) as usize]);
    }
    drop(data);
    buffer.unmap();
    Some(out)
}
