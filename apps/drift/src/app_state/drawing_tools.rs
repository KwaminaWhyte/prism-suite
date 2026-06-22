use super::{App, Action};

/// Pen tool editing mode.
#[derive(Clone, Debug, PartialEq)]
pub enum PenMode {
    Draw,
    Edit,
    AddPoint,
    DeletePoint,
}

/// A Bezier control point used by the interactive pen tool.
/// Distinct from [`super::BezierPoint`] (vector paths), which uses tuple handles.
#[derive(Clone, Debug)]
pub struct PenBezierPoint {
    pub x: f32,
    pub y: f32,
    pub cp_in_x: f32,
    pub cp_in_y: f32,
    pub cp_out_x: f32,
    pub cp_out_y: f32,
    pub smooth: bool,
}

/// Live state of the interactive pen tool (in-progress path editing).
#[derive(Clone, Debug)]
pub struct PenToolState {
    pub active_path_id: Option<usize>,
    pub points: Vec<PenBezierPoint>,
    pub mode: PenMode,
    pub closed: bool,
    pub selected_point: Option<usize>,
}

impl Default for PenToolState {
    fn default() -> Self {
        Self {
            active_path_id: None,
            points: vec![],
            mode: PenMode::Draw,
            closed: false,
            selected_point: None,
        }
    }
}

/// An in-progress or committed freehand pencil stroke.
#[derive(Clone, Debug)]
pub struct PencilStroke {
    pub id: usize,
    pub layer_id: usize,
    pub points: Vec<(f32, f32)>,
    pub color: u32,
    pub width: f32,
    pub smoothing: f32,
    pub committed: bool,
}

/// Which handle on a selection gizmo is active.
#[derive(Clone, Debug, PartialEq)]
pub enum GizmoHandle {
    TopLeft,
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
    Rotate,
    Pivot,
}

/// A transform gizmo overlaid on a selected layer.
#[derive(Clone, Debug)]
pub struct SelectionGizmo {
    pub layer_id: usize,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub rotation: f32,
    pub active_handle: Option<GizmoHandle>,
    pub dragging: bool,
}

impl App {
    pub(super) fn apply_drawing_tools(&mut self, action: Action) {
        match action {
            Action::PenAddPoint { x, y } => {
                let pt = PenBezierPoint {
                    x,
                    y,
                    cp_in_x: x - 20.0,
                    cp_in_y: y,
                    cp_out_x: x + 20.0,
                    cp_out_y: y,
                    smooth: true,
                };
                self.pen_tool.points.push(pt);
            }
            Action::PenSelectPoint { idx } => {
                self.pen_tool.selected_point = Some(idx);
            }
            Action::PenMovePoint { idx, x, y } => {
                if let Some(p) = self.pen_tool.points.get_mut(idx) {
                    p.x = x;
                    p.y = y;
                }
            }
            Action::PenSetHandle { idx, in_x, in_y, out_x, out_y } => {
                if let Some(p) = self.pen_tool.points.get_mut(idx) {
                    p.cp_in_x = in_x;
                    p.cp_in_y = in_y;
                    p.cp_out_x = out_x;
                    p.cp_out_y = out_y;
                }
            }
            Action::PenClosePath => {
                self.pen_tool.closed = true;
            }
            Action::PenCommitPath => {
                self.pen_tool = PenToolState::default();
            }
            Action::PenSetMode { mode } => {
                self.pen_tool.mode = mode;
            }
            Action::PencilBeginStroke { layer_id, color, width } => {
                let id = self.next_pencil_id;
                self.next_pencil_id += 1;
                self.pencil_strokes.push(PencilStroke {
                    id,
                    layer_id,
                    points: vec![],
                    color,
                    width,
                    smoothing: 0.5,
                    committed: false,
                });
                self.active_pencil_stroke = Some(id);
            }
            Action::PencilAddPoint { x, y } => {
                if let Some(id) = self.active_pencil_stroke {
                    if let Some(s) = self.pencil_strokes.iter_mut().find(|s| s.id == id) {
                        s.points.push((x, y));
                    }
                }
            }
            Action::PencilCommitStroke => {
                if let Some(id) = self.active_pencil_stroke {
                    if let Some(s) = self.pencil_strokes.iter_mut().find(|s| s.id == id) {
                        s.committed = true;
                    }
                    self.active_pencil_stroke = None;
                }
            }
            Action::PencilCancelStroke => {
                if let Some(id) = self.active_pencil_stroke {
                    self.pencil_strokes.retain(|s| s.id != id);
                    self.active_pencil_stroke = None;
                }
            }
            Action::SetOnionSkinEnabled { enabled } => {
                self.onion_skin.enabled = enabled;
            }
            Action::SetOnionSkinFrames { prev, next } => {
                self.onion_skin.frames_before = prev;
                self.onion_skin.frames_after = next;
            }
            Action::SetOnionSkinOpacity { prev_opacity, next_opacity } => {
                self.onion_skin.before_alpha = prev_opacity.clamp(0.0, 1.0);
                self.onion_skin.after_alpha = next_opacity.clamp(0.0, 1.0);
            }
            Action::SetSelectionGizmo { layer_id, x, y, w, h } => {
                self.selection_gizmo = Some(SelectionGizmo {
                    layer_id,
                    x,
                    y,
                    w,
                    h,
                    rotation: 0.0,
                    active_handle: None,
                    dragging: false,
                });
            }
            Action::ClearSelectionGizmo => {
                self.selection_gizmo = None;
            }
            Action::GizmoDragHandle { handle } => {
                if let Some(g) = &mut self.selection_gizmo {
                    g.active_handle = Some(handle);
                    g.dragging = true;
                }
            }
            Action::GizmoRelease => {
                if let Some(g) = &mut self.selection_gizmo {
                    g.active_handle = None;
                    g.dragging = false;
                }
            }
            Action::GizmoRotate { delta_deg } => {
                if let Some(g) = &mut self.selection_gizmo {
                    g.rotation = (g.rotation + delta_deg) % 360.0;
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{App, Action};
    use super::{PenMode, GizmoHandle};

    fn app() -> App {
        App::new()
    }

    #[test]
    fn test_pen_add_point_grows() {
        let mut a = app();
        a.apply(Action::PenAddPoint { x: 10.0, y: 20.0 });
        assert_eq!(a.pen_tool.points.len(), 1);
        assert_eq!(a.pen_tool.points[0].x, 10.0);
        assert_eq!(a.pen_tool.points[0].y, 20.0);
    }

    #[test]
    fn test_pen_add_multiple_points() {
        let mut a = app();
        a.apply(Action::PenAddPoint { x: 0.0, y: 0.0 });
        a.apply(Action::PenAddPoint { x: 100.0, y: 50.0 });
        a.apply(Action::PenAddPoint { x: 200.0, y: 0.0 });
        assert_eq!(a.pen_tool.points.len(), 3);
    }

    #[test]
    fn test_pen_move_point_changes_coords() {
        let mut a = app();
        a.apply(Action::PenAddPoint { x: 10.0, y: 10.0 });
        a.apply(Action::PenMovePoint { idx: 0, x: 50.0, y: 75.0 });
        assert_eq!(a.pen_tool.points[0].x, 50.0);
        assert_eq!(a.pen_tool.points[0].y, 75.0);
    }

    #[test]
    fn test_pen_move_point_oob_noop() {
        let mut a = app();
        a.apply(Action::PenAddPoint { x: 10.0, y: 10.0 });
        // index out of bounds — should not panic
        a.apply(Action::PenMovePoint { idx: 99, x: 50.0, y: 75.0 });
        assert_eq!(a.pen_tool.points[0].x, 10.0);
    }

    #[test]
    fn test_pen_close_path_sets_closed() {
        let mut a = app();
        assert!(!a.pen_tool.closed);
        a.apply(Action::PenClosePath);
        assert!(a.pen_tool.closed);
    }

    #[test]
    fn test_pen_commit_resets_state() {
        let mut a = app();
        a.apply(Action::PenAddPoint { x: 0.0, y: 0.0 });
        a.apply(Action::PenClosePath);
        a.apply(Action::PenCommitPath);
        assert!(a.pen_tool.points.is_empty());
        assert!(!a.pen_tool.closed);
        assert_eq!(a.pen_tool.mode, PenMode::Draw);
    }

    #[test]
    fn test_pen_set_mode_changes_mode() {
        let mut a = app();
        assert_eq!(a.pen_tool.mode, PenMode::Draw);
        a.apply(Action::PenSetMode { mode: PenMode::Edit });
        assert_eq!(a.pen_tool.mode, PenMode::Edit);
        a.apply(Action::PenSetMode { mode: PenMode::AddPoint });
        assert_eq!(a.pen_tool.mode, PenMode::AddPoint);
    }

    #[test]
    fn test_pen_set_handle_updates_control_points() {
        let mut a = app();
        a.apply(Action::PenAddPoint { x: 50.0, y: 50.0 });
        a.apply(Action::PenSetHandle { idx: 0, in_x: 10.0, in_y: 15.0, out_x: 90.0, out_y: 85.0 });
        let p = &a.pen_tool.points[0];
        assert_eq!(p.cp_in_x, 10.0);
        assert_eq!(p.cp_in_y, 15.0);
        assert_eq!(p.cp_out_x, 90.0);
        assert_eq!(p.cp_out_y, 85.0);
    }

    #[test]
    fn test_pencil_begin_stroke_pushes_new_stroke() {
        let mut a = app();
        a.apply(Action::PencilBeginStroke { layer_id: 1, color: 0xff0000, width: 2.0 });
        assert_eq!(a.pencil_strokes.len(), 1);
        assert!(a.active_pencil_stroke.is_some());
        assert_eq!(a.pencil_strokes[0].color, 0xff0000);
        assert_eq!(a.pencil_strokes[0].width, 2.0);
    }

    #[test]
    fn test_pencil_add_point_appends_to_active_stroke() {
        let mut a = app();
        a.apply(Action::PencilBeginStroke { layer_id: 1, color: 0, width: 1.0 });
        a.apply(Action::PencilAddPoint { x: 10.0, y: 20.0 });
        a.apply(Action::PencilAddPoint { x: 30.0, y: 40.0 });
        assert_eq!(a.pencil_strokes[0].points.len(), 2);
    }

    #[test]
    fn test_pencil_commit_sets_committed_clears_active() {
        let mut a = app();
        a.apply(Action::PencilBeginStroke { layer_id: 1, color: 0, width: 1.0 });
        a.apply(Action::PencilAddPoint { x: 5.0, y: 5.0 });
        a.apply(Action::PencilCommitStroke);
        assert!(a.pencil_strokes[0].committed);
        assert!(a.active_pencil_stroke.is_none());
    }

    #[test]
    fn test_pencil_cancel_removes_stroke() {
        let mut a = app();
        a.apply(Action::PencilBeginStroke { layer_id: 1, color: 0, width: 1.0 });
        a.apply(Action::PencilCancelStroke);
        assert!(a.pencil_strokes.is_empty());
        assert!(a.active_pencil_stroke.is_none());
    }

    #[test]
    fn test_onion_skin_enabled_toggle() {
        let mut a = app();
        assert!(!a.onion_skin.enabled);
        a.apply(Action::SetOnionSkinEnabled { enabled: true });
        assert!(a.onion_skin.enabled);
        a.apply(Action::SetOnionSkinEnabled { enabled: false });
        assert!(!a.onion_skin.enabled);
    }

    #[test]
    fn test_onion_skin_frames_set() {
        let mut a = app();
        a.apply(Action::SetOnionSkinFrames { prev: 3, next: 5 });
        assert_eq!(a.onion_skin.frames_before, 3);
        assert_eq!(a.onion_skin.frames_after, 5);
    }

    #[test]
    fn test_onion_skin_opacity_clamp() {
        let mut a = app();
        a.apply(Action::SetOnionSkinOpacity { prev_opacity: 1.5, next_opacity: -0.3 });
        assert_eq!(a.onion_skin.before_alpha, 1.0);
        assert_eq!(a.onion_skin.after_alpha, 0.0);
    }

    #[test]
    fn test_selection_gizmo_set() {
        let mut a = app();
        a.apply(Action::SetSelectionGizmo { layer_id: 1, x: 10.0, y: 20.0, w: 100.0, h: 50.0 });
        let g = a.selection_gizmo.as_ref().unwrap();
        assert_eq!(g.layer_id, 1);
        assert_eq!(g.x, 10.0);
        assert_eq!(g.w, 100.0);
    }

    #[test]
    fn test_clear_selection_gizmo() {
        let mut a = app();
        a.apply(Action::SetSelectionGizmo { layer_id: 1, x: 0.0, y: 0.0, w: 50.0, h: 50.0 });
        a.apply(Action::ClearSelectionGizmo);
        assert!(a.selection_gizmo.is_none());
    }

    #[test]
    fn test_gizmo_drag_handle_sets_handle() {
        let mut a = app();
        a.apply(Action::SetSelectionGizmo { layer_id: 1, x: 0.0, y: 0.0, w: 50.0, h: 50.0 });
        a.apply(Action::GizmoDragHandle { handle: GizmoHandle::TopRight });
        let g = a.selection_gizmo.as_ref().unwrap();
        assert_eq!(g.active_handle, Some(GizmoHandle::TopRight));
        assert!(g.dragging);
    }

    #[test]
    fn test_gizmo_release_clears_handle() {
        let mut a = app();
        a.apply(Action::SetSelectionGizmo { layer_id: 1, x: 0.0, y: 0.0, w: 50.0, h: 50.0 });
        a.apply(Action::GizmoDragHandle { handle: GizmoHandle::Rotate });
        a.apply(Action::GizmoRelease);
        let g = a.selection_gizmo.as_ref().unwrap();
        assert!(g.active_handle.is_none());
        assert!(!g.dragging);
    }

    #[test]
    fn test_gizmo_rotate_accumulates() {
        let mut a = app();
        a.apply(Action::SetSelectionGizmo { layer_id: 1, x: 0.0, y: 0.0, w: 50.0, h: 50.0 });
        a.apply(Action::GizmoRotate { delta_deg: 45.0 });
        a.apply(Action::GizmoRotate { delta_deg: 30.0 });
        let g = a.selection_gizmo.as_ref().unwrap();
        assert_eq!(g.rotation, 75.0);
    }

    #[test]
    fn test_pen_select_point() {
        let mut a = app();
        a.apply(Action::PenAddPoint { x: 0.0, y: 0.0 });
        a.apply(Action::PenAddPoint { x: 50.0, y: 50.0 });
        a.apply(Action::PenSelectPoint { idx: 1 });
        assert_eq!(a.pen_tool.selected_point, Some(1));
    }
}
