//! The apply layer for the Distort & Transform + Offset Path family: wires the
//! pure geometry functions in [`super`] onto `impl App`, mapping each transform
//! over the selected shape's contours with one undo checkpoint. This is the
//! **terminal** stage of the `Action` dispatch chain (reached from
//! `apply_textfield`'s catch-all); its own `_ => {}` is the chain's final no-op.

use super::{offset_path, pucker_bloat, roughen, transform_each_affine, twist, zigzag};
use crate::app_state::{Action, App};
use crate::document::{Shape, SubPath};

impl App {
    /// Terminal dispatcher for the Distort & Transform + Offset Path actions.
    /// Reached from `apply_textfield`'s catch-all; owns the chain's final no-op.
    pub(in crate::app_state) fn apply_path_distort(&mut self, action: Action) {
        match action {
            Action::DistortOffsetPath {
                shape_id,
                distance,
                join,
            } => self.distort_contours(shape_id, |p, h, c| offset_path(p, h, c, distance, join)),
            Action::DistortRoughen {
                shape_id,
                size,
                detail,
                seed,
                smooth,
            } => self.distort_contours(shape_id, |p, h, c| {
                roughen(p, h, c, size, detail, seed, smooth)
            }),
            Action::DistortZigZag {
                shape_id,
                size,
                ridges,
                smooth,
            } => self.distort_contours(shape_id, |p, h, c| zigzag(p, h, c, size, ridges, smooth)),
            Action::DistortPuckerBloat { shape_id, amount } => {
                self.distort_contours(shape_id, |p, h, c| pucker_bloat(p, h, c, amount))
            }
            Action::DistortTwist {
                shape_id,
                angle_deg,
            } => self.distort_contours(shape_id, |p, h, c| twist(p, h, c, angle_deg)),
            Action::DistortTransformEach {
                shape_id,
                move_x,
                move_y,
                scale_x,
                scale_y,
                angle_deg,
                copies,
            } => self.distort_transform_each(
                shape_id, move_x, move_y, scale_x, scale_y, angle_deg, copies,
            ),
            _ => {}
        }
    }

    /// Replace the shape at `shape_id` with the result of mapping the per-contour
    /// transform `f` over its geometry (converting Rect / Ellipse / Text to a
    /// path first). One undo step; no-op for an out-of-range id or empty result.
    fn distort_contours<F>(&mut self, shape_id: usize, f: F)
    where
        F: Fn(&[(f32, f32)], &[(f32, f32)], bool) -> (Vec<(f32, f32)>, Vec<(f32, f32)>),
    {
        if shape_id >= self.doc.shapes.len() {
            return;
        }
        let path = self.doc.shapes[shape_id].to_path();
        let Some(new_shape) = distort_shape(&path, &f) else {
            return;
        };
        self.checkpoint();
        self.doc.shapes[shape_id] = new_shape;
        self.host.mark_dirty();
    }

    /// Transform Each (with optional replication): transform the shape about its
    /// own bbox centre by the per-copy affine. With `copies == 0` it transforms in
    /// place; otherwise it leaves the source untouched and inserts `copies`
    /// cumulatively-transformed duplicates right after it. One undo step.
    #[allow(clippy::too_many_arguments)]
    fn distort_transform_each(
        &mut self,
        shape_id: usize,
        move_x: f32,
        move_y: f32,
        scale_x: f32,
        scale_y: f32,
        angle_deg: f32,
        copies: usize,
    ) {
        if shape_id >= self.doc.shapes.len() {
            return;
        }
        let Some(b) = self.doc.shapes[shape_id].bounds() else {
            return;
        };
        let (cx, cy) = (b.x + b.w * 0.5, b.y + b.h * 0.5);
        let step = transform_each_affine(move_x, move_y, scale_x, scale_y, angle_deg, cx, cy);
        if step.is_identity() && copies == 0 {
            return;
        }
        self.checkpoint();
        if copies == 0 {
            self.doc.shapes[shape_id].apply_affine(&step);
        } else {
            let base = self.doc.shapes[shape_id].clone();
            let mut aff = step;
            let mut new_shapes: Vec<Shape> = Vec::with_capacity(copies);
            for _ in 0..copies {
                let mut c = base.clone();
                c.apply_affine(&aff);
                new_shapes.push(c);
                aff = aff.then(step);
            }
            let at = shape_id + 1;
            for (i, s) in new_shapes.into_iter().enumerate() {
                self.doc.shapes.insert(at + i, s);
            }
        }
        self.host.mark_dirty();
    }
}

/// Map a per-contour transform over `shape` (a `Path` or `Compound`), rebuilding
/// it with the transformed geometry while preserving its paint style. Returns
/// `None` for shapes with no editable contour or a degenerate result.
fn distort_shape<F>(shape: &Shape, f: &F) -> Option<Shape>
where
    F: Fn(&[(f32, f32)], &[(f32, f32)], bool) -> (Vec<(f32, f32)>, Vec<(f32, f32)>),
{
    match shape {
        Shape::Path {
            points,
            handles,
            closed,
            fill,
            stroke,
            stroke_w,
            stroke_style,
            ..
        } => {
            if points.len() < 2 {
                return None;
            }
            let (np, nh) = f(points, handles, *closed);
            if np.len() < 2 {
                return None;
            }
            let mut out = Shape::path(np, nh, *closed, *fill, *stroke, *stroke_w);
            if let Shape::Path { stroke_style: ss, .. } = &mut out {
                *ss = stroke_style.clone();
            }
            Some(out)
        }
        Shape::Compound {
            subpaths,
            fill_rule,
            fill,
            stroke,
            stroke_w,
            stroke_style,
            ..
        } => {
            let mut new_sub: Vec<SubPath> = Vec::with_capacity(subpaths.len());
            for sp in subpaths {
                if sp.points.len() < 2 {
                    new_sub.push(sp.clone());
                    continue;
                }
                let (np, nh) = f(&sp.points, &sp.handles, sp.closed);
                if np.len() < 2 {
                    new_sub.push(sp.clone());
                    continue;
                }
                new_sub.push(SubPath {
                    points: np,
                    handles: nh,
                    closed: sp.closed,
                });
            }
            Some(Shape::Compound {
                subpaths: new_sub,
                fill_rule: *fill_rule,
                fill: *fill,
                fill_gradient: None,
                stroke: *stroke,
                stroke_w: *stroke_w,
                stroke_style: stroke_style.clone(),
                appearance: None,
                visible: true,
                group: None,
                clip: None,
                mask: false,
                omask: None,
                omask_path: false,
                omask_invert: false,
                blend: None,
                blend_step: false,
                name: None,
                locked: false,
                layer_color: None,
                envelope_mesh: None,
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state::OffsetJoin;

    /// A closed square corner path matching the pure-test fixture.
    fn square() -> (Vec<(f32, f32)>, Vec<(f32, f32)>) {
        (
            vec![(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)],
            vec![(0.0, 0.0); 4],
        )
    }

    fn app_with_square_path() -> App {
        let mut app = App::new();
        app.doc.shapes.clear();
        let (p, _) = square();
        app.doc.shapes.push(Shape::path(
            p,
            Vec::new(),
            true,
            [0.5, 0.5, 0.8, 1.0],
            [0.0, 0.0, 0.0, 1.0],
            1.0,
        ));
        app.select_single(0);
        app
    }

    #[test]
    fn apply_offset_path_grows_shape() {
        let mut app = app_with_square_path();
        let before = app.doc.shapes[0].bounds().unwrap();
        app.apply(Action::DistortOffsetPath {
            shape_id: 0,
            distance: 10.0,
            join: OffsetJoin::Miter,
        });
        let after = app.doc.shapes[0].bounds().unwrap();
        assert!(after.w > before.w + 15.0, "offset grew the shape");
        assert!(app.history.can_undo(), "offset is one undo step");
    }

    #[test]
    fn apply_pucker_bloat_action() {
        let mut app = app_with_square_path();
        let b0 = app.doc.shapes[0].bounds().unwrap();
        app.apply(Action::DistortPuckerBloat {
            shape_id: 0,
            amount: 0.5,
        });
        let b1 = app.doc.shapes[0].bounds().unwrap();
        assert!(b1.w > b0.w, "bloat enlarged the bbox");
        assert!(app.history.can_undo());
    }

    #[test]
    fn apply_roughen_action_is_deterministic() {
        let mut a = app_with_square_path();
        let mut b = app_with_square_path();
        a.apply(Action::DistortRoughen { shape_id: 0, size: 6.0, detail: 3, seed: 42, smooth: false });
        b.apply(Action::DistortRoughen { shape_id: 0, size: 6.0, detail: 3, seed: 42, smooth: false });
        let pa = match &a.doc.shapes[0] {
            Shape::Path { points, .. } => points.clone(),
            _ => panic!("expected path"),
        };
        let pb = match &b.doc.shapes[0] {
            Shape::Path { points, .. } => points.clone(),
            _ => panic!("expected path"),
        };
        assert_eq!(pa, pb, "same seed → identical roughen via the action");
        assert!(pa.len() > 4, "roughen subdivided the path");
    }

    #[test]
    fn apply_twist_action_changes_geometry() {
        let mut app = app_with_square_path();
        let orig = match &app.doc.shapes[0] {
            Shape::Path { points, .. } => points.clone(),
            _ => unreachable!(),
        };
        app.apply(Action::DistortTwist { shape_id: 0, angle_deg: 45.0 });
        let now = match &app.doc.shapes[0] {
            Shape::Path { points, .. } => points.clone(),
            _ => unreachable!(),
        };
        assert_ne!(orig, now, "twist moved the anchors");
        assert!(app.history.can_undo());
    }

    #[test]
    fn apply_transform_each_replicates() {
        let mut app = app_with_square_path();
        assert_eq!(app.doc.shapes.len(), 1);
        app.apply(Action::DistortTransformEach {
            shape_id: 0,
            move_x: 120.0,
            move_y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            angle_deg: 0.0,
            copies: 3,
        });
        assert_eq!(app.doc.shapes.len(), 4, "source + 3 copies");
        // The 3rd copy is shifted by 3×120 from the source.
        let src = app.doc.shapes[0].bounds().unwrap();
        let last = app.doc.shapes[3].bounds().unwrap();
        assert!((last.x - (src.x + 360.0)).abs() < 1.0, "cumulative replication offset");
        assert!(app.history.can_undo());
    }

    #[test]
    fn apply_transform_each_in_place_when_zero_copies() {
        let mut app = app_with_square_path();
        app.apply(Action::DistortTransformEach {
            shape_id: 0,
            move_x: 50.0,
            move_y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            angle_deg: 0.0,
            copies: 0,
        });
        assert_eq!(app.doc.shapes.len(), 1, "no copies → transform in place");
        let b = app.doc.shapes[0].bounds().unwrap();
        assert!((b.x - 50.0).abs() < 1.0, "shape moved by 50");
    }

    #[test]
    fn apply_zigzag_action_on_compound_subpaths() {
        // A compound path's sub-contours each get zig-zagged.
        let mut app = app_with_square_path();
        let (p, _) = square();
        app.doc.shapes[0] = Shape::Compound {
            subpaths: vec![SubPath::ring(p)],
            fill_rule: crate::document::FillRule::NonZero,
            fill: [0.5, 0.5, 0.8, 1.0],
            fill_gradient: None,
            stroke: [0.0, 0.0, 0.0, 1.0],
            stroke_w: 1.0,
            stroke_style: Default::default(),
            appearance: None,
            visible: true,
            group: None,
            clip: None,
            mask: false,
            omask: None,
            omask_path: false,
            omask_invert: false,
            blend: None,
            blend_step: false,
            name: None,
            locked: false,
            layer_color: None,
            envelope_mesh: None,
        };
        app.apply(Action::DistortZigZag { shape_id: 0, size: 8.0, ridges: 2, smooth: false });
        match &app.doc.shapes[0] {
            Shape::Compound { subpaths, .. } => {
                assert!(subpaths[0].points.len() > 4, "subpath zig-zagged");
            }
            _ => panic!("expected a compound result"),
        }
    }

    #[test]
    fn distort_out_of_range_is_noop() {
        let mut app = app_with_square_path();
        app.apply(Action::DistortTwist { shape_id: 99, angle_deg: 30.0 });
        assert!(!app.history.can_undo(), "bad id changes nothing");
    }
}
