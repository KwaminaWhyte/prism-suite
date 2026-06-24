//! [`Comp`](super::Comp) **construction** extracted from `comp/mod.rs` (workspace
//! size rule): `new` (the seeded demo composition — animated solid, parented
//! satellite, shape, text, adjustment, and fractal-noise layers), `empty_like`
//! (the bare container a pre-compose drops layers into), and `display_name`.
//!
//! A first `impl Comp` block — Rust allows the inherent impl to span files in the
//! same crate. `use super::*` brings the `comp` module's types into scope, so the
//! bodies are byte-identical to when they lived inline.

use super::*;

impl Comp {
    /// A fresh 1280x720, 5-second, 30fps composition with a parented demo pair.
    pub fn new() -> Self {
        let mut c = Self {
            id: 0,
            name: "Comp 1".to_string(),
            width: 1280,
            height: 720,
            duration: 5.0,
            fps: 30.0,
            motion_blur: MotionBlur::default(),
            markers: Vec::new(),
            work_area: WorkArea::full(5.0),
            camera: Camera::default(),
            lights: Vec::new(),
            layers: Vec::new(),
            hide_shy: false,
        };
        // Place the default camera at the comp-height-derived distance so the
        // z = 0 plane fills the frame at unit scale (the 2-D look) out of the box.
        c.camera.position = [0.0, 0.0, -Camera::default_distance(c.height as f32)];
        // Enable comp motion blur so the demo's fast slide/spin reads with a
        // cinematic shutter out of the box (the sliding solid opts in below).
        c.motion_blur.enabled = true;
        // Seed an animated layer so the preview/timeline aren't empty on launch.
        // The X slide uses Easy Ease so the easing is visible immediately (it
        // eases in and out of the travel rather than gliding linearly), while
        // rotation stays linear for contrast.
        let mut demo = PulseLayer::new("Solid 1", [0.27, 0.55, 0.85, 1.0]);
        demo.x.set_key(0.0, -300.0);
        demo.x.set_key(5.0, 300.0);
        demo.x.set_interp(0.0, Interp::Ease(Ease::EASY));
        demo.rotation.set_key(0.0, 0.0);
        demo.rotation.set_key(5.0, 180.0);
        demo.motion_blur = true; // opt this layer into the comp's shutter
                                 // A soft elliptical mask carves the solid into a feathered oval (sized to
                                 // the layer's base quad), so masks read out of the box.
        let mask_hw = 1280.0 * 0.22; // matches the renderer's LAYER_HALF_FRAC
        let mask_hh = 720.0 * 0.22;
        let mut oval = Mask::ellipse(mask_hw, mask_hh);
        oval.feather = 60.0;
        demo.masks.push(oval);
        c.layers.push(demo); // index 0

        // A smaller satellite parented to Solid 1: it rides the parent's slide
        // and spin while orbiting via its own position offset — showcasing
        // parenting and the anchor-based pivot out of the box.
        let mut satellite = PulseLayer::new("Satellite", [0.95, 0.72, 0.25, 1.0]);
        satellite.parent = Some(0);
        satellite.scale.set_key(0.0, 0.4);
        satellite.x.set_key(0.0, 360.0);
        satellite.y.set_key(0.0, -180.0);
        // An **expression** drives the satellite's rotation: it spins steadily
        // with time and jitters with a deterministic wiggle — so the AE-style
        // per-property expression engine reads on launch (and demonstrates
        // `time` + `wiggle` + offsetting the keyframed `value`).
        satellite.rotation.expression = Some("value + time * 120 + wiggle(3, 15)".to_string());
        // A soft drop shadow + glow on the satellite so the spatial-effect stack
        // (whole-buffer blur/shadow/bloom passes) reads out of the box.
        satellite.spatial_effects.push(SpatialEffect::DropShadow {
            color: [0.0, 0.0, 0.0],
            opacity: 0.55,
            angle: 135.0,
            distance: 16.0,
            softness: 10.0,
            shadow_only: false,
        });
        satellite.spatial_effects.push(SpatialEffect::Glow {
            threshold: 0.5,
            radius: 18.0,
            intensity: 0.9,
        });
        // A subtle effect-level **Transform** (a Distort effect) gives the
        // satellite an extra in-stack scale-up, so the whole-buffer
        // coordinate-remap distort family reads out of the box (it warps the
        // already-shadowed/glowed buffer, after the spatial passes).
        satellite.distort_effects.push(DistortEffect::Transform {
            anchor: [0.5, 0.5],
            position: [0.5, 0.5],
            scale: 1.12,
            rotation: 0.0,
            skew: 0.0,
            opacity: 1.0,
        });
        // A gentle **Matte Choke** (a Keying effect) crisps the satellite's matte
        // edge before the spatial passes, so the whole-buffer alpha-pulling keying
        // family reads out of the box. It runs *before* the glow/shadow, so the
        // keyer carves the matte and the spatial passes then soften it — AE's
        // keyer-then-blur matte-refine order. Near-identity on the crisp solid
        // (clips just the soft alpha tails), so the demo's look is preserved.
        satellite.key_effects.push(KeyEffect::MatteChoke {
            choke: 0.0,
            clip_black: 0.02,
            clip_white: 0.98,
        });
        c.layers.push(satellite); // index 1

        // A shape layer: a stroked five-point star drifting up the frame, so the
        // vector shape rasterizer (fill + stroke, parametric primitive) reads out
        // of the box. It slides on its own X position with an Easy Ease.
        let mut star = PulseLayer::of_kind(LayerKind::Shape, "Star", [0.9, 0.3, 0.45, 1.0]);
        let mut star_item = ShapeItem::new(ShapePrimitive::Star {
            points: 5,
            outer: 130.0,
            inner: 56.0,
        });
        star_item.fill = Some(Fill {
            color: [0.95, 0.35, 0.5],
            opacity: 1.0,
        });
        star_item.stroke = Some(Stroke {
            color: [1.0, 1.0, 1.0],
            width: 8.0,
            opacity: 1.0,
        });
        star.shape.items.push(star_item);
        // Screen blend so the star brightens (rather than covers) wherever it
        // crosses the layers beneath it — the per-layer blend mode reads on launch.
        star.blend = LayerBlend(BlendMode::Screen);
        star.x.set_key(0.0, -260.0);
        star.x.set_key(5.0, 260.0);
        star.x.set_interp(0.0, Interp::Ease(Ease::EASY));
        star.y.set_key(0.0, 180.0);
        star.rotation.set_key(0.0, 0.0);
        star.rotation.set_key(5.0, 90.0);
        c.layers.push(star); // index 2

        // A title text layer near the top, drawn with the built-in stroke font:
        // it fades up over the first second (an opacity key) and carries a soft
        // outline, so text layers read out of the box.
        let mut title = PulseLayer::of_kind(LayerKind::Text, "Title", [1.0; 4]);
        title.text = TextLayer {
            text: "PULSE".to_string(),
            size: 150.0,
            tracking: 12.0,
            leading: 0.0,
            align: TextAlign::Center,
            font_family: None,
            fill: Some(Fill {
                color: [0.96, 0.97, 1.0],
                opacity: 1.0,
            }),
            stroke: Some(Stroke {
                color: [0.27, 0.55, 0.85],
                width: 6.0,
                opacity: 1.0,
            }),
        };
        title.y.set_key(0.0, -230.0);
        title.opacity.set_key(0.0, 0.0);
        title.opacity.set_key(1.0, 1.0);
        title.opacity.set_interp(0.0, Interp::Ease(Ease::EASY));
        c.layers.push(title); // index 3

        // A full-frame adjustment layer on top: its effect stack regrades every
        // layer beneath it (here a punchy Levels contrast) without drawing any
        // pixels of its own — showcasing layer kinds + the effect stack on launch.
        let mut grade = PulseLayer::of_kind(LayerKind::Adjustment, "Grade", [1.0; 4]);
        grade.scale.set_key(0.0, 3.0); // cover the whole frame
        grade.effects.push(Effect::Levels {
            in_black: 0.05,
            in_white: 0.85,
            gamma: 1.1,
            out_black: 0.0,
            out_white: 1.0,
        });
        c.layers.push(grade); // index 4

        // A full-frame **Fractal Noise** layer on top: a moving cloud texture
        // screened over the composite. Its **evolution** is keyframed (0 → 6 over
        // the comp), so the noise field flows — the generate workhorse + its
        // signature animate-the-evolution motion read out of the box. Screen blend
        // at modest opacity so it textures rather than covers.
        let mut noise = PulseLayer::new("Fractal Noise", [1.0; 4]);
        noise.scale.set_key(0.0, 3.0); // cover the whole frame
        noise.blend = LayerBlend(BlendMode::Screen);
        noise.opacity.set_key(0.0, 0.35);
        noise.generate = Some(GenerateEffect::FractalNoise {
            fractal_type: FractalType::Turbulent,
            contrast: 1.3,
            brightness: -0.1,
            scale: 140.0,
            scale_x: 1.0,
            scale_y: 1.0,
            complexity: 6,
            sub_influence: 0.6,
            sub_scaling: 2.0,
            evolution: 0.0,
            seed: 7,
            overflow: Overflow::Clip,
            opacity: 1.0,
        });
        // Keyframe the evolution to flow the field over the timeline.
        noise.generate_evolution.set_key(0.0, 0.0);
        noise.generate_evolution.set_key(5.0, 6.0);
        c.layers.push(noise); // index 5

        // A comp marker mid-timeline so markers + time navigation read out of the
        // box (jump to it with the timeline's marker-nav buttons).
        c.markers.push({
            let mut m = Marker::at(2.5);
            m.label = "Beat".to_string();
            m
        });
        c
    }

    /// An empty comp with the given name and canvas/timeline matching `like`
    /// (size, duration, fps) but no layers and no demo content — the container a
    /// **pre-compose** drops the wrapped layers into. Its `id` is `0` until the
    /// project assigns one on [`Project::push_comp`].
    pub fn empty_like(name: impl Into<String>, like: &Comp) -> Self {
        Self {
            id: 0,
            name: name.into(),
            width: like.width,
            height: like.height,
            duration: like.duration,
            fps: like.fps,
            motion_blur: MotionBlur::default(),
            markers: Vec::new(),
            work_area: WorkArea::full(like.duration),
            camera: {
                let mut cam = Camera::default();
                cam.position = [0.0, 0.0, -Camera::default_distance(like.height as f32)];
                cam
            },
            lights: Vec::new(),
            layers: Vec::new(),
            hide_shy: false,
        }
    }

    /// A short label for the comp: its `name`, or a generated `Comp <id>` when
    /// unnamed (old files / freshly minted comps).
    pub fn display_name(&self) -> String {
        if self.name.is_empty() {
            format!("Comp {}", self.id)
        } else {
            self.name.clone()
        }
    }
}
