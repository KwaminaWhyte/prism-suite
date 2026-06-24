//! GPU canvas: per-layer Rgba16Float linear-premultiplied textures, a ping-pong
//! compositor (blend modes in-shader), instanced brush-dab painting, and a
//! display pass that composites over a checkerboard and sRGB-encodes for egui.
//! Phase 1 (PLAN.md §4). Tiling/atlas is the next refinement — layers are
//! currently one canvas-sized texture each (a degenerate single tile).

use std::collections::HashMap;

use prism_core::{LayerId, Size};

// Re-export the wgpu version the engine is built against so hosts (the egui app,
// the gpui host, tests) construct devices/types in lockstep with the engine.
pub use wgpu;

mod api;
mod gpu_util;

// Re-export the public value types and the crate-internal GPU helpers/consts at
// the crate root, so `prism_canvas::Dab` etc. still resolve for hosts and the
// sibling modules' `use super::*` keeps finding the moved internal items.
pub use api::{Dab, LayerDraw, SelectionOp, SmartPass, ViewTransform};
pub(crate) use gpu_util::*;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct DisplayUniform {
    clip_min: [f32; 2],
    clip_max: [f32; 2],
    checker_px: f32,
    has_selection: f32,
    time: f32,
    canvas_w: f32,
    canvas_h: f32,
    _pad: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct FilterParams {
    kind: u32,
    _p: [u32; 3],
    texel: [f32; 2],
    dir: [f32; 2],
    amount: f32,
    radius: f32,
    center: [f32; 2],
    // Camera Raw (kind 32) overflow payload: the eleven develop controls packed
    // as three vec4s (slot 11 reserved). All zero for every other kind — and an
    // all-zero payload is an exact no-op — so it is harmless padding elsewhere.
    cr: [[f32; 4]; 3],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ShapeUniform {
    rect: [f32; 4],
    size: [f32; 2],
    kind: u32,
    _p: u32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CompositeParams {
    opacity: f32,
    blend_mode: u32,
    has_xform: u32,
    adjust_kind: u32,
    m: [f32; 4],
    off: [f32; 2],
    _p1: [f32; 2],
    adjust: [f32; 4],
    has_blend_if: u32,
    has_clip: u32,   // clip this layer to the layer below's alpha
    has_stroke: u32, // outer-stroke layer style
    stroke_w: f32,   // stroke half-width in uv units
    /// Blend-If luma ranges: [this_black, this_white, under_black, under_white].
    blend_if: [f32; 4],
    /// Stroke color (straight, premultiplied at use): [r, g, b, a].
    stroke_color: [f32; 4],
    has_shadow: u32,         // drop-shadow layer style
    shadow_blur: f32,        // shadow softness radius, uv units
    shadow_off: [f32; 2],    // shadow offset, uv units
    shadow_color: [f32; 4],  // straight rgba
    has_overlay: u32,             // color-overlay layer style
    has_inner_shadow: u32,        // inner-shadow layer style
    has_outer_glow: u32,          // outer-glow layer style
    has_inner_glow: u32,          // inner-glow layer style
    overlay_color: [f32; 4],      // straight rgba (a = strength)
    inner_shadow_color: [f32; 4], // straight rgba
    inner_shadow_off: [f32; 2],   // uv units
    inner_shadow_blur: f32,       // uv units
    outer_glow_size: f32,         // uv units
    outer_glow_color: [f32; 4],   // straight rgba
    inner_glow_color: [f32; 4],   // straight rgba
    inner_glow_size: f32,         // uv units
    has_grad_overlay: u32,        // gradient-overlay layer style
    grad_angle: f32,              // radians
    grad_opacity: f32,            // 0..1
    grad_color0: [f32; 4],        // straight rgb (a unused)
    grad_color1: [f32; 4],        // straight rgb (a unused)
    has_bevel: u32,               // bevel-&-emboss layer style (Inner Bevel)
    bevel_size: f32,              // edge width, uv units
    bevel_soften: f32,            // extra normal-field blur, uv units
    _pb: f32,
    bevel_light: [f32; 4],        // light direction (xyz unit vector; w unused)
    bevel_highlight: [f32; 4],    // straight rgba (a = opacity)
    bevel_shadow: [f32; 4],       // straight rgba (a = opacity)
    // Channel-Mixer matrix (adjust_kind 14): per-output [from_r, from_g, from_b, const].
    mix_r: [f32; 4],
    mix_g: [f32; 4],
    mix_b: [f32; 4],
}

impl CompositeParams {
    fn plain(opacity: f32, blend_mode: u32) -> Self {
        Self {
            opacity,
            blend_mode,
            has_xform: 0,
            adjust_kind: 0,
            m: [1.0, 0.0, 0.0, 1.0],
            off: [0.0; 2],
            _p1: [0.0; 2],
            adjust: [0.0; 4],
            has_blend_if: 0,
            has_clip: 0,
            has_stroke: 0,
            stroke_w: 0.0,
            blend_if: [0.0, 1.0, 0.0, 1.0],
            stroke_color: [0.0; 4],
            has_shadow: 0,
            shadow_blur: 0.0,
            shadow_off: [0.0; 2],
            shadow_color: [0.0; 4],
            has_overlay: 0,
            has_inner_shadow: 0,
            has_outer_glow: 0,
            has_inner_glow: 0,
            overlay_color: [0.0; 4],
            inner_shadow_color: [0.0; 4],
            inner_shadow_off: [0.0; 2],
            inner_shadow_blur: 0.0,
            outer_glow_size: 0.0,
            outer_glow_color: [0.0; 4],
            inner_glow_color: [0.0; 4],
            inner_glow_size: 0.0,
            has_grad_overlay: 0,
            grad_angle: 0.0,
            grad_opacity: 0.0,
            grad_color0: [0.0; 4],
            grad_color1: [0.0; 4],
            has_bevel: 0,
            bevel_size: 0.0,
            bevel_soften: 0.0,
            _pb: 0.0,
            bevel_light: [0.0; 4],
            bevel_highlight: [0.0; 4],
            bevel_shadow: [0.0; 4],
            mix_r: [1.0, 0.0, 0.0, 0.0],
            mix_g: [0.0, 1.0, 0.0, 0.0],
            mix_b: [0.0, 0.0, 1.0, 0.0],
        }
    }
}

// The compositor binds CompositeParams via dynamic-offset slots spaced by
// PARAMS_STRIDE, so the struct must fit within one slot. It must also be a
// multiple of 16 bytes to match the WGSL std140 uniform layout exactly. Both
// are compile-time invariants.
const _: () = {
    assert!(std::mem::size_of::<CompositeParams>() as u64 <= PARAMS_STRIDE);
    assert!(std::mem::size_of::<CompositeParams>().is_multiple_of(16));
};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct LayerInfo {
    size: [f32; 2],
    has_selection: f32,
    _pad: f32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CloneParams {
    /// destAnchor − sourceAnchor in document px (aligned Clone Stamp).
    offset: [f32; 2],
    _pad: [f32; 2],
}

/// One GPU texture + its default view. The engine's per-layer storage unit; also
/// the type of the ping/pong/selection/mask scratch targets.
pub struct GpuLayer {
    pub tex: wgpu::Texture,
    pub view: wgpu::TextureView,
}

/// A region pixel snapshot held GPU-side for one undo step. `rect` is the
/// `[x, y, w, h]` sub-region of the layer the snapshot covers (region-COW: a
/// stroke only snapshots the tiles/area it touched, not the whole layer).
struct Snapshot {
    id: LayerId,
    tex: wgpu::Texture,
    rect: [u32; 4],
    label: String,
}

/// All long-lived canvas GPU state, stored as a single egui callback resource.
pub struct CanvasGpu {
    sampler: wgpu::Sampler,

    // Compositor.
    composite_pipeline: wgpu::RenderPipeline,
    composite_bgl: wgpu::BindGroupLayout,
    params_buf: wgpu::Buffer,

    // Brush dabs.
    dab_pipeline: wgpu::RenderPipeline,
    dab_erase_pipeline: wgpu::RenderPipeline,
    dab_bgl: wgpu::BindGroupLayout,
    dab_info_buf: wgpu::Buffer,
    dab_instances: wgpu::Buffer,
    dab_capacity: u64,

    // Clone Stamp: a clone dab pass samples a frozen source snapshot.
    clone_pipeline: wgpu::RenderPipeline,
    clone_bgl: wgpu::BindGroupLayout,
    clone_params_buf: wgpu::Buffer,
    /// Pre-stroke source snapshot for the active clone stroke (sampleable).
    clone_src: Option<GpuLayer>,

    // Undo/redo: region snapshots. A stroke copies its layer to `stroke_pre` at
    // start, then on commit extracts only the dirty sub-rect into the stack.
    undo_stack: Vec<Snapshot>,
    redo_stack: Vec<Snapshot>,
    stroke_pre: Option<wgpu::Texture>,
    stroke_owner: Option<LayerId>,
    stroke_label: String,

    // Display.
    display_pipeline: wgpu::RenderPipeline,
    display_bgl: wgpu::BindGroupLayout,
    display_uniform: wgpu::Buffer,
    display_bind_group: Option<wgpu::BindGroup>,

    // Document state mirror.
    layers: HashMap<LayerId, GpuLayer>,
    /// Per-layer **smart-filter source** textures: the un-filtered pixels a layer
    /// had when its smart-filter stack was created. The displayed `layers[id]`
    /// texture is this source with the layer's enabled smart filters re-applied
    /// in order (see `reapply_smart_filters`), so the stack stays non-destructive
    /// — toggling/removing a filter restores the source. Present only while a
    /// layer carries a non-empty smart-filter stack.
    smart_sources: HashMap<LayerId, GpuLayer>,
    /// Per-Curves-layer 256×1 LUT textures (rgba = r/g/b/master tone curves).
    curve_luts: HashMap<LayerId, GpuLayer>,
    /// Identity LUT bound to composite passes that have no Curves layer (the
    /// shader only samples it when `adjust_kind == 8`, so it must still be valid).
    identity_lut: Option<GpuLayer>,
    canvas_size: Size,
    ping: Option<GpuLayer>,
    pong: Option<GpuLayer>,

    // In-progress brush stroke ("wet ink") composited over its owner layer and
    // flattened on pen-up — gives correct per-stroke opacity (RESEARCH.md §4).
    wet: Option<GpuLayer>,
    wet_owner: Option<LayerId>,
    wet_opacity: f32,

    // Frame-level dirty tracking: skip recompositing when the document is
    // unchanged (pan/zoom only touch the display pass). Per-tile region
    // invalidation needs the tile model (deferred).
    last_final_is_ping: bool,
    composite_valid: bool,

    // Selection (R16Float mask; 1 = selected). Marquee + invert pipelines.
    selection: Option<GpuLayer>,
    selection_tmp: Option<GpuLayer>,
    has_selection: bool,
    /// Saved selections (alpha channels): name → R16Float mask, canvas-sized.
    channels: Vec<(String, GpuLayer)>,
    shape_pipeline: wgpu::RenderPipeline,
    shape_bgl: wgpu::BindGroupLayout,
    shape_uniform: wgpu::Buffer,
    invert_pipeline: wgpu::RenderPipeline,
    invert_bgl: wgpu::BindGroupLayout,

    // Live move/transform of the active layer (uv-space layer-from-canvas affine).
    xform_layer: Option<LayerId>,
    xform_m: [f32; 4],
    xform_off: [f32; 2],

    // Per-layer masks (Rgba16Float; .r multiplies layer alpha). 1x1 white fallback.
    masks: HashMap<LayerId, GpuLayer>,
    white_mask: GpuLayer,
    white_written: bool,

    // Destructive filters.
    filter_pipeline: wgpu::RenderPipeline,
    filter_bgl: wgpu::BindGroupLayout,
    filter_uniform: wgpu::Buffer,
}

impl CanvasGpu {
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        let composite_mod = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("composite.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/composite.wgsl").into()),
        });
        let dab_mod = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("dab.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/dab.wgsl").into()),
        });
        let display_mod = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("display.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/display.wgsl").into()),
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("canvas.sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        // --- Compositor pipeline ---
        let composite_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("composite.bgl"),
            entries: &[
                sampler_entry(0),
                tex_entry(1),
                tex_entry(2),
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(
                            std::mem::size_of::<CompositeParams>() as u64,
                        ),
                    },
                    count: None,
                },
                tex_entry(4),
                tex_entry(5),
                tex_entry(6),
            ],
        });
        let composite_pipeline = make_render_pipeline(
            device,
            "composite",
            &composite_mod,
            "fs_main",
            &[&composite_bgl],
            &[],
            FMT,
            None, // we overwrite every pixel (read backdrop via sampler)
        );
        let params_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("composite.params"),
            size: PARAMS_STRIDE * MAX_LAYERS,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // --- Dab pipeline ---
        let dab_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("dab.bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                sampler_entry(1),
                tex_entry(2),
            ],
        });
        const DAB_ATTRS: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![
            0 => Float32x2, 1 => Float32, 2 => Float32, 3 => Float32x4
        ];
        let dab_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Dab>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &DAB_ATTRS,
        };
        let dab_pipeline = make_render_pipeline(
            device,
            "dab",
            &dab_mod,
            "fs_main",
            &[&dab_bgl],
            std::slice::from_ref(&dab_layout),
            FMT,
            Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
        );
        let dab_erase_pipeline = make_render_pipeline(
            device,
            "dab.erase",
            &dab_mod,
            "fs_main",
            &[&dab_bgl],
            std::slice::from_ref(&dab_layout),
            FMT,
            Some(ERASE_BLEND),
        );
        let dab_info_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("dab.info"),
            size: std::mem::size_of::<LayerInfo>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let dab_capacity = 1024;
        let dab_instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("dab.instances"),
            size: dab_capacity * std::mem::size_of::<Dab>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // --- Clone-stamp pipeline (samples a frozen source + offset) ---
        let clone_mod = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("clone.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/clone.wgsl").into()),
        });
        let clone_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("clone.bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                sampler_entry(1),
                tex_entry(2),
                tex_entry(3),
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let clone_pipeline = make_render_pipeline(
            device,
            "clone",
            &clone_mod,
            "fs_main",
            &[&clone_bgl],
            std::slice::from_ref(&dab_layout),
            FMT,
            Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
        );
        let clone_params_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("clone.params"),
            size: std::mem::size_of::<CloneParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // --- Display pipeline ---
        let display_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("display.bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                sampler_entry(1),
                tex_entry(2),
                tex_entry(3),
            ],
        });
        let display_pipeline = make_render_pipeline(
            device,
            "display",
            &display_mod,
            "fs_main",
            &[&display_bgl],
            &[],
            target_format,
            None,
        );

        // --- Selection pipelines ---
        let selection_mod = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("selection.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/selection.wgsl").into()),
        });
        let shape_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sel.shape.bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let shape_pipeline = make_render_pipeline(
            device,
            "sel.shape",
            &selection_mod,
            "fs_shape",
            &[&shape_bgl],
            &[],
            SEL_FMT,
            None,
        );
        let invert_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sel.invert.bgl"),
            entries: &[sampler_entry(1), tex_entry(2)],
        });
        let invert_pipeline = make_render_pipeline(
            device,
            "sel.invert",
            &selection_mod,
            "fs_invert",
            &[&invert_bgl],
            &[],
            SEL_FMT,
            None,
        );
        let shape_uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sel.shape.uniform"),
            size: std::mem::size_of::<ShapeUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // --- Filter pipeline ---
        let filter_mod = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("filter.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/filter.wgsl").into()),
        });
        let filter_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("filter.bgl"),
            entries: &[
                sampler_entry(0),
                tex_entry(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Secondary input texture: for single-input kinds it is aliased
                // to binding 1, so only High Pass (kind 24) reads it separately.
                tex_entry(3),
            ],
        });
        let filter_pipeline = make_render_pipeline(
            device,
            "filter",
            &filter_mod,
            "fs_main",
            &[&filter_bgl],
            &[],
            FMT,
            None,
        );
        let filter_uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("filter.uniform"),
            size: std::mem::size_of::<FilterParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let display_uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("display.uniform"),
            size: std::mem::size_of::<DisplayUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            sampler,
            composite_pipeline,
            composite_bgl,
            params_buf,
            dab_pipeline,
            dab_erase_pipeline,
            dab_bgl,
            dab_info_buf,
            dab_instances,
            clone_pipeline,
            clone_bgl,
            clone_params_buf,
            clone_src: None,
            dab_capacity,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            stroke_pre: None,
            stroke_owner: None,
            stroke_label: String::new(),
            display_pipeline,
            display_bgl,
            display_uniform,
            display_bind_group: None,
            layers: HashMap::new(),
            smart_sources: HashMap::new(),
            curve_luts: HashMap::new(),
            identity_lut: None,
            canvas_size: Size::new(1, 1),
            ping: None,
            pong: None,
            wet: None,
            wet_owner: None,
            wet_opacity: 1.0,
            last_final_is_ping: true,
            composite_valid: false,
            selection: None,
            selection_tmp: None,
            has_selection: false,
            channels: Vec::new(),
            shape_pipeline,
            shape_bgl,
            shape_uniform,
            invert_pipeline,
            invert_bgl,
            xform_layer: None,
            xform_m: [1.0, 0.0, 0.0, 1.0],
            xform_off: [0.0; 2],
            masks: HashMap::new(),
            white_mask: make_target_fmt(device, Size::new(1, 1), "white.mask", FMT),
            white_written: false,
            filter_pipeline,
            filter_bgl,
            filter_uniform,
        }
    }

    // --- Display-glue accessors -------------------------------------------
    // The egui callback (`CanvasPaint`, host-side) drives compositing and then
    // issues the display draw. These keep the engine's GPU fields private while
    // still letting the host run the frame.

    /// Whether the cached composite is still valid (no document change since).
    pub fn composite_valid(&self) -> bool {
        self.composite_valid
    }

    /// Which ping/pong buffer holds the last completed composite.
    pub fn last_final_is_ping(&self) -> bool {
        self.last_final_is_ping
    }

    /// Record the result of a composite pass so the next frame can reuse it.
    pub fn set_composite_state(&mut self, valid: bool, last_final_is_ping: bool) {
        self.composite_valid = valid;
        self.last_final_is_ping = last_final_is_ping;
    }

    /// Write the per-frame display-pass uniform. `clip_min`/`clip_max` are the
    /// document rect in clip space; the engine fills in `has_selection` itself.
    #[allow(clippy::too_many_arguments)]
    pub fn write_display_uniform(
        &self,
        queue: &wgpu::Queue,
        clip_min: [f32; 2],
        clip_max: [f32; 2],
        checker_px: f32,
        time: f32,
        canvas_w: f32,
        canvas_h: f32,
    ) {
        let uni = DisplayUniform {
            clip_min,
            clip_max,
            checker_px,
            has_selection: if self.has_selection() { 1.0 } else { 0.0 },
            time,
            canvas_w,
            canvas_h,
            _pad: [0.0; 3],
        };
        queue.write_buffer(&self.display_uniform, 0, bytemuck::bytes_of(&uni));
    }

    /// Issue the full-screen display draw into the host's render pass. No-op
    /// until `build_display_bind_group` has run for this frame.
    pub fn issue_display_draw(&self, render_pass: &mut wgpu::RenderPass<'_>) {
        let Some(bg) = &self.display_bind_group else {
            return;
        };
        render_pass.set_pipeline(&self.display_pipeline);
        render_pass.set_bind_group(0, bg, &[]);
        render_pass.draw(0..6, 0..1);
    }
}

mod brush;
mod channels;
mod clone;
mod command;
mod compositor;
mod layers;
mod selection;

// CPU reference math for the blur-family filters — compiled only for tests
// (the live path is the GPU shader pass in `compositor`).
#[cfg(test)]
mod filter_math;

#[cfg(test)]
mod gpu_tests;
