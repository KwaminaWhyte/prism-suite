use super::{App, Action};

/// A control point on a Bezier path.
#[derive(Clone, Debug, PartialEq)]
pub struct BezierPoint {
    pub x: f32,
    pub y: f32,
    pub in_handle: (f32, f32),
    pub out_handle: (f32, f32),
}

/// A single color stop in a gradient.
#[derive(Clone, Debug, PartialEq)]
pub struct GradientStop {
    pub position: f32,
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

/// Fill style for a vector shape.
#[derive(Clone, Debug, PartialEq)]
pub enum Fill {
    None,
    Solid { r: f32, g: f32, b: f32, a: f32 },
    LinearGradient { stops: Vec<GradientStop>, angle: f32 },
    RadialGradient { stops: Vec<GradientStop>, cx: f32, cy: f32, radius: f32 },
}

/// Stroke cap style.
#[derive(Clone, Debug, PartialEq)]
pub enum StrokeCap {
    Butt,
    Round,
    Square,
}

/// Stroke join style.
#[derive(Clone, Debug, PartialEq)]
pub enum StrokeJoin {
    Miter,
    Round,
    Bevel,
}

/// Stroke style for a vector shape.
#[derive(Clone, Debug, PartialEq)]
pub struct Stroke {
    pub width: f32,
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
    pub cap: StrokeCap,
    pub join: StrokeJoin,
}

impl Stroke {
    pub fn default() -> Self {
        Self {
            width: 1.0,
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
            cap: StrokeCap::Butt,
            join: StrokeJoin::Miter,
        }
    }
}

/// A vector path on a layer.
#[derive(Clone, Debug)]
pub struct VectorPath {
    pub id: usize,
    pub layer_id: usize,
    pub points: Vec<BezierPoint>,
    pub fill: Fill,
    pub stroke: Stroke,
    pub closed: bool,
}

impl VectorPath {
    /// Returns `Some((x, y, w, h))` if this looks like an axis-aligned rectangle
    /// (4 closed points where all bezier handles coincide with their anchors).
    pub fn as_rect(&self) -> Option<(f32, f32, f32, f32)> {
        if self.points.len() == 4 && self.closed {
            let all_linear = self.points.iter().all(|p| {
                (p.in_handle.0 - p.x).abs() < 0.001
                    && (p.in_handle.1 - p.y).abs() < 0.001
                    && (p.out_handle.0 - p.x).abs() < 0.001
                    && (p.out_handle.1 - p.y).abs() < 0.001
            });
            if all_linear {
                let (min_x, max_x, min_y, max_y) = self.point_bounds();
                return Some((min_x, min_y, max_x - min_x, max_y - min_y));
            }
        }
        None
    }

    /// Returns `true` if this looks like a bezier-approximated ellipse (4 curved points).
    pub fn is_ellipse(&self) -> bool {
        self.points.len() == 4
            && self.closed
            && self.points.iter().any(|p| {
                (p.in_handle.0 - p.x).abs() > 0.001 || (p.in_handle.1 - p.y).abs() > 0.001
            })
    }

    /// Axis-aligned bounding box `(min_x, min_y, width, height)`.
    pub fn bbox(&self) -> (f32, f32, f32, f32) {
        if self.points.is_empty() {
            return (0.0, 0.0, 10.0, 10.0);
        }
        let (min_x, max_x, min_y, max_y) = self.point_bounds();
        (min_x, min_y, max_x - min_x, max_y - min_y)
    }

    fn point_bounds(&self) -> (f32, f32, f32, f32) {
        let min_x = self.points.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
        let max_x = self.points.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max);
        let min_y = self.points.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
        let max_y = self.points.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max);
        (min_x, max_x, min_y, max_y)
    }
}

impl App {
    pub fn apply_vector(&mut self, action: Action) {
        match action {
            Action::AddVectorPath { layer_id, points, fill, stroke, closed } => {
                let id = self.next_path_id;
                self.next_path_id += 1;
                self.vector_paths.push(VectorPath {
                    id,
                    layer_id,
                    points,
                    fill,
                    stroke,
                    closed,
                });
            }
            Action::RemoveVectorPath { path_id } => {
                self.vector_paths.retain(|p| p.id != path_id);
            }
            Action::UpdatePathFill { path_id, fill } => {
                if let Some(p) = self.vector_paths.iter_mut().find(|p| p.id == path_id) {
                    p.fill = fill;
                }
            }
            Action::UpdatePathStroke { path_id, stroke } => {
                if let Some(p) = self.vector_paths.iter_mut().find(|p| p.id == path_id) {
                    p.stroke = stroke;
                }
            }
            Action::AddBezierPoint { path_id, point } => {
                if let Some(p) = self.vector_paths.iter_mut().find(|p| p.id == path_id) {
                    p.points.push(point);
                }
            }
            Action::ClosePath { path_id } => {
                if let Some(p) = self.vector_paths.iter_mut().find(|p| p.id == path_id) {
                    p.closed = true;
                }
            }
            Action::AddRectangle { layer_id, x, y, width, height, fill, stroke } => {
                let id = self.next_path_id;
                self.next_path_id += 1;
                // Rectangle: 4 corner points in order TL, TR, BR, BL
                let points = vec![
                    BezierPoint { x, y, in_handle: (x, y), out_handle: (x, y) },
                    BezierPoint { x: x + width, y, in_handle: (x + width, y), out_handle: (x + width, y) },
                    BezierPoint { x: x + width, y: y + height, in_handle: (x + width, y + height), out_handle: (x + width, y + height) },
                    BezierPoint { x, y: y + height, in_handle: (x, y + height), out_handle: (x, y + height) },
                ];
                self.vector_paths.push(VectorPath {
                    id,
                    layer_id,
                    points,
                    fill,
                    stroke,
                    closed: true,
                });
            }
            Action::AddEllipse { layer_id, cx, cy, rx, ry, fill, stroke } => {
                let id = self.next_path_id;
                self.next_path_id += 1;
                // Approximate ellipse with 4 bezier points
                let k = 0.5522847498; // magic number for circle approximation
                let points = vec![
                    BezierPoint { x: cx, y: cy - ry, in_handle: (cx - rx * k, cy - ry), out_handle: (cx + rx * k, cy - ry) },
                    BezierPoint { x: cx + rx, y: cy, in_handle: (cx + rx, cy - ry * k), out_handle: (cx + rx, cy + ry * k) },
                    BezierPoint { x: cx, y: cy + ry, in_handle: (cx + rx * k, cy + ry), out_handle: (cx - rx * k, cy + ry) },
                    BezierPoint { x: cx - rx, y: cy, in_handle: (cx - rx, cy + ry * k), out_handle: (cx - rx, cy - ry * k) },
                ];
                self.vector_paths.push(VectorPath {
                    id,
                    layer_id,
                    points,
                    fill,
                    stroke,
                    closed: true,
                });
            }
            Action::AddLine { layer_id, x1, y1, x2, y2, stroke } => {
                let id = self.next_path_id;
                self.next_path_id += 1;
                let points = vec![
                    BezierPoint { x: x1, y: y1, in_handle: (x1, y1), out_handle: (x1, y1) },
                    BezierPoint { x: x2, y: y2, in_handle: (x2, y2), out_handle: (x2, y2) },
                ];
                self.vector_paths.push(VectorPath {
                    id,
                    layer_id,
                    points,
                    fill: Fill::None,
                    stroke,
                    closed: false,
                });
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{App, Action};
    use super::super::LayerKind;
    use super::{BezierPoint, Fill, Stroke, StrokeCap, StrokeJoin, GradientStop};

    fn app() -> App {
        App::new()
    }

    fn default_fill() -> Fill {
        Fill::Solid { r: 1.0, g: 0.0, b: 0.0, a: 1.0 }
    }

    fn default_stroke() -> Stroke {
        Stroke::default()
    }

    fn pt(x: f32, y: f32) -> BezierPoint {
        BezierPoint { x, y, in_handle: (x, y), out_handle: (x, y) }
    }

    #[test]
    fn test_add_vector_path() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Vec".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::AddVectorPath {
            layer_id: lid,
            points: vec![pt(0.0, 0.0), pt(100.0, 0.0), pt(100.0, 100.0)],
            fill: default_fill(),
            stroke: default_stroke(),
            closed: false,
        });
        assert_eq!(a.vector_paths.len(), 1);
        assert_eq!(a.vector_paths[0].layer_id, lid);
        assert_eq!(a.vector_paths[0].points.len(), 3);
    }

    #[test]
    fn test_add_vector_path_id_increments() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Vec".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::AddVectorPath {
            layer_id: lid,
            points: vec![pt(0.0, 0.0)],
            fill: Fill::None,
            stroke: default_stroke(),
            closed: false,
        });
        a.apply(Action::AddVectorPath {
            layer_id: lid,
            points: vec![pt(10.0, 10.0)],
            fill: Fill::None,
            stroke: default_stroke(),
            closed: false,
        });
        assert_ne!(a.vector_paths[0].id, a.vector_paths[1].id);
    }

    #[test]
    fn test_remove_vector_path() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Vec".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::AddVectorPath {
            layer_id: lid,
            points: vec![pt(0.0, 0.0)],
            fill: Fill::None,
            stroke: default_stroke(),
            closed: false,
        });
        let path_id = a.vector_paths[0].id;
        a.apply(Action::RemoveVectorPath { path_id });
        assert!(a.vector_paths.is_empty());
    }

    #[test]
    fn test_update_path_fill() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Vec".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::AddVectorPath {
            layer_id: lid,
            points: vec![pt(0.0, 0.0)],
            fill: Fill::None,
            stroke: default_stroke(),
            closed: false,
        });
        let path_id = a.vector_paths[0].id;
        a.apply(Action::UpdatePathFill {
            path_id,
            fill: Fill::Solid { r: 0.0, g: 1.0, b: 0.0, a: 1.0 },
        });
        assert_eq!(a.vector_paths[0].fill, Fill::Solid { r: 0.0, g: 1.0, b: 0.0, a: 1.0 });
    }

    #[test]
    fn test_update_path_stroke() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Vec".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::AddVectorPath {
            layer_id: lid,
            points: vec![pt(0.0, 0.0)],
            fill: Fill::None,
            stroke: default_stroke(),
            closed: false,
        });
        let path_id = a.vector_paths[0].id;
        let new_stroke = Stroke { width: 5.0, r: 1.0, g: 0.0, b: 0.0, a: 1.0, cap: StrokeCap::Round, join: StrokeJoin::Round };
        a.apply(Action::UpdatePathStroke { path_id, stroke: new_stroke.clone() });
        assert_eq!(a.vector_paths[0].stroke.width, 5.0);
        assert_eq!(a.vector_paths[0].stroke.cap, StrokeCap::Round);
    }

    #[test]
    fn test_add_bezier_point() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Vec".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::AddVectorPath {
            layer_id: lid,
            points: vec![pt(0.0, 0.0)],
            fill: Fill::None,
            stroke: default_stroke(),
            closed: false,
        });
        let path_id = a.vector_paths[0].id;
        a.apply(Action::AddBezierPoint { path_id, point: pt(50.0, 50.0) });
        assert_eq!(a.vector_paths[0].points.len(), 2);
        assert_eq!(a.vector_paths[0].points[1].x, 50.0);
    }

    #[test]
    fn test_close_path() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Vec".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::AddVectorPath {
            layer_id: lid,
            points: vec![pt(0.0, 0.0), pt(100.0, 0.0)],
            fill: Fill::None,
            stroke: default_stroke(),
            closed: false,
        });
        let path_id = a.vector_paths[0].id;
        assert!(!a.vector_paths[0].closed);
        a.apply(Action::ClosePath { path_id });
        assert!(a.vector_paths[0].closed);
    }

    #[test]
    fn test_add_rectangle() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Vec".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::AddRectangle {
            layer_id: lid,
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 50.0,
            fill: default_fill(),
            stroke: default_stroke(),
        });
        assert_eq!(a.vector_paths.len(), 1);
        assert!(a.vector_paths[0].closed);
        assert_eq!(a.vector_paths[0].points.len(), 4);
        assert_eq!(a.vector_paths[0].points[0].x, 10.0);
        assert_eq!(a.vector_paths[0].points[0].y, 20.0);
    }

    #[test]
    fn test_add_ellipse() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Vec".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::AddEllipse {
            layer_id: lid,
            cx: 50.0,
            cy: 50.0,
            rx: 30.0,
            ry: 20.0,
            fill: default_fill(),
            stroke: default_stroke(),
        });
        assert_eq!(a.vector_paths.len(), 1);
        assert!(a.vector_paths[0].closed);
        assert_eq!(a.vector_paths[0].points.len(), 4);
    }

    #[test]
    fn test_add_line() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Vec".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::AddLine {
            layer_id: lid,
            x1: 0.0,
            y1: 0.0,
            x2: 100.0,
            y2: 100.0,
            stroke: default_stroke(),
        });
        assert_eq!(a.vector_paths.len(), 1);
        assert!(!a.vector_paths[0].closed);
        assert_eq!(a.vector_paths[0].points.len(), 2);
        assert_eq!(a.vector_paths[0].fill, Fill::None);
    }

    #[test]
    fn test_fill_linear_gradient() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Vec".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        let gradient_fill = Fill::LinearGradient {
            stops: vec![
                GradientStop { position: 0.0, r: 1.0, g: 0.0, b: 0.0, a: 1.0 },
                GradientStop { position: 1.0, r: 0.0, g: 0.0, b: 1.0, a: 1.0 },
            ],
            angle: 45.0,
        };
        a.apply(Action::AddVectorPath {
            layer_id: lid,
            points: vec![pt(0.0, 0.0)],
            fill: gradient_fill.clone(),
            stroke: default_stroke(),
            closed: false,
        });
        assert_eq!(a.vector_paths[0].fill, gradient_fill);
    }

    #[test]
    fn test_fill_radial_gradient() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Vec".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        let radial = Fill::RadialGradient {
            stops: vec![GradientStop { position: 0.0, r: 1.0, g: 1.0, b: 0.0, a: 1.0 }],
            cx: 50.0,
            cy: 50.0,
            radius: 100.0,
        };
        a.apply(Action::AddVectorPath {
            layer_id: lid,
            points: vec![pt(0.0, 0.0)],
            fill: radial.clone(),
            stroke: default_stroke(),
            closed: false,
        });
        assert_eq!(a.vector_paths[0].fill, radial);
    }

    #[test]
    fn test_stroke_cap_square() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Vec".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        let stroke = Stroke { width: 2.0, r: 0.0, g: 0.0, b: 0.0, a: 1.0, cap: StrokeCap::Square, join: StrokeJoin::Bevel };
        a.apply(Action::AddVectorPath {
            layer_id: lid,
            points: vec![pt(0.0, 0.0)],
            fill: Fill::None,
            stroke,
            closed: false,
        });
        assert_eq!(a.vector_paths[0].stroke.cap, StrokeCap::Square);
        assert_eq!(a.vector_paths[0].stroke.join, StrokeJoin::Bevel);
    }

    #[test]
    fn test_path_id_monotonic_across_shapes() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Vec".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::AddRectangle { layer_id: lid, x: 0.0, y: 0.0, width: 10.0, height: 10.0, fill: Fill::None, stroke: default_stroke() });
        a.apply(Action::AddEllipse { layer_id: lid, cx: 5.0, cy: 5.0, rx: 3.0, ry: 3.0, fill: Fill::None, stroke: default_stroke() });
        a.apply(Action::AddLine { layer_id: lid, x1: 0.0, y1: 0.0, x2: 10.0, y2: 10.0, stroke: default_stroke() });
        assert_eq!(a.vector_paths.len(), 3);
        assert!(a.vector_paths[0].id < a.vector_paths[1].id);
        assert!(a.vector_paths[1].id < a.vector_paths[2].id);
    }
}
