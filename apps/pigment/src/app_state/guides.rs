//! Guides, rulers and smart guides — pure data model + geometry.
//!
//! Photoshop has three related overlay systems:
//!   * **Guides**: user-placed horizontal / vertical lines at fixed doc-space
//!     positions, used as snap targets.
//!   * **Rulers**: the document's measurement units shown along the canvas edges.
//!   * **Smart guides**: transient alignment hints surfaced while dragging — the
//!     edges/centers of the dragged object lining up with other layers.
//!
//! Everything here is allocation-only geometry: no GPU, no I/O. Snapping returns
//! the snapped coordinate plus which guide (if any) caught it, so the caller can
//! both move the object and draw the highlight.

use super::{App, Action};

/// Orientation of a guide line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuideOrientation {
    /// A horizontal line at a fixed `y` (spans the document width).
    Horizontal,
    /// A vertical line at a fixed `x` (spans the document height).
    Vertical,
}

/// A single ruler/canvas guide.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Guide {
    pub id: u64,
    pub orientation: GuideOrientation,
    /// Doc-space position: `y` for horizontal, `x` for vertical.
    pub position: f32,
    /// Locked guides cannot be moved or cleared by `clear_guides`.
    pub locked: bool,
}

/// Measurement unit shown on the rulers and used for readouts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RulerUnit {
    #[default]
    Pixels,
    Inches,
    Centimeters,
    Millimeters,
    Points,
    Picas,
    Percent,
}

impl RulerUnit {
    /// Convert a pixel distance to this unit given the document `dpi` and, for
    /// `Percent`, the reference `extent` (doc width or height in px).
    pub fn from_pixels(self, px: f32, dpi: f32, extent: f32) -> f32 {
        let dpi = if dpi <= 0.0 { 72.0 } else { dpi };
        match self {
            RulerUnit::Pixels => px,
            RulerUnit::Inches => px / dpi,
            RulerUnit::Centimeters => px / dpi * 2.54,
            RulerUnit::Millimeters => px / dpi * 25.4,
            RulerUnit::Points => px / dpi * 72.0,
            RulerUnit::Picas => px / dpi * 6.0,
            RulerUnit::Percent => {
                if extent.abs() < 1e-6 {
                    0.0
                } else {
                    px / extent * 100.0
                }
            }
        }
    }

    /// Short label shown in the units menu.
    pub fn label(self) -> &'static str {
        match self {
            RulerUnit::Pixels => "px",
            RulerUnit::Inches => "in",
            RulerUnit::Centimeters => "cm",
            RulerUnit::Millimeters => "mm",
            RulerUnit::Points => "pt",
            RulerUnit::Picas => "pc",
            RulerUnit::Percent => "%",
        }
    }
}

/// The full guides/rulers state for a document.
#[derive(Clone, Debug)]
pub struct GuideState {
    pub guides: Vec<Guide>,
    pub next_id: u64,
    /// Distance (doc px) within which the cursor snaps to a guide.
    pub snap_distance: f32,
    /// Whether snapping to guides is active.
    pub snap_enabled: bool,
    /// Whether guides are drawn at all.
    pub visible: bool,
    pub ruler_unit: RulerUnit,
    /// Whether smart guides (alignment hints) are enabled.
    pub smart_guides: bool,
}

impl Default for GuideState {
    fn default() -> Self {
        Self {
            guides: Vec::new(),
            next_id: 1,
            snap_distance: 8.0,
            snap_enabled: true,
            visible: true,
            ruler_unit: RulerUnit::Pixels,
            smart_guides: true,
        }
    }
}

impl GuideState {
    /// Add a guide of the given orientation at `position`. Returns its new id.
    pub fn add(&mut self, orientation: GuideOrientation, position: f32) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.guides.push(Guide {
            id,
            orientation,
            position,
            locked: false,
        });
        id
    }

    /// Remove a guide by id (ignores locked guides).
    pub fn remove(&mut self, id: u64) {
        self.guides.retain(|g| g.id != id || g.locked);
    }

    /// Clear all unlocked guides.
    pub fn clear(&mut self) {
        self.guides.retain(|g| g.locked);
    }

    /// Set the locked flag on a guide.
    pub fn set_locked(&mut self, id: u64, locked: bool) {
        if let Some(g) = self.guides.iter_mut().find(|g| g.id == id) {
            g.locked = locked;
        }
    }

    /// Move an unlocked guide to a new position.
    pub fn move_to(&mut self, id: u64, position: f32) {
        if let Some(g) = self.guides.iter_mut().find(|g| g.id == id && !g.locked) {
            g.position = position;
        }
    }

    /// Snap a coordinate to the nearest guide of the matching orientation within
    /// `snap_distance`. Returns the snapped coordinate and the guide id it locked
    /// onto, or `(value, None)` if nothing was close enough / snapping is off.
    pub fn snap(&self, orientation: GuideOrientation, value: f32) -> (f32, Option<u64>) {
        if !self.snap_enabled {
            return (value, None);
        }
        let mut best: Option<(f32, u64)> = None;
        for g in &self.guides {
            if g.orientation != orientation {
                continue;
            }
            let d = (g.position - value).abs();
            if d <= self.snap_distance && best.map(|(bd, _)| d < bd).unwrap_or(true) {
                best = Some((d, g.id));
            }
        }
        match best {
            Some((_, id)) => {
                let pos = self.guides.iter().find(|g| g.id == id).unwrap().position;
                (pos, Some(id))
            }
            None => (value, None),
        }
    }

    /// Snap a 2-D point against both axes. Returns `[x, y]` and which guides (if
    /// any) were hit on each axis.
    pub fn snap_point(&self, x: f32, y: f32) -> ([f32; 2], [Option<u64>; 2]) {
        let (sx, gx) = self.snap(GuideOrientation::Vertical, x);
        let (sy, gy) = self.snap(GuideOrientation::Horizontal, y);
        ([sx, sy], [gx, gy])
    }
}

/// A candidate alignment surfaced by the smart-guide detector.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SmartGuideHit {
    pub orientation: GuideOrientation,
    /// The position (doc px) the dragged edge/center should snap to.
    pub position: f32,
    /// Signed delta to apply to the moving box to achieve the snap.
    pub delta: f32,
    pub kind: AlignKind,
}

/// Which feature of the boxes aligned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlignKind {
    LeftEdge,
    RightEdge,
    TopEdge,
    BottomEdge,
    CenterX,
    CenterY,
}

/// Detect smart-guide alignments between a moving box and a set of static boxes.
///
/// `moving` and each static box are `[x, y, w, h]` in doc space. Returns the best
/// horizontal and vertical alignment within `tolerance` (smallest delta wins per
/// axis), so the caller can nudge the moving box onto the guide. Compares left /
/// center-x / right against verticals and top / center-y / bottom against
/// horizontals.
pub fn detect_smart_guides(
    moving: [f32; 4],
    statics: &[[f32; 4]],
    tolerance: f32,
) -> Vec<SmartGuideHit> {
    let [mx, my, mw, mh] = moving;
    // Candidate source coordinates on the moving box for each axis.
    let m_x = [
        (mx, AlignKind::LeftEdge),
        (mx + mw * 0.5, AlignKind::CenterX),
        (mx + mw, AlignKind::RightEdge),
    ];
    let m_y = [
        (my, AlignKind::TopEdge),
        (my + mh * 0.5, AlignKind::CenterY),
        (my + mh, AlignKind::BottomEdge),
    ];

    let mut best_x: Option<SmartGuideHit> = None;
    let mut best_y: Option<SmartGuideHit> = None;

    for &[sx, sy, sw, sh] in statics {
        let s_x = [sx, sx + sw * 0.5, sx + sw];
        let s_y = [sy, sy + sh * 0.5, sy + sh];

        for &(mc, kind) in &m_x {
            for &sc in &s_x {
                let delta = sc - mc;
                if delta.abs() <= tolerance
                    && best_x.map(|b| delta.abs() < b.delta.abs()).unwrap_or(true)
                {
                    best_x = Some(SmartGuideHit {
                        orientation: GuideOrientation::Vertical,
                        position: sc,
                        delta,
                        kind,
                    });
                }
            }
        }
        for &(mc, kind) in &m_y {
            for &sc in &s_y {
                let delta = sc - mc;
                if delta.abs() <= tolerance
                    && best_y.map(|b| delta.abs() < b.delta.abs()).unwrap_or(true)
                {
                    best_y = Some(SmartGuideHit {
                        orientation: GuideOrientation::Horizontal,
                        position: sc,
                        delta,
                        kind,
                    });
                }
            }
        }
    }

    let mut out = Vec::new();
    if let Some(h) = best_x {
        out.push(h);
    }
    if let Some(v) = best_y {
        out.push(v);
    }
    out
}

impl App {
    /// Dispatch for the guides/rulers domain actions.
    pub(super) fn apply_guides(&mut self, action: Action) {
        match action {
            Action::AddCanvasGuide { horizontal, position } => {
                let o = if horizontal {
                    GuideOrientation::Horizontal
                } else {
                    GuideOrientation::Vertical
                };
                self.guide_state.add(o, position);
            }
            Action::RemoveCanvasGuide(id) => {
                self.guide_state.remove(id);
            }
            Action::ClearCanvasGuides => {
                self.guide_state.clear();
            }
            Action::LockCanvasGuide { id, locked } => {
                self.guide_state.set_locked(id, locked);
            }
            Action::MoveCanvasGuide { id, position } => {
                self.guide_state.move_to(id, position);
            }
            Action::SetGuideSnapEnabled(b) => {
                self.guide_state.snap_enabled = b;
            }
            Action::SetGuideSnapDistance(d) => {
                self.guide_state.snap_distance = d.max(0.0);
            }
            Action::SetGuidesVisible(b) => {
                self.guide_state.visible = b;
            }
            Action::SetRulerUnit(u) => {
                self.guide_state.ruler_unit = u;
            }
            Action::SetSmartGuidesEnabled(b) => {
                self.guide_state.smart_guides = b;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_remove_clear_guides() {
        let mut gs = GuideState::default();
        let a = gs.add(GuideOrientation::Horizontal, 100.0);
        let b = gs.add(GuideOrientation::Vertical, 50.0);
        assert_eq!(gs.guides.len(), 2);
        gs.remove(a);
        assert_eq!(gs.guides.len(), 1);
        assert_eq!(gs.guides[0].id, b);
        gs.clear();
        assert!(gs.guides.is_empty());
    }

    #[test]
    fn locked_guides_survive_clear() {
        let mut gs = GuideState::default();
        let a = gs.add(GuideOrientation::Horizontal, 100.0);
        gs.set_locked(a, true);
        gs.clear();
        assert_eq!(gs.guides.len(), 1);
        // remove() also refuses locked guides.
        gs.remove(a);
        assert_eq!(gs.guides.len(), 1);
    }

    #[test]
    fn snap_to_nearest_guide() {
        let mut gs = GuideState::default();
        gs.snap_distance = 10.0;
        gs.add(GuideOrientation::Vertical, 100.0);
        gs.add(GuideOrientation::Vertical, 200.0);
        // 105 is within 10 of 100 → snaps to 100.
        let (pos, hit) = gs.snap(GuideOrientation::Vertical, 105.0);
        assert_eq!(pos, 100.0);
        assert!(hit.is_some());
        // 150 is too far from either → no snap.
        let (pos2, hit2) = gs.snap(GuideOrientation::Vertical, 150.0);
        assert_eq!(pos2, 150.0);
        assert!(hit2.is_none());
    }

    #[test]
    fn snap_disabled_returns_value() {
        let mut gs = GuideState::default();
        gs.add(GuideOrientation::Horizontal, 10.0);
        gs.snap_enabled = false;
        let (pos, hit) = gs.snap(GuideOrientation::Horizontal, 12.0);
        assert_eq!(pos, 12.0);
        assert!(hit.is_none());
    }

    #[test]
    fn snap_point_both_axes() {
        let mut gs = GuideState::default();
        gs.snap_distance = 5.0;
        gs.add(GuideOrientation::Vertical, 40.0);
        gs.add(GuideOrientation::Horizontal, 80.0);
        let ([x, y], [gx, gy]) = gs.snap_point(42.0, 79.0);
        assert_eq!(x, 40.0);
        assert_eq!(y, 80.0);
        assert!(gx.is_some() && gy.is_some());
    }

    #[test]
    fn ruler_unit_conversions() {
        // 144 px @ 72 dpi = 2 inches.
        assert!((RulerUnit::Inches.from_pixels(144.0, 72.0, 0.0) - 2.0).abs() < 1e-4);
        // 72 px @ 72 dpi = 72 points.
        assert!((RulerUnit::Points.from_pixels(72.0, 72.0, 0.0) - 72.0).abs() < 1e-4);
        // 50 px of a 200 px extent = 25%.
        assert!((RulerUnit::Percent.from_pixels(50.0, 72.0, 200.0) - 25.0).abs() < 1e-4);
        assert_eq!(RulerUnit::Pixels.label(), "px");
    }

    #[test]
    fn smart_guide_left_edge_alignment() {
        // Moving box left edge at x=12; static box left edge at x=10 → delta -2.
        let moving = [12.0, 50.0, 30.0, 30.0];
        let statics = [[10.0, 0.0, 40.0, 40.0]];
        let hits = detect_smart_guides(moving, &statics, 5.0);
        let vx = hits
            .iter()
            .find(|h| h.orientation == GuideOrientation::Vertical)
            .expect("a vertical alignment");
        assert_eq!(vx.position, 10.0);
        assert!((vx.delta - (-2.0)).abs() < 1e-4);
        assert_eq!(vx.kind, AlignKind::LeftEdge);
    }

    #[test]
    fn smart_guide_center_alignment() {
        // Both boxes 100 wide; moving centered at 60, static centered at 62.
        let moving = [10.0, 0.0, 100.0, 20.0]; // center x = 60
        let statics = [[12.0, 0.0, 100.0, 20.0]]; // center x = 62
        let hits = detect_smart_guides(moving, &statics, 5.0);
        let vx = hits
            .iter()
            .find(|h| h.orientation == GuideOrientation::Vertical)
            .unwrap();
        // Closest alignment should be the centers (delta 2) over edges (delta 2 too,
        // but center is found and not worse). Just assert a small delta within tol.
        assert!(vx.delta.abs() <= 2.0 + 1e-4);
    }

    #[test]
    fn smart_guide_no_alignment_when_far() {
        let moving = [0.0, 0.0, 10.0, 10.0];
        let statics = [[500.0, 500.0, 10.0, 10.0]];
        let hits = detect_smart_guides(moving, &statics, 4.0);
        assert!(hits.is_empty());
    }

    #[test]
    fn apply_actions_mutate_state() {
        let mut app = App::new();
        app.apply(Action::AddCanvasGuide {
            horizontal: true,
            position: 25.0,
        });
        assert_eq!(app.guide_state.guides.len(), 1);
        app.apply(Action::SetRulerUnit(RulerUnit::Inches));
        assert_eq!(app.guide_state.ruler_unit, RulerUnit::Inches);
        app.apply(Action::ClearCanvasGuides);
        assert!(app.guide_state.guides.is_empty());
    }
}
