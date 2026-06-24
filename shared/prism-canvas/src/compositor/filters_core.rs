//! Destructive + smart-filter GPU plumbing for [`CanvasGpu`]: the low-level
//! `filter_pass*` primitives that drive the `filter.wgsl` shader, the simple
//! [`CanvasGpu::apply_filter`] entry point, and the non-destructive smart-filter
//! source/restore/re-apply machinery. The per-filter `apply_*` methods live in
//! [`super::filters_apply`]; compositing lives in [`super::composite`].

use crate::*;

impl CanvasGpu {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn filter_pass(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        input: &wgpu::TextureView,
        output: &wgpu::TextureView,
        kind: u32,
        dir: [f32; 2],
        amount: f32,
        radius: f32,
    ) {
        self.filter_pass_c(
            device, queue, input, output, kind, dir, amount, radius, [0.0; 2],
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn filter_pass_c(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        input: &wgpu::TextureView,
        output: &wgpu::TextureView,
        kind: u32,
        dir: [f32; 2],
        amount: f32,
        radius: f32,
        center: [f32; 2],
    ) {
        // Single-input pass: alias the secondary texture to `input` (only the
        // High Pass combine, kind 24, needs a distinct secondary — see
        // `filter_pass_2`).
        self.filter_pass_2(
            device, queue, input, input, output, kind, dir, amount, radius, center,
        )
    }

    /// Like `filter_pass_c` but with a distinct secondary input texture bound at
    /// binding 3 (`orig` in the shader). Used by High Pass (kind 24) to subtract
    /// a Gaussian-blurred copy (`input`) from the untouched source (`orig`).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn filter_pass_2(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        input: &wgpu::TextureView,
        orig: &wgpu::TextureView,
        output: &wgpu::TextureView,
        kind: u32,
        dir: [f32; 2],
        amount: f32,
        radius: f32,
        center: [f32; 2],
    ) {
        self.filter_pass_cr(
            device,
            queue,
            input,
            orig,
            output,
            kind,
            dir,
            amount,
            radius,
            center,
            [[0.0; 4]; 3],
        )
    }

    /// Like `filter_pass_2` but also writes the Camera Raw `cr` overflow payload
    /// (shader kind 32). Every other path passes an all-zero `cr` (a no-op).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn filter_pass_cr(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        input: &wgpu::TextureView,
        orig: &wgpu::TextureView,
        output: &wgpu::TextureView,
        kind: u32,
        dir: [f32; 2],
        amount: f32,
        radius: f32,
        center: [f32; 2],
        cr: [[f32; 4]; 3],
    ) {
        let (w, h) = (
            self.canvas_size.width as f32,
            self.canvas_size.height as f32,
        );
        queue.write_buffer(
            &self.filter_uniform,
            0,
            bytemuck::bytes_of(&FilterParams {
                kind,
                _p: [0; 3],
                texel: [1.0 / w, 1.0 / h],
                dir,
                amount,
                radius,
                center,
                cr,
            }),
        );
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("filter.bg"),
            layout: &self.filter_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(input),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.filter_uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(orig),
                },
            ],
        });
        let mut enc = device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("filter.pass"),
                color_attachments: &[Some(clear_attachment(output))],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.filter_pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit([enc.finish()]);
    }

    /// Apply a destructive filter to a layer.
    /// kind: 1 Gaussian blur, 2 sharpen, 3 pixelate, 5 box blur. Gaussian and
    /// box blur are separable and run two passes (H then V); the rest run once.
    /// (Motion blur and radial blur take extra geometry — see their methods.)
    pub fn apply_filter(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        id: LayerId,
        kind: u32,
        radius: f32,
        amount: f32,
    ) {
        self.begin_command_now(device, queue, id, "Filter");
        let (Some(layer), Some(pong)) = (self.layers.get(&id), self.pong.as_ref()) else {
            return;
        };
        // Separable blurs (Gaussian=1, Box=5): horizontal then vertical pass.
        if kind == 1 || kind == 5 {
            let tx = [1.0 / self.canvas_size.width as f32, 0.0];
            let ty = [0.0, 1.0 / self.canvas_size.height as f32];
            self.filter_pass(
                device,
                queue,
                &layer.view,
                &pong.view,
                kind,
                tx,
                0.0,
                radius,
            );
            self.filter_pass(
                device,
                queue,
                &pong.view,
                &layer.view,
                kind,
                ty,
                0.0,
                radius,
            );
        } else {
            self.filter_pass(
                device,
                queue,
                &layer.view,
                &pong.view,
                kind,
                [0.0; 2],
                amount,
                radius,
            );
            let mut enc = device.create_command_encoder(&Default::default());
            copy_tex(&mut enc, &pong.tex, &layer.tex, self.canvas_size);
            queue.submit([enc.finish()]);
        }
    }

    /// Capture the layer's current pixels as its **smart-filter source** (the
    /// un-filtered baseline the stack re-applies from). Idempotent: if a source
    /// already exists for `id` it is left untouched, so re-applying an edited
    /// stack never re-snapshots an already-filtered result. Call this once, when
    /// the layer's first smart filter is added.
    pub fn ensure_smart_source(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, id: LayerId) {
        if self.smart_sources.contains_key(&id) {
            return;
        }
        let Some(layer) = self.layers.get(&id) else {
            return;
        };
        let src = make_target(device, self.canvas_size, "smart.source");
        let mut enc = device.create_command_encoder(&Default::default());
        copy_tex(&mut enc, &layer.tex, &src.tex, self.canvas_size);
        queue.submit([enc.finish()]);
        self.smart_sources.insert(id, src);
    }

    /// Whether layer `id` currently holds a smart-filter source snapshot.
    pub fn has_smart_source(&self, id: LayerId) -> bool {
        self.smart_sources.contains_key(&id)
    }

    /// Restore the layer's pixels from its smart-filter source and drop the
    /// source. Used when the last smart filter is removed (the layer goes back to
    /// being plain, editable pixels — the source becomes the live layer again).
    pub fn clear_smart_source(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        id: LayerId,
    ) {
        let (Some(src), Some(layer)) = (self.smart_sources.get(&id), self.layers.get(&id)) else {
            self.smart_sources.remove(&id);
            return;
        };
        let mut enc = device.create_command_encoder(&Default::default());
        copy_tex(&mut enc, &src.tex, &layer.tex, self.canvas_size);
        queue.submit([enc.finish()]);
        self.smart_sources.remove(&id);
    }

    /// Re-apply a layer's **non-destructive smart-filter stack**: reset the layer
    /// to its source pixels, then run each `(kind, radius, amount)` pass over it
    /// in order. The source is snapshotted on the first call (see
    /// [`Self::ensure_smart_source`]) and never overwritten, so this is fully
    /// reversible — passing an empty `passes` (all filters disabled) leaves the
    /// layer equal to the source. Reuses the same GPU filter passes the
    /// destructive Filter menu uses (separable blur runs H then V); no undo
    /// snapshot is taken (the stack itself, held app-side, is the edit history).
    pub fn reapply_smart_filters(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        id: LayerId,
        passes: &[crate::SmartPass],
    ) {
        self.ensure_smart_source(device, queue, id);
        let (Some(src), Some(layer), Some(pong)) = (
            self.smart_sources.get(&id),
            self.layers.get(&id),
            self.pong.as_ref(),
        ) else {
            return;
        };
        // Start from the un-filtered source pixels.
        let mut enc = device.create_command_encoder(&Default::default());
        copy_tex(&mut enc, &src.tex, &layer.tex, self.canvas_size);
        queue.submit([enc.finish()]);
        // Apply each enabled filter in order, in place on the layer texture.
        let (txw, txh) = (
            1.0 / self.canvas_size.width as f32,
            1.0 / self.canvas_size.height as f32,
        );
        for pass in passes {
            let (kind, radius, amount) = (pass.kind, pass.radius, pass.amount);
            if kind == 1 || kind == 5 {
                // Separable blur: layer -> pong (H), pong -> layer (V).
                self.filter_pass(device, queue, &layer.view, &pong.view, kind, [txw, 0.0], 0.0, radius);
                self.filter_pass(device, queue, &pong.view, &layer.view, kind, [0.0, txh], 0.0, radius);
            } else if kind == 32 {
                // Camera Raw: single pass carrying the develop controls in `cr`.
                let cr = [
                    [pass.cr[0], pass.cr[1], pass.cr[2], pass.cr[3]],
                    [pass.cr[4], pass.cr[5], pass.cr[6], pass.cr[7]],
                    [pass.cr[8], pass.cr[9], pass.cr[10], pass.cr[11]],
                ];
                self.filter_pass_cr(
                    device, queue, &layer.view, &layer.view, &pong.view, kind, [0.0; 2], amount,
                    radius, [0.0; 2], cr,
                );
                let mut enc = device.create_command_encoder(&Default::default());
                copy_tex(&mut enc, &pong.tex, &layer.tex, self.canvas_size);
                queue.submit([enc.finish()]);
            } else {
                // Single pass: layer -> pong, then copy pong back into layer.
                self.filter_pass(device, queue, &layer.view, &pong.view, kind, [0.0; 2], amount, radius);
                let mut enc = device.create_command_encoder(&Default::default());
                copy_tex(&mut enc, &pong.tex, &layer.tex, self.canvas_size);
                queue.submit([enc.finish()]);
            }
        }
    }
}
