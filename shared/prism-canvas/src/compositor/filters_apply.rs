//! The per-filter destructive `apply_*` methods on [`CanvasGpu`] — one public
//! entry point per Filter-menu effect (motion / radial / distort / stylize /
//! noise / pixelate / tonal / Blur Gallery / clouds). Each drives the shared
//! `filter_pass*` primitives in [`super::filters_core`] over a layer's texture
//! (with the ping/pong scratch), takes an undo snapshot, and is destructive +
//! undoable. The CPU reference math for these lives in [`super::super::filter_math`].

use crate::*;

impl CanvasGpu {
    /// Motion blur: a flat box average of `2*radius+1` taps along `angle_rad`
    /// (a directional/linear blur). Single pass; destructive + undoable.
    pub fn apply_motion_blur(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        id: LayerId,
        angle_rad: f32,
        radius: f32,
    ) {
        self.begin_command_now(device, queue, id, "Motion Blur");
        let (Some(layer), Some(pong)) = (self.layers.get(&id), self.pong.as_ref()) else {
            return;
        };
        // Unit direction scaled into uv (texel) space, matching the CPU ref
        // which steps one pixel per tap along (cos, sin).
        let dir = [
            angle_rad.cos() / self.canvas_size.width as f32,
            angle_rad.sin() / self.canvas_size.height as f32,
        ];
        self.filter_pass(device, queue, &layer.view, &pong.view, 4, dir, 0.0, radius);
        let mut enc = device.create_command_encoder(&Default::default());
        copy_tex(&mut enc, &pong.tex, &layer.tex, self.canvas_size);
        queue.submit([enc.finish()]);
    }

    /// Radial blur about `(cx, cy)` (pixel coords). `spin` selects rotational
    /// (true) vs zoom (false). `amount` is the spin angle in radians or the
    /// zoom fraction; `samples` is the tap count. Single pass; destructive.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_radial_blur(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        id: LayerId,
        cx: f32,
        cy: f32,
        spin: bool,
        amount: f32,
        samples: u32,
    ) {
        self.begin_command_now(device, queue, id, "Radial Blur");
        let (Some(layer), Some(pong)) = (self.layers.get(&id), self.pong.as_ref()) else {
            return;
        };
        let kind = if spin { 6 } else { 7 };
        let center = [
            cx / self.canvas_size.width as f32,
            cy / self.canvas_size.height as f32,
        ];
        self.filter_pass_c(
            device,
            queue,
            &layer.view,
            &pong.view,
            kind,
            [0.0; 2],
            amount,
            samples.max(1) as f32,
            center,
        );
        let mut enc = device.create_command_encoder(&Default::default());
        copy_tex(&mut enc, &pong.tex, &layer.tex, self.canvas_size);
        queue.submit([enc.finish()]);
    }

    /// Apply a coordinate-displacement Distort filter about `(cx, cy)` (pixel
    /// coords). Each Distort kind remaps the sampled source coordinate per pixel
    /// and edge-clamps: Twirl (kind 8, `amount` = max angle rad, `radius` px),
    /// Pinch/Spherize (kind 9, signed `amount`, `radius` px), Ripple/Wave (kind
    /// 10, `dir` = `[amplitude_px, wavelength_px]`), Polar rect→polar (kind 11)
    /// and polar→rect (kind 12). Single pass; destructive + undoable.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_distort(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        id: LayerId,
        kind: u32,
        cx: f32,
        cy: f32,
        amount: f32,
        radius: f32,
        dir: [f32; 2],
    ) {
        self.begin_command_now(device, queue, id, "Distort");
        let (Some(layer), Some(pong)) = (self.layers.get(&id), self.pong.as_ref()) else {
            return;
        };
        let center = [
            cx / self.canvas_size.width as f32,
            cy / self.canvas_size.height as f32,
        ];
        self.filter_pass_c(
            device,
            queue,
            &layer.view,
            &pong.view,
            kind,
            dir,
            amount,
            radius,
            center,
        );
        let mut enc = device.create_command_encoder(&Default::default());
        copy_tex(&mut enc, &pong.tex, &layer.tex, self.canvas_size);
        queue.submit([enc.finish()]);
    }

    /// Apply a Stylize edge/relief filter to the active layer. Each is a single
    /// neighbour-sampling pass over a `width`-px Sobel step (`radius` in the
    /// shader): Find Edges (kind 13), Emboss (kind 14, `dir` = unit light dir,
    /// `amount` = relief gain), Glowing Edges (kind 15, `amount` = brightness),
    /// Diffuse (kind 16, `dir.x` = seed, `amount` = max neighbour displacement
    /// px, `width` ignored). Single pass; destructive + undoable (region-COW).
    #[allow(clippy::too_many_arguments)]
    pub fn apply_stylize(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        id: LayerId,
        kind: u32,
        amount: f32,
        width: f32,
        dir: [f32; 2],
    ) {
        self.begin_command_now(device, queue, id, "Stylize");
        let (Some(layer), Some(pong)) = (self.layers.get(&id), self.pong.as_ref()) else {
            return;
        };
        self.filter_pass_c(
            device,
            queue,
            &layer.view,
            &pong.view,
            kind,
            dir,
            amount,
            width,
            [0.0; 2],
        );
        let mut enc = device.create_command_encoder(&Default::default());
        copy_tex(&mut enc, &pong.tex, &layer.tex, self.canvas_size);
        queue.submit([enc.finish()]);
    }

    /// Oil Paint (Kuwahara quadrant filter, kind 27) on the active layer:
    /// replace each pixel with the mean colour of the lowest-luma-variance
    /// quadrant of its `(2·radius+1)²` window — painterly patches with crisp
    /// edges. `radius` is the quadrant half-size in px (clamped 1..8 in-shader).
    /// Single pass; destructive + undoable (region-COW).
    pub fn apply_oil_paint(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        id: LayerId,
        radius: f32,
    ) {
        self.begin_command_now(device, queue, id, "Oil Paint");
        let (Some(layer), Some(pong)) = (self.layers.get(&id), self.pong.as_ref()) else {
            return;
        };
        self.filter_pass_c(
            device,
            queue,
            &layer.view,
            &pong.view,
            27,
            [0.0; 2],
            0.0,
            radius,
            [0.0; 2],
        );
        let mut enc = device.create_command_encoder(&Default::default());
        copy_tex(&mut enc, &pong.tex, &layer.tex, self.canvas_size);
        queue.submit([enc.finish()]);
    }

    /// Add seeded-deterministic noise to the active layer (kind 17). `amount` is
    /// the noise strength (0..1); `mono` applies the same noise to R/G/B;
    /// `gaussian` selects gaussian (true) vs uniform (false) noise; `seed` makes
    /// it reproducible. Single pass; destructive + undoable (region-COW).
    pub fn apply_noise(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        id: LayerId,
        amount: f32,
        mono: bool,
        gaussian: bool,
        seed: f32,
    ) {
        self.begin_command_now(device, queue, id, "Add Noise");
        let (Some(layer), Some(pong)) = (self.layers.get(&id), self.pong.as_ref()) else {
            return;
        };
        let dir = [seed, if mono { 1.0 } else { 0.0 }];
        let gflag = if gaussian { 1.0 } else { 0.0 };
        self.filter_pass_c(
            device,
            queue,
            &layer.view,
            &pong.view,
            17,
            dir,
            amount,
            gflag,
            [0.0; 2],
        );
        let mut enc = device.create_command_encoder(&Default::default());
        copy_tex(&mut enc, &pong.tex, &layer.tex, self.canvas_size);
        queue.submit([enc.finish()]);
    }

    /// Per-channel median despeckle on the active layer: Median (kind 18, full
    /// replacement) or Dust & Scratches (kind 19, replace only when the pixel
    /// differs from the window median by more than `threshold`). `radius` is the
    /// window radius in px (window = `2·radius+1`). Single pass; destructive +
    /// undoable (region-COW).
    pub fn apply_median(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        id: LayerId,
        radius: f32,
        threshold: Option<f32>,
    ) {
        self.begin_command_now(device, queue, id, "Median");
        let (Some(layer), Some(pong)) = (self.layers.get(&id), self.pong.as_ref()) else {
            return;
        };
        let (kind, amount) = match threshold {
            Some(t) => (19, t),
            None => (18, 0.0),
        };
        self.filter_pass_c(
            device,
            queue,
            &layer.view,
            &pong.view,
            kind,
            [0.0; 2],
            amount,
            radius,
            [0.0; 2],
        );
        let mut enc = device.create_command_encoder(&Default::default());
        copy_tex(&mut enc, &pong.tex, &layer.tex, self.canvas_size);
        queue.submit([enc.finish()]);
    }

    /// Mosaic on the active layer (kind 20): average each `cell`×`cell` block to
    /// one colour (the true block mean, vs the legacy point-sampling Pixelate).
    /// `cell` is the cell size in px. Single pass; destructive + undoable.
    pub fn apply_mosaic(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        id: LayerId,
        cell: f32,
    ) {
        self.begin_command_now(device, queue, id, "Mosaic");
        let (Some(layer), Some(pong)) = (self.layers.get(&id), self.pong.as_ref()) else {
            return;
        };
        self.filter_pass_c(
            device,
            queue,
            &layer.view,
            &pong.view,
            20,
            [0.0; 2],
            0.0,
            cell,
            [0.0; 2],
        );
        let mut enc = device.create_command_encoder(&Default::default());
        copy_tex(&mut enc, &pong.tex, &layer.tex, self.canvas_size);
        queue.submit([enc.finish()]);
    }

    /// Crystallize on the active layer (kind 21): snap each pixel to the colour
    /// of its nearest jittered seed (one per `cell`×`cell` block, jittered by a
    /// hash of the block index + `seed`), giving irregular Voronoi cells. Single
    /// pass; destructive + undoable.
    pub fn apply_crystallize(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        id: LayerId,
        cell: f32,
        seed: f32,
    ) {
        self.begin_command_now(device, queue, id, "Crystallize");
        let (Some(layer), Some(pong)) = (self.layers.get(&id), self.pong.as_ref()) else {
            return;
        };
        self.filter_pass_c(
            device,
            queue,
            &layer.view,
            &pong.view,
            21,
            [seed, 0.0],
            0.0,
            cell,
            [0.0; 2],
        );
        let mut enc = device.create_command_encoder(&Default::default());
        copy_tex(&mut enc, &pong.tex, &layer.tex, self.canvas_size);
        queue.submit([enc.finish()]);
    }

    /// Color Halftone on the active layer (kind 22): a per-channel dot screen of
    /// `cell`-px cells rotated by `angle_rad`, each cell's channel average setting
    /// a dot radius (denser ink for darker channels). Single pass; destructive +
    /// undoable.
    pub fn apply_color_halftone(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        id: LayerId,
        cell: f32,
        angle_rad: f32,
    ) {
        self.begin_command_now(device, queue, id, "Color Halftone");
        let (Some(layer), Some(pong)) = (self.layers.get(&id), self.pong.as_ref()) else {
            return;
        };
        let dir = [angle_rad.cos(), angle_rad.sin()];
        self.filter_pass_c(
            device,
            queue,
            &layer.view,
            &pong.view,
            22,
            dir,
            0.0,
            cell,
            [0.0; 2],
        );
        let mut enc = device.create_command_encoder(&Default::default());
        copy_tex(&mut enc, &pong.tex, &layer.tex, self.canvas_size);
        queue.submit([enc.finish()]);
    }

    /// Mezzotint on the active layer (kind 23): seeded threshold dither to pure
    /// black/white grain. `amount` biases the threshold; `seed` makes it
    /// reproducible. Single pass; destructive + undoable.
    pub fn apply_mezzotint(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        id: LayerId,
        amount: f32,
        seed: f32,
    ) {
        self.begin_command_now(device, queue, id, "Mezzotint");
        let (Some(layer), Some(pong)) = (self.layers.get(&id), self.pong.as_ref()) else {
            return;
        };
        self.filter_pass_c(
            device,
            queue,
            &layer.view,
            &pong.view,
            23,
            [seed, 0.0],
            amount,
            0.0,
            [0.0; 2],
        );
        let mut enc = device.create_command_encoder(&Default::default());
        copy_tex(&mut enc, &pong.tex, &layer.tex, self.canvas_size);
        queue.submit([enc.finish()]);
    }

    /// High Pass on the active layer (kind 24): the classic Photoshop sharpen
    /// prep — subtract a Gaussian-blurred copy from the original and re-centre at
    /// mid-gray, leaving only the high-frequency detail/edges as a signed
    /// deviation about 0.5. `radius` is the Gaussian blur radius (larger →
    /// coarser detail kept); `amount` scales the detail (1 = identity high pass).
    /// Runs the separable Gaussian (kind 1, H then V) into the layer, then a
    /// two-input combine pass that reads the blurred layer + the saved original.
    /// Destructive + undoable (region-COW).
    pub fn apply_high_pass(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        id: LayerId,
        radius: f32,
        amount: f32,
    ) {
        self.begin_command_now(device, queue, id, "High Pass");
        let (Some(layer), Some(ping), Some(pong)) =
            (self.layers.get(&id), self.ping.as_ref(), self.pong.as_ref())
        else {
            return;
        };
        // Stash the untouched source in `ping` so the combine pass can read it
        // after the blur has overwritten the layer.
        let mut enc = device.create_command_encoder(&Default::default());
        copy_tex(&mut enc, &layer.tex, &ping.tex, self.canvas_size);
        queue.submit([enc.finish()]);
        // Separable Gaussian blur in place (kind 1): H into pong, V back into the
        // layer — mirrors `apply_filter`'s blur path exactly.
        let tx = [1.0 / self.canvas_size.width as f32, 0.0];
        let ty = [0.0, 1.0 / self.canvas_size.height as f32];
        self.filter_pass(device, queue, &layer.view, &pong.view, 1, tx, 0.0, radius);
        self.filter_pass(device, queue, &pong.view, &layer.view, 1, ty, 0.0, radius);
        // Combine: input = blurred layer, orig = saved original (ping) → pong.
        self.filter_pass_2(
            device,
            queue,
            &layer.view,
            &ping.view,
            &pong.view,
            24,
            [0.0; 2],
            amount,
            radius,
            [0.0; 2],
        );
        let mut enc = device.create_command_encoder(&Default::default());
        copy_tex(&mut enc, &pong.tex, &layer.tex, self.canvas_size);
        queue.submit([enc.finish()]);
    }

    /// Render Clouds (kind 25) / Difference Clouds (kind 26) into the active
    /// layer — a generator that fills it with a deterministic multi-octave
    /// value-noise (fBm) field. `seed` makes it reproducible; `scale` is the base
    /// feature size (px), `roughness` the per-octave amplitude falloff, `octaves`
    /// the layer count. Clouds ignores the source; Difference Clouds composites
    /// the field against the existing pixels via per-channel absolute difference
    /// (so repeated application builds veins). Single pass; destructive + undoable
    /// (region-COW).
    #[allow(clippy::too_many_arguments)]
    pub fn apply_clouds(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        id: LayerId,
        difference: bool,
        seed: f32,
        scale: f32,
        roughness: f32,
        octaves: u32,
    ) {
        let label = if difference {
            "Difference Clouds"
        } else {
            "Clouds"
        };
        self.begin_command_now(device, queue, id, label);
        let (Some(layer), Some(pong)) = (self.layers.get(&id), self.pong.as_ref()) else {
            return;
        };
        let kind = if difference { 26 } else { 25 };
        self.filter_pass_c(
            device,
            queue,
            &layer.view,
            &pong.view,
            kind,
            [seed, roughness],
            scale,
            octaves.max(1) as f32,
            [0.0; 2],
        );
        let mut enc = device.create_command_encoder(&Default::default());
        copy_tex(&mut enc, &pong.tex, &layer.tex, self.canvas_size);
        queue.submit([enc.finish()]);
    }

    /// Posterize the active layer (kind 28): quantize each colour channel to
    /// `levels` (2..=255) evenly spaced steps in display (sRGB) space, the classic
    /// destructive Image ▸ Adjustments ▸ Posterize. Alpha is preserved. Single
    /// pass; destructive + undoable (region-COW).
    pub fn apply_posterize(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        id: LayerId,
        levels: u32,
    ) {
        self.begin_command_now(device, queue, id, "Posterize");
        let (Some(layer), Some(pong)) = (self.layers.get(&id), self.pong.as_ref()) else {
            return;
        };
        self.filter_pass_c(
            device,
            queue,
            &layer.view,
            &pong.view,
            28,
            [0.0; 2],
            levels.clamp(2, 255) as f32,
            0.0,
            [0.0; 2],
        );
        let mut enc = device.create_command_encoder(&Default::default());
        copy_tex(&mut enc, &pong.tex, &layer.tex, self.canvas_size);
        queue.submit([enc.finish()]);
    }

    /// Threshold the active layer (kind 29): convert to pure black/white at a
    /// display-space Rec.709 luma cutoff `level` (0..1) — at/above → white, below
    /// → black — the destructive Image ▸ Adjustments ▸ Threshold. Alpha is
    /// preserved. Single pass; destructive + undoable (region-COW).
    pub fn apply_threshold(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        id: LayerId,
        level: f32,
    ) {
        self.begin_command_now(device, queue, id, "Threshold");
        let (Some(layer), Some(pong)) = (self.layers.get(&id), self.pong.as_ref()) else {
            return;
        };
        self.filter_pass_c(
            device,
            queue,
            &layer.view,
            &pong.view,
            29,
            [0.0; 2],
            level.clamp(0.0, 1.0),
            0.0,
            [0.0; 2],
        );
        let mut enc = device.create_command_encoder(&Default::default());
        copy_tex(&mut enc, &pong.tex, &layer.tex, self.canvas_size);
        queue.submit([enc.finish()]);
    }

    /// Tilt-Shift (Blur Gallery, kind 30) on the active layer: a graduated focus
    /// blur. The image stays sharp inside a horizontal focus band centred on
    /// `center_y` (uv 0..1) of `half_band` px half-width, and blurs progressively
    /// up to `max_radius` px once past `half_band + feather` (px). `angle_rad`
    /// tilts the band (0 = horizontal band, normal pointing down). Single pass;
    /// destructive + undoable (region-COW).
    #[allow(clippy::too_many_arguments)]
    pub fn apply_tilt_shift(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        id: LayerId,
        center_y: f32,
        half_band: f32,
        feather: f32,
        max_radius: f32,
        angle_rad: f32,
    ) {
        self.begin_command_now(device, queue, id, "Tilt-Shift");
        let (Some(layer), Some(pong)) = (self.layers.get(&id), self.pong.as_ref()) else {
            return;
        };
        // Band normal: angle 0 → (0, 1) (horizontal band). `center.x` carries the
        // feather (px), `center.y` the focus line (uv).
        let nrm = [-angle_rad.sin(), angle_rad.cos()];
        let center = [feather, center_y.clamp(0.0, 1.0)];
        self.filter_pass_c(
            device,
            queue,
            &layer.view,
            &pong.view,
            30,
            nrm,
            half_band.max(0.0),
            max_radius.max(0.0),
            center,
        );
        let mut enc = device.create_command_encoder(&Default::default());
        copy_tex(&mut enc, &pong.tex, &layer.tex, self.canvas_size);
        queue.submit([enc.finish()]);
    }

    /// Iris Blur (Blur Gallery, kind 31) on the active layer: the radial sibling
    /// of Tilt-Shift. The image stays sharp inside an elliptical region centred at
    /// `(cx, cy)` (pixel coords) with pixel radii `(rx, ry)`, and blurs
    /// progressively up to `max_radius` px outside it. `feather` is a normalized
    /// fraction of the ellipse radius (how far past the boundary the blur ramps to
    /// full). Single pass; destructive + undoable (region-COW).
    #[allow(clippy::too_many_arguments)]
    pub fn apply_iris_blur(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        id: LayerId,
        cx: f32,
        cy: f32,
        rx: f32,
        ry: f32,
        feather: f32,
        max_radius: f32,
    ) {
        self.begin_command_now(device, queue, id, "Iris Blur");
        let (Some(layer), Some(pong)) = (self.layers.get(&id), self.pong.as_ref()) else {
            return;
        };
        // `dir` carries the ellipse radii (px), `center` the center (uv),
        // `amount` the feather (normalized), `radius` the max blur (px).
        let center = [
            cx / self.canvas_size.width as f32,
            cy / self.canvas_size.height as f32,
        ];
        self.filter_pass_c(
            device,
            queue,
            &layer.view,
            &pong.view,
            31,
            [rx.max(0.0), ry.max(0.0)],
            feather.max(0.0),
            max_radius.max(0.0),
            center,
        );
        let mut enc = device.create_command_encoder(&Default::default());
        copy_tex(&mut enc, &pong.tex, &layer.tex, self.canvas_size);
        queue.submit([enc.finish()]);
    }

    /// Field Blur (Blur Gallery, kind 33) on the active layer: a multi-pin
    /// variable blur. Each pin is `(x_px, y_px, amount_px)`; the per-pixel blur
    /// radius is interpolated between the pins by inverse-distance-squared
    /// weighting (mirroring `filter_math::field_blur_radius_at`), then a local 2D
    /// Gaussian of that radius runs at the pixel. A single pin is a uniform blur;
    /// an empty list is the identity. Up to three pins fit the uniform overflow
    /// slots (extras are ignored). Single pass; destructive + undoable (region-COW).
    pub fn apply_field_blur(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        id: LayerId,
        pins: &[(f32, f32, f32)],
    ) {
        self.begin_command_now(device, queue, id, "Field Blur");
        let (Some(layer), Some(pong)) = (self.layers.get(&id), self.pong.as_ref()) else {
            return;
        };
        let (w, h) = (
            self.canvas_size.width as f32,
            self.canvas_size.height as f32,
        );
        // Pack up to three pins into the cr overflow vec4s as
        // (x_uv, y_uv, amount_px, _); cr0.w carries the pin count.
        let n = pins.len().min(3);
        let mut cr = [[0.0f32; 4]; 3];
        for (i, &(x, y, amount)) in pins.iter().take(3).enumerate() {
            cr[i] = [x / w, y / h, amount.max(0.0), 0.0];
        }
        cr[0][3] = n as f32;
        self.filter_pass_cr(
            device,
            queue,
            &layer.view,
            &layer.view,
            &pong.view,
            33,
            [0.0; 2],
            0.0,
            0.0,
            [0.0; 2],
            cr,
        );
        let mut enc = device.create_command_encoder(&Default::default());
        copy_tex(&mut enc, &pong.tex, &layer.tex, self.canvas_size);
        queue.submit([enc.finish()]);
    }

}
