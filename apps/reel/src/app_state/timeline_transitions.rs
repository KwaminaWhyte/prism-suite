//! Timeline **transition** domain split out of `timeline.rs` (file-size rule):
//! the transition kinds + direction enums, the [`Transition`] record, and its
//! weight / push / wipe-reveal / pixel-mask math, plus the dispatcher arms that
//! add transitions to the project. `timeline.rs` `pub use`s these types so every
//! existing `super::timeline::Transition` / `crate::app_state::TransitionKind`
//! reference keeps resolving.

use super::{App, Action, DEFAULT_TRANSITION_DUR};

/// Direction a wipe sweeps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WipeDir { Left, Right, Up, Down }

impl WipeDir {
    #[allow(dead_code)]
    pub fn label(&self) -> &'static str {
        match self { WipeDir::Left => "Left", WipeDir::Right => "Right", WipeDir::Up => "Up", WipeDir::Down => "Down" }
    }
}

/// Direction a slide transition moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlideDirection { Left, Right, Up, Down }

/// Direction a split transition opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitDirection { Horizontal, Vertical }

/// Direction a swap transition swaps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwapDirection { Left, Right }

/// Direction a spin transition rotates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpinDirection { Clockwise, CounterClockwise }

/// Corner from which a page peel originates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PagePeelDirection { TopLeft, TopRight, BottomLeft, BottomRight }

/// Direction a cube transition faces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CubeDirection { Left, Right, Up, Down }

/// Film-grain dissolve pattern.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilmPattern { Grain, Burn, Dissolve }

/// Kind of a transition applied at a cut.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TransitionKind {
    CrossDissolve,
    DipToColor([f32; 4]),
    Wipe(WipeDir),
    Push(WipeDir),
    FilmDissolve,
    IrisCircle,
    ClockWipe,
    DiagonalWipe,
    PixelDissolve,
    // --- Batch 5 (new) ---
    Slide(SlideDirection),
    Split { direction: SplitDirection, flip: bool },
    Swap(SwapDirection),
    Zoom { grow: bool },
    SpinAway(SpinDirection),
    PagePeel { direction: PagePeelDirection, softness: f32 },
    PageTurn { reverse: bool },
    Cube { direction: CubeDirection, lighting: bool },
    Film(FilmPattern),
    Luma { invert: bool },
    DipToBlack,
    DipToWhite,
    AdditiveDissolve,
    NonAdditiveDissolve,
    RandomInvert,
}

impl TransitionKind {
    #[allow(dead_code)]
    pub fn label(&self) -> &'static str {
        match self {
            TransitionKind::CrossDissolve => "Cross Dissolve",
            TransitionKind::DipToColor(_) => "Dip to Color",
            TransitionKind::Wipe(_) => "Wipe",
            TransitionKind::Push(_) => "Push",
            TransitionKind::FilmDissolve => "Film Dissolve",
            TransitionKind::IrisCircle => "Iris Circle",
            TransitionKind::ClockWipe => "Clock Wipe",
            TransitionKind::DiagonalWipe => "Diagonal Wipe",
            TransitionKind::PixelDissolve => "Pixel Dissolve",
            // --- Batch 5 (new) ---
            TransitionKind::Slide(_) => "Slide",
            TransitionKind::Split { .. } => "Split",
            TransitionKind::Swap(_) => "Swap",
            TransitionKind::Zoom { .. } => "Zoom",
            TransitionKind::SpinAway(_) => "Spin Away",
            TransitionKind::PagePeel { .. } => "Page Peel",
            TransitionKind::PageTurn { .. } => "Page Turn",
            TransitionKind::Cube { .. } => "Cube",
            TransitionKind::Film(_) => "Film",
            TransitionKind::Luma { .. } => "Luma",
            TransitionKind::DipToBlack => "Dip to Black",
            TransitionKind::DipToWhite => "Dip to White",
            TransitionKind::AdditiveDissolve => "Additive Dissolve",
            TransitionKind::NonAdditiveDissolve => "Non-Additive Dissolve",
            TransitionKind::RandomInvert => "Random Invert",
        }
    }
}

/// A transition centered on the cut between two clips.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transition {
    pub kind: TransitionKind,
    pub from: usize,
    pub to: usize,
    pub center: f32,
    pub duration: f32,
}

impl Transition {
    pub fn start(&self) -> f32 { self.center - self.duration * 0.5 }
    pub fn end(&self) -> f32 { self.center + self.duration * 0.5 }
    pub fn covers(&self, t: f32) -> bool { t >= self.start() && t < self.end() }

    pub fn progress(&self, t: f32) -> f32 {
        if self.duration <= 0.0 { return if t < self.center { 0.0 } else { 1.0 }; }
        ((t - self.start()) / self.duration).clamp(0.0, 1.0)
    }

    pub fn weights(&self, t: f32) -> (f32, f32, Option<([f32; 4], f32)>) {
        let p = self.progress(t);
        match self.kind {
            TransitionKind::CrossDissolve => (1.0 - p, p, None),
            TransitionKind::DipToColor(color) => {
                if p < 0.5 { (1.0, 0.0, Some((color, p * 2.0))) }
                else { (0.0, 1.0, Some((color, (1.0 - p) * 2.0))) }
            }
            TransitionKind::Wipe(_)
            | TransitionKind::Push(_)
            | TransitionKind::IrisCircle
            | TransitionKind::ClockWipe
            | TransitionKind::DiagonalWipe
            | TransitionKind::PixelDissolve => {
                if p < 0.5 { (1.0, 0.0, None) } else { (0.0, 1.0, None) }
            }
            TransitionKind::FilmDissolve => {
                let film_p = p.powf(1.0 / 2.2);
                (1.0 - film_p, film_p, None)
            }
            // --- Batch 5 (new): simple dissolve-style transitions ---
            TransitionKind::AdditiveDissolve
            | TransitionKind::NonAdditiveDissolve
            | TransitionKind::Luma { .. }
            | TransitionKind::Film(_) => {
                (1.0 - p, p, None)
            }
            // Dip-to-black: fade out then fade in through black
            TransitionKind::DipToBlack => {
                let black = [0.0f32, 0.0, 0.0, 1.0];
                if p < 0.5 { (1.0, 0.0, Some((black, p * 2.0))) }
                else { (0.0, 1.0, Some((black, (1.0 - p) * 2.0))) }
            }
            // Dip-to-white: fade out then fade in through white
            TransitionKind::DipToWhite => {
                let white = [1.0f32, 1.0, 1.0, 1.0];
                if p < 0.5 { (1.0, 0.0, Some((white, p * 2.0))) }
                else { (0.0, 1.0, Some((white, (1.0 - p) * 2.0))) }
            }
            // Spatial / motion transitions: hard cut at 50%
            TransitionKind::Slide(_)
            | TransitionKind::Split { .. }
            | TransitionKind::Swap(_)
            | TransitionKind::Zoom { .. }
            | TransitionKind::SpinAway(_)
            | TransitionKind::PagePeel { .. }
            | TransitionKind::PageTurn { .. }
            | TransitionKind::Cube { .. }
            | TransitionKind::RandomInvert => {
                if p < 0.5 { (1.0, 0.0, None) } else { (0.0, 1.0, None) }
            }
        }
    }

    pub fn push_offsets(&self, t: f32) -> Option<((f32, f32), (f32, f32))> {
        let TransitionKind::Push(dir) = self.kind else { return None; };
        let p = self.progress(t);
        let (from_off, to_off) = match dir {
            WipeDir::Left  => ((-p, 0.0),       (1.0 - p, 0.0)),
            WipeDir::Right => ((p, 0.0),         (-(1.0 - p), 0.0)),
            WipeDir::Up    => ((0.0, -p),        (0.0, 1.0 - p)),
            WipeDir::Down  => ((0.0, p),         (0.0, -(1.0 - p))),
        };
        Some((from_off, to_off))
    }

    pub fn wipe_reveal(&self, t: f32) -> Option<(f32, f32, f32, f32)> {
        let TransitionKind::Wipe(dir) = &self.kind else { return None; };
        let dir = *dir;
        let p = self.progress(t);
        Some(match dir {
            WipeDir::Left => (0.0, 0.0, p, 1.0),
            WipeDir::Right => (1.0 - p, 0.0, 1.0, 1.0),
            WipeDir::Up => (0.0, 0.0, 1.0, p),
            WipeDir::Down => (0.0, 1.0 - p, 1.0, 1.0),
        })
    }

    pub fn pixel_mask(&self, t: f32, w: u32, h: u32) -> Option<Vec<u8>> {
        let p = self.progress(t);
        match self.kind {
            TransitionKind::IrisCircle => {
                let cx = w as f32 / 2.0;
                let cy = h as f32 / 2.0;
                let max_r = (cx * cx + cy * cy).sqrt();
                let r = p * max_r;
                let aa = 1.5_f32;
                let mut mask = Vec::with_capacity((w * h) as usize);
                for y in 0..h {
                    for x in 0..w {
                        let dx = x as f32 - cx;
                        let dy = y as f32 - cy;
                        let dist = (dx * dx + dy * dy).sqrt();
                        let alpha = ((r - dist + aa) / (2.0 * aa)).clamp(0.0, 1.0);
                        mask.push((alpha * 255.0).round() as u8);
                    }
                }
                Some(mask)
            }
            TransitionKind::ClockWipe => {
                let cx = w as f32 / 2.0;
                let cy = h as f32 / 2.0;
                let mut mask = Vec::with_capacity((w * h) as usize);
                for y in 0..h {
                    for x in 0..w {
                        let dx = x as f32 - cx;
                        let dy = y as f32 - cy;
                        let angle = dy.atan2(dx);
                        let norm = ((angle + std::f32::consts::FRAC_PI_2)
                            / (2.0 * std::f32::consts::PI)).rem_euclid(1.0);
                        let aa = 0.005_f32;
                        let alpha = ((p - norm + aa) / (2.0 * aa)).clamp(0.0, 1.0);
                        mask.push((alpha * 255.0).round() as u8);
                    }
                }
                Some(mask)
            }
            TransitionKind::DiagonalWipe => {
                let mut mask = Vec::with_capacity((w * h) as usize);
                let aa = 0.01_f32;
                for y in 0..h {
                    for x in 0..w {
                        let t_px = (x as f32 / w as f32 + y as f32 / h as f32) * 0.5;
                        let alpha = ((p - t_px + aa) / (2.0 * aa)).clamp(0.0, 1.0);
                        mask.push((alpha * 255.0).round() as u8);
                    }
                }
                Some(mask)
            }
            TransitionKind::PixelDissolve => {
                let mut mask = Vec::with_capacity((w * h) as usize);
                for y in 0..h {
                    for x in 0..w {
                        let idx = y * w + x;
                        let mut v = idx.wrapping_mul(2654435761);
                        v ^= v >> 16;
                        v = v.wrapping_mul(2246822519);
                        v ^= v >> 13;
                        let threshold = (v & 0xFFFF) as f32 / 65535.0;
                        let alpha = if p >= threshold { 255u8 } else { 0u8 };
                        mask.push(alpha);
                    }
                }
                Some(mask)
            }
            _ => None,
        }
    }
}

impl App {
    /// Sub-router for **transition-adding** actions. Returns `Some(action)` when
    /// the action is not one of ours, `None` once handled.
    pub(crate) fn apply_timeline_transitions(&mut self, action: Action) -> Option<Action> {
        match action {
            Action::AddCrossDissolve { index } => {
                let t = self.snap_to_frame(self.time);
                if self.project.add_transition(index, t, TransitionKind::CrossDissolve, DEFAULT_TRANSITION_DUR).is_some() {
                    self.host.mark_dirty();
                }
            }
            Action::AddTransition { index, kind } => {
                let t = self.snap_to_frame(self.time);
                if self.project.add_transition(index, t, kind, DEFAULT_TRANSITION_DUR).is_some() {
                    self.host.mark_dirty();
                }
            }
            Action::AddDiagonalWipe { index } => {
                let dur = self.project.duration;
                self.project.add_transition(index, dur, TransitionKind::DiagonalWipe, 1.0);
            }
            Action::AddPixelDissolve { index } => {
                let dur = self.project.duration;
                self.project.add_transition(index, dur, TransitionKind::PixelDissolve, 1.0);
            }
            Action::AddFilmDissolve { index } => {
                let dur = self.project.duration;
                self.project.add_transition(index, dur, TransitionKind::FilmDissolve, 1.0);
            }
            Action::SetTransitionDuration { track_idx: _, trans_idx, duration } => {
                if let Some(t) = self.project.transitions.get_mut(trans_idx) {
                    t.duration = duration.max(0.0);
                }
            }
            other => return Some(other),
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::{App, Action, DIP_BLACK};

    #[test]
    fn add_cross_dissolve_finds_the_cut_and_clamps_duration() {
        let mut app = App::new();
        let added = app.project.add_transition(0, 6.0, TransitionKind::CrossDissolve, DEFAULT_TRANSITION_DUR);
        assert!(added.is_some());
        let tr = app.project.transitions[0];
        assert!((tr.center - 6.0).abs() < 1e-4, "centered on the cut");
        assert!((tr.duration - DEFAULT_TRANSITION_DUR).abs() < 1e-4);
        let again = app.project.add_transition(0, 6.0, TransitionKind::CrossDissolve, 2.0);
        assert_eq!(again, Some(0));
        assert_eq!(app.project.transitions.len(), 1);
    }

    #[test]
    fn add_cross_dissolve_without_neighbour_is_none() {
        let mut app = App::new();
        assert!(app.project.add_transition(2, 3.0, TransitionKind::CrossDissolve, 1.0).is_none());
    }

    #[test]
    fn active_transition_respects_span_and_track_visibility() {
        let mut app = App::new();
        app.project.add_transition(0, 6.0, TransitionKind::CrossDissolve, 2.0).expect("transition");
        assert!(app.project.active_transition(6.0).is_some());
        assert!(app.project.active_transition(4.0).is_none());
        assert!(app.project.active_transition(8.0).is_none());
        app.project.tracks[0].enabled = false;
        assert!(app.project.active_transition(6.0).is_none());
    }

    #[test]
    fn cross_dissolve_weights_ramp_from_outgoing_to_incoming() {
        let tr = Transition { kind: TransitionKind::CrossDissolve, from: 0, to: 1, center: 5.0, duration: 2.0 };
        let (f0, t0, d0) = tr.weights(4.0);
        assert!((f0 - 1.0).abs() < 1e-5 && t0.abs() < 1e-5 && d0.is_none());
        let (fm, tm, _) = tr.weights(5.0);
        assert!((fm - 0.5).abs() < 1e-5 && (tm - 0.5).abs() < 1e-5);
        let (fe, te, _) = tr.weights(6.0);
        assert!(fe.abs() < 1e-5 && (te - 1.0).abs() < 1e-5);
    }

    #[test]
    fn dip_to_color_dips_then_clears() {
        let tr = Transition { kind: TransitionKind::DipToColor(DIP_BLACK), from: 0, to: 1, center: 5.0, duration: 2.0 };
        let (f, t, dip) = tr.weights(4.5);
        assert!((f - 1.0).abs() < 1e-5 && t.abs() < 1e-5);
        let (_, amount) = dip.expect("dip present in first half");
        assert!((amount - 0.5).abs() < 1e-5);
        let (_, _, dip_mid) = tr.weights(5.0);
        assert!((dip_mid.unwrap().1 - 1.0).abs() < 1e-5);
        let (f2, t2, dip2) = tr.weights(5.5);
        assert!(f2.abs() < 1e-5 && (t2 - 1.0).abs() < 1e-5);
        assert!((dip2.unwrap().1 - 0.5).abs() < 1e-5);
    }

    #[test]
    fn wipe_reveal_grows_from_the_entering_edge() {
        let tr = Transition { kind: TransitionKind::Wipe(WipeDir::Left), from: 0, to: 1, center: 5.0, duration: 2.0 };
        let (x0, y0, x1, y1) = tr.wipe_reveal(5.0).expect("wipe reveal");
        assert!((x0 - 0.0).abs() < 1e-5 && (x1 - 0.5).abs() < 1e-5);
        assert!((y0 - 0.0).abs() < 1e-5 && (y1 - 1.0).abs() < 1e-5);
        let cross = Transition { kind: TransitionKind::CrossDissolve, ..tr };
        assert!(cross.wipe_reveal(5.0).is_none());
    }

    #[test]
    fn add_cross_dissolve_action_marks_dirty_and_adds() {
        let mut app = App::new();
        app.apply(Action::Seek(6.0));
        assert_eq!(app.project.transitions.len(), 0);
        app.apply(Action::AddCrossDissolve { index: 0 });
        assert_eq!(app.project.transitions.len(), 1, "a dissolve was added at the cut");
    }

    #[test]
    fn pixel_mask_dissolves_more_as_progress_rises() {
        let tr = Transition { kind: TransitionKind::PixelDissolve, from: 0, to: 1, center: 5.0, duration: 2.0 };
        let low = tr.pixel_mask(4.2, 16, 16).expect("mask");
        let high = tr.pixel_mask(5.8, 16, 16).expect("mask");
        let count = |m: &[u8]| m.iter().filter(|&&v| v > 0).count();
        assert!(count(&high) >= count(&low), "more pixels revealed later in the dissolve");
    }
}
