//! GPU-plumbing helpers shared across the engine's modules: texture-format /
//! capacity constants, the erase blend state, render-target / LUT allocators,
//! bind-group-layout entry builders, texture copy helpers, and the render
//! pipeline builder. All `pub(crate)` — internal to the engine, re-exported into
//! the crate root so the sibling modules' `use super::*` keeps resolving them.

use prism_core::Size;

use crate::GpuLayer;

pub(crate) const FMT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
pub(crate) const SEL_FMT: wgpu::TextureFormat = wgpu::TextureFormat::R16Float;
/// Width of a Curves tone-curve LUT (256 input levels, 1px tall, Rgba16Float).
pub(crate) const LUT_W: u32 = 256;
pub(crate) const MAX_LAYERS: u64 = 64;
// Dynamic-offset alignment must be a multiple of 256; CompositeParams grew past
// 256 bytes once the full layer-style set landed (currently 352, after Bevel &
// Emboss), so the slot stride is 512. A compile-time assert guards this.
pub(crate) const PARAMS_STRIDE: u64 = 512;
pub(crate) const UNDO_MAX: usize = 32;
/// Params slot reserved for the wet stroke (last slot in the buffer).
pub(crate) const WET_PARAMS_OFFSET: u32 = ((MAX_LAYERS - 1) * PARAMS_STRIDE) as u32;

/// Erase blend: dst *= (1 - dab_alpha). Removes coverage.
pub(crate) const ERASE_BLEND: wgpu::BlendState = wgpu::BlendState {
    color: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::Zero,
        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
        operation: wgpu::BlendOperation::Add,
    },
    alpha: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::Zero,
        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
        operation: wgpu::BlendOperation::Add,
    },
};

pub(crate) fn make_target(device: &wgpu::Device, size: Size, label: &str) -> GpuLayer {
    make_target_fmt(device, size, label, FMT)
}

/// A 256×1 Rgba16Float tone-curve LUT target.
pub(crate) fn make_lut_target(device: &wgpu::Device, label: &str) -> GpuLayer {
    make_target_fmt(device, Size::new(LUT_W, 1), label, FMT)
}

/// Upload `LUT_W*4` f16 texels (interleaved rgba) into a LUT texture.
pub(crate) fn write_lut(queue: &wgpu::Queue, tex: &wgpu::Texture, texels: &[half::f16]) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        bytemuck::cast_slice(texels),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(LUT_W * 4 * 2), // rgba * 2 bytes (f16)
            rows_per_image: Some(1),
        },
        wgpu::Extent3d {
            width: LUT_W,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
}

pub(crate) fn make_target_fmt(
    device: &wgpu::Device,
    size: Size,
    label: &str,
    format: wgpu::TextureFormat,
) -> GpuLayer {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: size.width.max(1),
            height: size.height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    GpuLayer { tex, view }
}

pub(crate) fn sampler_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}

pub(crate) fn tex_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
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

/// GPU-side full-texture copy (same size/format).
pub(crate) fn copy_tex(
    encoder: &mut wgpu::CommandEncoder,
    src: &wgpu::Texture,
    dst: &wgpu::Texture,
    size: Size,
) {
    encoder.copy_texture_to_texture(
        wgpu::TexelCopyTextureInfo {
            texture: src,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyTextureInfo {
            texture: dst,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::Extent3d {
            width: size.width.max(1),
            height: size.height.max(1),
            depth_or_array_layers: 1,
        },
    );
}

/// GPU-side copy of a `[w,h]` region from `src@src_xy` to `dst@dst_xy`.
pub(crate) fn copy_region(
    encoder: &mut wgpu::CommandEncoder,
    src: &wgpu::Texture,
    src_xy: [u32; 2],
    dst: &wgpu::Texture,
    dst_xy: [u32; 2],
    wh: [u32; 2],
) {
    encoder.copy_texture_to_texture(
        wgpu::TexelCopyTextureInfo {
            texture: src,
            mip_level: 0,
            origin: wgpu::Origin3d {
                x: src_xy[0],
                y: src_xy[1],
                z: 0,
            },
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyTextureInfo {
            texture: dst,
            mip_level: 0,
            origin: wgpu::Origin3d {
                x: dst_xy[0],
                y: dst_xy[1],
                z: 0,
            },
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::Extent3d {
            width: wh[0].max(1),
            height: wh[1].max(1),
            depth_or_array_layers: 1,
        },
    );
}

/// A clear-to-transparent color attachment for `view` (helper for inline descriptors).
pub(crate) fn clear_attachment(view: &wgpu::TextureView) -> wgpu::RenderPassColorAttachment<'_> {
    wgpu::RenderPassColorAttachment {
        view,
        depth_slice: None,
        resolve_target: None,
        ops: wgpu::Operations {
            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
            store: wgpu::StoreOp::Store,
        },
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn make_render_pipeline(
    device: &wgpu::Device,
    label: &str,
    module: &wgpu::ShaderModule,
    fs_entry: &str,
    bind_group_layouts: &[&wgpu::BindGroupLayout],
    vertex_buffers: &[wgpu::VertexBufferLayout],
    format: wgpu::TextureFormat,
    blend: Option<wgpu::BlendState>,
) -> wgpu::RenderPipeline {
    let bgls: Vec<Option<&wgpu::BindGroupLayout>> =
        bind_group_layouts.iter().map(|b| Some(*b)).collect();
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: &bgls,
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("vs_main"),
            buffers: vertex_buffers,
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(fs_entry),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}
