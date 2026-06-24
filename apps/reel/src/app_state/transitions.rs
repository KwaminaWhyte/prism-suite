//! Transition **geometry suite** — the spatial / motion transitions whose look
//! is a per-pixel warp of the outgoing (A) and incoming (B) frames rather than a
//! simple opacity blend.
//!
//! The opacity-style transitions (cross-dissolve, dip-to-color, film dissolve)
//! and the mask-style ones (iris, clock, diagonal, pixel dissolve) live in
//! [`super::timeline`] as `Transition::weights` / `Transition::pixel_mask`. The
//! *geometric* ones — **Slide**, **Zoom**, **Spin**, **Cube fold**, and the
//! directional **Push** / **Wipe** family — map progress `t ∈ [0,1]` to a warped
//! composite of two pre-sampled RGBA frames. That logic lives here so it doesn't
//! bloat the already-large `timeline.rs`.
//!
//! [`TransitionGeometryExt::geometry_blend`] is the single entry point the
//! compositor ([`crate::program_frame`]) calls: given the outgoing + incoming
//! frames it returns `Some(blended)` for a geometric kind, or `None` so the
//! caller falls back to the weight/mask path. Every helper here is pure and
//! deterministic — straight-sRGB RGBA8 in, straight-sRGB RGBA8 out — so the unit
//! tests can assert the blend at `t = 0 / 0.5 / 1`.

use super::{App, Action, SlideDirection, SpinDirection, CubeDirection, Transition, TransitionKind, DEFAULT_TRANSITION_DUR};

/// Geometric-transition sampling, implemented on [`Transition`] via an extension
/// trait (the type lives in `timeline.rs`; the geometry lives here).
pub trait TransitionGeometryExt {
    /// Blend the outgoing `from` and incoming `to` frames (each `w*h*4` straight
    /// sRGB RGBA8) at this transition's progress for timeline time `t`. Returns
    /// `Some(buf)` for a geometric kind handled here, `None` for kinds the
    /// compositor resolves through `weights` / `push_offsets` / `pixel_mask`.
    fn geometry_blend(&self, t: f32, from: &[u8], to: &[u8], w: u32, h: u32) -> Option<Vec<u8>>;
}

impl TransitionGeometryExt for Transition {
    fn geometry_blend(&self, t: f32, from: &[u8], to: &[u8], w: u32, h: u32) -> Option<Vec<u8>> {
        let p = self.progress(t);
        match self.kind {
            TransitionKind::Slide(dir) => Some(slide(from, to, w, h, p, dir)),
            TransitionKind::Zoom { grow } => Some(zoom(from, to, w, h, p, grow)),
            TransitionKind::SpinAway(dir) => Some(spin(from, to, w, h, p, dir)),
            TransitionKind::Cube { direction, .. } => Some(cube_fold(from, to, w, h, p, direction)),
            _ => None,
        }
    }
}

/// Sample one straight-RGBA pixel at fractional source coords `(fx, fy)` in
/// `[0,w)×[0,h)`. Out-of-bounds reads return transparent black. Nearest sample.
#[inline]
fn sample_px(buf: &[u8], w: u32, h: u32, fx: f32, fy: f32) -> [u8; 4] {
    if fx < 0.0 || fy < 0.0 {
        return [0, 0, 0, 0];
    }
    let x = fx as u32;
    let y = fy as u32;
    if x >= w || y >= h {
        return [0, 0, 0, 0];
    }
    let i = ((y * w + x) * 4) as usize;
    if i + 4 > buf.len() {
        return [0, 0, 0, 0];
    }
    [buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]
}

/// Source-over one straight-RGBA pixel `fg` onto `dst` at index `i` (RGBA8).
#[inline]
fn over_px(dst: &mut [u8], i: usize, fg: [u8; 4]) {
    let fa = fg[3] as f32 / 255.0;
    if fa <= 0.0 {
        return;
    }
    let inv = 1.0 - fa;
    for c in 0..3 {
        dst[i + c] = (fg[c] as f32 * fa + dst[i + c] as f32 * inv).round().clamp(0.0, 255.0) as u8;
    }
    let out_a = fa + (dst[i + 3] as f32 / 255.0) * inv;
    dst[i + 3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
}

/// **Slide**: the incoming clip slides over the (stationary) outgoing clip from
/// the given edge, covering it as `p` goes 0→1. (Unlike Push, the outgoing clip
/// does not move.)
fn slide(from: &[u8], to: &[u8], w: u32, h: u32, p: f32, dir: SlideDirection) -> Vec<u8> {
    let p = p.clamp(0.0, 1.0);
    let mut out = from.to_vec();
    let (dx, dy) = match dir {
        SlideDirection::Left => (-(1.0 - p) * w as f32, 0.0),
        SlideDirection::Right => ((1.0 - p) * w as f32, 0.0),
        SlideDirection::Up => (0.0, -(1.0 - p) * h as f32),
        SlideDirection::Down => (0.0, (1.0 - p) * h as f32),
    };
    for y in 0..h {
        for x in 0..w {
            let sx = x as f32 - dx;
            let sy = y as f32 - dy;
            let px = sample_px(to, w, h, sx, sy);
            if px[3] > 0 {
                over_px(&mut out, ((y * w + x) * 4) as usize, px);
            }
        }
    }
    out
}

/// **Zoom**: when `grow`, the incoming clip scales up from a point (0→full) and
/// dissolves in; otherwise the outgoing clip scales up and out while the incoming
/// shows beneath. A crisp, deterministic scale-about-center with an opacity ramp.
fn zoom(from: &[u8], to: &[u8], w: u32, h: u32, p: f32, grow: bool) -> Vec<u8> {
    let p = p.clamp(0.0, 1.0);
    let cx = w as f32 / 2.0;
    let cy = h as f32 / 2.0;
    let mut out = vec![0u8; (w * h * 4) as usize];
    // Base layer fully covers; overlay scales with an opacity ramp.
    let (base, overlay, scale, overlay_alpha) = if grow {
        // Incoming grows from tiny → full; outgoing is the static base.
        (from, to, 0.1 + 0.9 * p, p)
    } else {
        // Outgoing grows huge and fades; incoming is the static base.
        (to, from, 1.0 + 3.0 * p, 1.0 - p)
    };
    // Paint the base first.
    out.copy_from_slice(base);
    let inv_scale = 1.0 / scale.max(1e-3);
    for y in 0..h {
        for x in 0..w {
            // Map dest → overlay source about the center at `scale`.
            let sx = (x as f32 - cx) * inv_scale + cx;
            let sy = (y as f32 - cy) * inv_scale + cy;
            let mut px = sample_px(overlay, w, h, sx, sy);
            px[3] = ((px[3] as f32 / 255.0) * overlay_alpha * 255.0).round().clamp(0.0, 255.0) as u8;
            if px[3] > 0 {
                over_px(&mut out, ((y * w + x) * 4) as usize, px);
            }
        }
    }
    out
}

/// **Spin**: the outgoing clip rotates about the center and shrinks away (with a
/// fade) revealing the incoming clip beneath. `dir` sets the spin direction; the
/// total sweep is one full turn.
fn spin(from: &[u8], to: &[u8], w: u32, h: u32, p: f32, dir: SpinDirection) -> Vec<u8> {
    let p = p.clamp(0.0, 1.0);
    let cx = w as f32 / 2.0;
    let cy = h as f32 / 2.0;
    // Incoming clip is the base; outgoing spins + shrinks + fades on top.
    let mut out = to.to_vec();
    let turns = std::f32::consts::PI * 2.0;
    let angle = match dir {
        SpinDirection::Clockwise => -turns * p,
        SpinDirection::CounterClockwise => turns * p,
    };
    let scale = (1.0 - p).max(0.0); // shrink to nothing
    if scale <= 1e-3 {
        return out;
    }
    let (sin_a, cos_a) = angle.sin_cos();
    let inv_scale = 1.0 / scale;
    let overlay_alpha = 1.0 - p;
    for y in 0..h {
        for x in 0..w {
            // Inverse map dest → outgoing source: undo scale then undo rotation.
            let rx = (x as f32 - cx) * inv_scale;
            let ry = (y as f32 - cy) * inv_scale;
            // Rotate by -angle (inverse).
            let sx = rx * cos_a + ry * sin_a + cx;
            let sy = -rx * sin_a + ry * cos_a + cy;
            let mut px = sample_px(from, w, h, sx, sy);
            px[3] = ((px[3] as f32 / 255.0) * overlay_alpha * 255.0).round().clamp(0.0, 255.0) as u8;
            if px[3] > 0 {
                over_px(&mut out, ((y * w + x) * 4) as usize, px);
            }
        }
    }
    out
}

/// **Cube fold**: a 3D-cube-style rotation. Modeled as a horizontal (or vertical)
/// split where the outgoing clip horizontally compresses out toward one edge and
/// the incoming clip expands in from the other, simulating two faces of a cube
/// turning. The seam moves with `p`; each face is horizontally scaled so the
/// whole frame is always filled (no gap).
fn cube_fold(from: &[u8], to: &[u8], w: u32, h: u32, p: f32, dir: CubeDirection) -> Vec<u8> {
    let p = p.clamp(0.0, 1.0);
    let mut out = vec![0u8; (w * h * 4) as usize];
    match dir {
        CubeDirection::Left | CubeDirection::Right => {
            // Seam x-position: for Left the incoming enters from the right.
            let seam = match dir {
                CubeDirection::Left => (1.0 - p) * w as f32,
                _ => p * w as f32, // Right
            };
            let seam_px = seam.round().clamp(0.0, w as f32) as u32;
            for y in 0..h {
                for x in 0..w {
                    let (src, sx) = if (dir == CubeDirection::Left && x < seam_px)
                        || (dir == CubeDirection::Right && x >= seam_px)
                    {
                        // Outgoing face, horizontally compressed into its band.
                        let band_w = match dir {
                            CubeDirection::Left => seam_px.max(1),
                            _ => (w - seam_px).max(1),
                        };
                        let local = match dir {
                            CubeDirection::Left => x as f32,
                            _ => (x - seam_px) as f32,
                        };
                        (from, local / band_w as f32 * w as f32)
                    } else {
                        // Incoming face, horizontally compressed into its band.
                        let band_w = match dir {
                            CubeDirection::Left => (w - seam_px).max(1),
                            _ => seam_px.max(1),
                        };
                        let local = match dir {
                            CubeDirection::Left => (x - seam_px) as f32,
                            _ => x as f32,
                        };
                        (to, local / band_w as f32 * w as f32)
                    };
                    let px = sample_px(src, w, h, sx, y as f32);
                    let i = ((y * w + x) * 4) as usize;
                    out[i..i + 4].copy_from_slice(&px);
                }
            }
        }
        CubeDirection::Up | CubeDirection::Down => {
            let seam = match dir {
                CubeDirection::Up => (1.0 - p) * h as f32,
                _ => p * h as f32,
            };
            let seam_px = seam.round().clamp(0.0, h as f32) as u32;
            for y in 0..h {
                for x in 0..w {
                    let (src, sy) = if (dir == CubeDirection::Up && y < seam_px)
                        || (dir == CubeDirection::Down && y >= seam_px)
                    {
                        let band_h = match dir {
                            CubeDirection::Up => seam_px.max(1),
                            _ => (h - seam_px).max(1),
                        };
                        let local = match dir {
                            CubeDirection::Up => y as f32,
                            _ => (y - seam_px) as f32,
                        };
                        (from, local / band_h as f32 * h as f32)
                    } else {
                        let band_h = match dir {
                            CubeDirection::Up => (h - seam_px).max(1),
                            _ => seam_px.max(1),
                        };
                        let local = match dir {
                            CubeDirection::Up => (y - seam_px) as f32,
                            _ => y as f32,
                        };
                        (to, local / band_h as f32 * h as f32)
                    };
                    let px = sample_px(src, w, h, x as f32, sy);
                    let i = ((y * w + x) * 4) as usize;
                    out[i..i + 4].copy_from_slice(&px);
                }
            }
        }
    }
    out
}

// --- Apply helpers (panel → state) ------------------------------------------

pub trait AppTransitionsExt {
    fn apply_transitions(&mut self, action: Action);
}

impl AppTransitionsExt for App {
    fn apply_transitions(&mut self, action: Action) {
        match action {
            Action::AddSlideTransition { index, direction, duration } => {
                let t = self.snap_to_frame(self.time);
                let dur = if duration > 0.0 { duration } else { DEFAULT_TRANSITION_DUR };
                if self.project.add_transition(index, t, TransitionKind::Slide(direction), dur).is_some() {
                    self.host.mark_dirty();
                }
            }
            Action::AddSpinTransition { index, direction } => {
                let t = self.snap_to_frame(self.time);
                if self.project.add_transition(index, t, TransitionKind::SpinAway(direction), DEFAULT_TRANSITION_DUR).is_some() {
                    self.host.mark_dirty();
                }
            }
            Action::AddZoomTransition2 { index, grow } => {
                let t = self.snap_to_frame(self.time);
                if self.project.add_transition(index, t, TransitionKind::Zoom { grow }, DEFAULT_TRANSITION_DUR).is_some() {
                    self.host.mark_dirty();
                }
            }
            Action::AddCubeFoldTransition { index, direction } => {
                let t = self.snap_to_frame(self.time);
                if self.project.add_transition(index, t, TransitionKind::Cube { direction, lighting: true }, DEFAULT_TRANSITION_DUR).is_some() {
                    self.host.mark_dirty();
                }
            }
            Action::AddPushTransition { index, direction } => {
                let t = self.snap_to_frame(self.time);
                if self.project.add_transition(index, t, TransitionKind::Push(direction), DEFAULT_TRANSITION_DUR).is_some() {
                    self.host.mark_dirty();
                }
            }
            Action::AddWipeTransition2 { index, direction } => {
                let t = self.snap_to_frame(self.time);
                if self.project.add_transition(index, t, TransitionKind::Wipe(direction), DEFAULT_TRANSITION_DUR).is_some() {
                    self.host.mark_dirty();
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state::{App, Action, Clip, ClipSource, Project, Track, SlideDirection, SpinDirection, CubeDirection, WipeDir, Transition, TransitionKind};

    fn flat(color: [u8; 4], w: u32, h: u32) -> Vec<u8> {
        let mut v = Vec::with_capacity((w * h * 4) as usize);
        for _ in 0..(w * h) {
            v.extend_from_slice(&color);
        }
        v
    }

    fn tr(kind: TransitionKind) -> Transition {
        Transition { kind, from: 0, to: 1, center: 5.0, duration: 2.0 }
    }

    #[test]
    fn slide_at_zero_is_outgoing_at_one_is_incoming() {
        let from = flat([255, 0, 0, 255], 8, 8);
        let to = flat([0, 0, 255, 255], 8, 8);
        let t = tr(TransitionKind::Slide(SlideDirection::Left));
        // p=0 at start (t=4): incoming fully off-frame, so still outgoing (red).
        let a = t.geometry_blend(4.0, &from, &to, 8, 8).unwrap();
        assert_eq!(&a[0..4], &[255, 0, 0, 255]);
        // p=1 at end (t=6): incoming fully covers (blue).
        let b = t.geometry_blend(6.0, &from, &to, 8, 8).unwrap();
        assert_eq!(&b[0..4], &[0, 0, 255, 255]);
    }

    #[test]
    fn slide_at_half_is_a_mix_of_both() {
        let from = flat([255, 0, 0, 255], 8, 8);
        let to = flat([0, 0, 255, 255], 8, 8);
        let t = tr(TransitionKind::Slide(SlideDirection::Left));
        let mid = t.geometry_blend(5.0, &from, &to, 8, 8).unwrap();
        // Left half should be covered by the incoming blue, right half red.
        let left = &mid[(0 * 8 + 0) as usize * 4..][..4];
        let right = &mid[(0 * 8 + 7) as usize * 4..][..4];
        assert_eq!(left, &[0, 0, 255, 255], "left covered by incoming");
        assert_eq!(right, &[255, 0, 0, 255], "right still outgoing");
    }

    #[test]
    fn zoom_grow_endpoints() {
        let from = flat([255, 0, 0, 255], 8, 8);
        let to = flat([0, 255, 0, 255], 8, 8);
        let t = tr(TransitionKind::Zoom { grow: true });
        // p=0 → base (outgoing red), overlay alpha 0.
        let a = t.geometry_blend(4.0, &from, &to, 8, 8).unwrap();
        assert_eq!(&a[0..4], &[255, 0, 0, 255]);
        // p=1 → incoming fully grown + opaque (green) over center.
        let b = t.geometry_blend(6.0, &from, &to, 8, 8).unwrap();
        let center = ((4 * 8 + 4) * 4) as usize;
        assert_eq!(&b[center..center + 4], &[0, 255, 0, 255]);
    }

    #[test]
    fn spin_endpoints() {
        let from = flat([255, 0, 0, 255], 8, 8);
        let to = flat([0, 255, 0, 255], 8, 8);
        let t = tr(TransitionKind::SpinAway(SpinDirection::Clockwise));
        // p=0 → outgoing on top, full (red) at center.
        let a = t.geometry_blend(4.0, &from, &to, 8, 8).unwrap();
        let center = ((4 * 8 + 4) * 4) as usize;
        assert_eq!(&a[center..center + 4], &[255, 0, 0, 255]);
        // p=1 → outgoing shrunk to nothing, only incoming (green) remains.
        let b = t.geometry_blend(6.0, &from, &to, 8, 8).unwrap();
        assert_eq!(&b[center..center + 4], &[0, 255, 0, 255]);
    }

    #[test]
    fn cube_fold_endpoints_and_seam() {
        let from = flat([255, 0, 0, 255], 8, 8);
        let to = flat([0, 255, 0, 255], 8, 8);
        let t = tr(TransitionKind::Cube { direction: CubeDirection::Left, lighting: true });
        // p=0 → fully outgoing (red).
        let a = t.geometry_blend(4.0, &from, &to, 8, 8).unwrap();
        assert_eq!(&a[0..4], &[255, 0, 0, 255]);
        // p=1 → fully incoming (green).
        let b = t.geometry_blend(6.0, &from, &to, 8, 8).unwrap();
        assert_eq!(&b[0..4], &[0, 255, 0, 255]);
        // p=0.5 → left band outgoing, right band incoming.
        let mid = t.geometry_blend(5.0, &from, &to, 8, 8).unwrap();
        assert_eq!(&mid[0..4], &[255, 0, 0, 255], "left band is outgoing");
        let right = ((0 * 8 + 7) * 4) as usize;
        assert_eq!(&mid[right..right + 4], &[0, 255, 0, 255], "right band is incoming");
    }

    #[test]
    fn non_geometric_kind_returns_none() {
        let from = flat([255, 0, 0, 255], 4, 4);
        let to = flat([0, 0, 255, 255], 4, 4);
        let t = tr(TransitionKind::CrossDissolve);
        assert!(t.geometry_blend(5.0, &from, &to, 4, 4).is_none());
    }

    fn two_clip_project() -> Project {
        let tracks = vec![Track { name: "V1".into(), enabled: true }];
        let clips = vec![
            Clip { name: "A".into(), source: ClipSource::Color([1.0, 0.0, 0.0, 1.0]), track: 0, start: 0.0, duration: 5.0, ..Clip::default() },
            Clip { name: "B".into(), source: ClipSource::Color([0.0, 0.0, 1.0, 1.0]), track: 0, start: 5.0, duration: 5.0, ..Clip::default() },
        ];
        Project { name: "t".into(), width: 16, height: 16, fps: 30.0, duration: 10.0, tracks, clips, transitions: Vec::new() }
    }

    #[test]
    fn add_slide_transition_action_appends() {
        let mut app = App::new();
        app.project = two_clip_project();
        app.time = 5.0;
        let before = app.project.transitions.len();
        app.apply(Action::AddSlideTransition { index: 0, direction: SlideDirection::Left, duration: 1.0 });
        assert_eq!(app.project.transitions.len(), before + 1);
        assert!(matches!(app.project.transitions.last().unwrap().kind, TransitionKind::Slide(SlideDirection::Left)));
    }

    #[test]
    fn add_spin_cube_push_wipe_actions() {
        let mut app = App::new();
        app.project = two_clip_project();
        app.time = 5.0;
        app.apply(Action::AddSpinTransition { index: 0, direction: SpinDirection::Clockwise });
        app.apply(Action::AddCubeFoldTransition { index: 0, direction: CubeDirection::Right });
        app.apply(Action::AddPushTransition { index: 0, direction: WipeDir::Up });
        app.apply(Action::AddWipeTransition2 { index: 0, direction: WipeDir::Down });
        // add_transition replaces same-cut entries, so the last kind wins.
        assert_eq!(app.project.transitions.len(), 1);
        assert!(matches!(app.project.transitions[0].kind, TransitionKind::Wipe(WipeDir::Down)));
    }
}
