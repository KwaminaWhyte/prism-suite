//! Center preview surface — the bridged CPU-composited frame plus the on-canvas
//! **transform gizmo** and **click-to-select** interaction.
//!
//! The preview pixels come from the host ([`crate::canvas_host`]); this panel
//! wraps that image in a relative container that:
//!
//! - records the image's painted [`Bounds`] into `app.preview_rect` (via a
//!   `canvas`), so the root [`App`] can map a pointer position to **comp space**
//!   with the exact same fit the image is drawn at;
//! - hit-tests pointer presses: a press on a gizmo **handle** of the selected
//!   layer arms a drag (`App::begin_gizmo_drag`); otherwise the press **selects**
//!   whichever layer's quad sits under the pointer (`App::layer_at_pointer`),
//!   matching the egui app's click-to-select on the canvas;
//! - drags the held handle live (`App::update_gizmo_drag` → keys the changed
//!   transform at the grab time, exactly like the properties panel's writes);
//! - paints the gizmo overlay (bounding box, corner scale handles, rotation knob
//!   plus connector, anchor cross) through a `canvas`, mirroring the egui app's
//!   `pulse_app::preview::paint_gizmo`.
//!
//! All the drag math is the engine's pure [`pulse_app::gizmo`] module (no fork);
//! this panel only owns hit-testing the screen handles and painting them.

use std::sync::Arc;

use gpui::prelude::FluentBuilder;
use gpui::{
    canvas, div, fill, img, point, px, rgb, Bounds, Context, InteractiveElement, IntoElement,
    MouseButton, ParentElement, Path, Pixels, RenderImage, Styled,
};

use pulse_app::gizmo::{GizmoGeom, Handle as GizmoHandle};

use crate::app_state::{Action, App};
use crate::Pulse;

/// Accent for the gizmo chrome (teal, matching the playhead / keyframes).
const ACCENT: u32 = 0x37c8c0;
/// Highlight color for a hovered gizmo handle.
const HOVER: u32 = 0xffee55;
/// A near-black fill for unhighlighted point handles (so they read as hollow).
const HANDLE_BG: u32 = 0x16181c;
/// Screen-space hit radius (px) for grabbing a gizmo handle.
const HIT_TOL: f32 = 8.0;

/// Build the preview surface element: the bridged image, a bounds-recording
/// canvas, the pointer interaction, and the gizmo overlay canvas.
pub fn render(
    app: &App,
    image: Arc<RenderImage>,
    w: f32,
    h: f32,
    cx: &mut Context<Pulse>,
) -> impl IntoElement {
    let preview_rect = app.preview_rect.clone();

    // Snapshot the geometry the overlay canvas needs (it can't borrow `app`).
    let geom = app.selected_gizmo();
    let fit = app.preview_fit();
    let hovered_handle = app.hovered_gizmo_handle;

    // The image, sized to the host's capped preview dims, in a relative wrapper so
    // the canvases overlay it exactly. The wrapper records the image bounds.
    div()
        .relative()
        .w(px(w))
        .h(px(h))
        // The bridged composited frame.
        .child(img(image).w(px(w)).h(px(h)))
        // Record the painted image bounds so the root App can map pointer→comp.
        .child(
            canvas(
                move |bounds, _win, _cx| {
                    preview_rect.set(Some(bounds));
                },
                |_bounds, _state, _win, _cx| {},
            )
            .absolute()
            .size_full(),
        )
        // Pointer interaction: select a layer or grab/drag the gizmo.
        .child(
            div()
                .id("preview-surface")
                .absolute()
                .size_full()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|root, ev: &gpui::MouseDownEvent, _win, cx| {
                        let (sx, sy) = (f32::from(ev.position.x), f32::from(ev.position.y));
                        // Alt+drag on a move handle → duplicate then move.
                        if ev.modifiers.alt {
                            if let Some(i) = root.app.selected_layer {
                                if root.app.gizmo_hit(sx, sy, HIT_TOL).is_some() {
                                    root.app.apply(Action::DuplicateLayer(i));
                                    // begin drag on the newly duplicated layer.
                                    root.app.begin_gizmo_drag(sx, sy, HIT_TOL);
                                    root.app.apply(Action::Pause);
                                    cx.notify();
                                    return;
                                }
                            }
                        }
                        // A press on a handle of the selected layer's gizmo arms a
                        // drag; otherwise it selects whatever layer is under it.
                        if root.app.begin_gizmo_drag(sx, sy, HIT_TOL) {
                            root.app.apply(Action::Pause);
                            cx.notify();
                            return;
                        }
                        if let Some(i) = root.app.layer_at_pointer(sx, sy) {
                            root.app.apply(Action::SelectLayer(i));
                            // If the freshly selected layer's gizmo is under the
                            // pointer (its body), begin a move drag immediately so
                            // a click-drag both selects and moves in one gesture.
                            root.app.begin_gizmo_drag(sx, sy, HIT_TOL);
                            cx.notify();
                        }
                    }),
                )
                .on_mouse_move(cx.listener(|root, ev: &gpui::MouseMoveEvent, _win, cx| {
                    let (sx, sy) = (f32::from(ev.position.x), f32::from(ev.position.y));
                    if root.app.gizmo_drag.is_some() {
                        // Shift+drag on scale handle → uniform scale (lock aspect).
                        // Ctrl/Cmd+drag on rotate handle → snap to 15°.
                        // The actual gizmo drag math in `update_gizmo_drag` uses
                        // the engine's `gizmo::drag`; we store the modifier flags
                        // in GizmoDrag so it can consult them. For now we apply
                        // snapping post-hoc: if Ctrl and we're rotating, snap.
                        root.app.update_gizmo_drag(sx, sy);
                        // Snap rotation to 15° when Ctrl/Cmd held.
                        if ev.modifiers.control || ev.modifiers.secondary() {
                            if let Some(drag) = root.app.gizmo_drag {
                                if drag.handle == GizmoHandle::Rotate {
                                    let ci = root.app.active_comp_index();
                                    let t = drag.time;
                                    if let Some(layer) = root.app.project.comps[ci].layers.get_mut(drag.layer) {
                                        let raw = layer.rotation.sample(t, 0.0);
                                        let snapped = (raw / 15.0).round() * 15.0;
                                        if (snapped - raw).abs() > 0.01 {
                                            layer.rotation.set_key(t, snapped);
                                            root.app.host.mark_dirty();
                                        }
                                    }
                                }
                            }
                        }
                        cx.notify();
                    } else {
                        // Update hovered handle for highlight.
                        let hit = root.app.gizmo_hit(sx, sy, HIT_TOL);
                        if hit != root.app.hovered_gizmo_handle {
                            root.app.apply(Action::SetHoveredGizmoHandle(hit));
                            cx.notify();
                        }
                    }
                }))
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|root, _ev, _win, cx| {
                        if root.app.gizmo_drag.take().is_some() {
                            cx.notify();
                        }
                    }),
                )
                .on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(|root, _ev, _win, cx| {
                        if root.app.gizmo_drag.take().is_some() {
                            cx.notify();
                        }
                    }),
                ),
        )
        // The gizmo overlay (only when a layer is selected and the fit is known).
        .when_some(geom.zip(fit), |this, (geom, ((cx_s, cy_s), scale))| {
            this.child(
                canvas(
                    move |_bounds, _win, _cx| {},
                    move |_bounds, _state, window, _cx| {
                        paint_gizmo(window, &geom, cx_s, cy_s, scale, hovered_handle);
                    },
                )
                .absolute()
                .size_full(),
            )
        })
}

/// Paint the transform gizmo for `geom` (comp space) into the window, using the
/// comp→screen mapping `(center, scale)`. Mirrors `pulse_app::preview::paint_gizmo`:
/// a bounding box, corner scale handles, the rotation knob + connector, and the
/// anchor cross. Hovered handles are highlighted in yellow.
fn paint_gizmo(
    window: &mut gpui::Window,
    geom: &GizmoGeom,
    cx_s: f32,
    cy_s: f32,
    scale: f32,
    hovered: Option<GizmoHandle>,
) {
    let to_screen = |(cx, cy): (f32, f32)| (cx_s + cx * scale, cy_s + cy * scale);
    let corners: [(f32, f32); 4] = geom.corners.map(to_screen);
    let knob = to_screen(geom.rotate_knob);
    let anchor = to_screen(geom.anchor);

    // Bounding box outline (four edges).
    for i in 0..4 {
        stroke_line(window, corners[i], corners[(i + 1) % 4], 1.5, ACCENT);
    }

    // Connector from the top-edge midpoint to the rotation knob.
    let top_mid = mid(corners[0], corners[1]);
    stroke_line(window, top_mid, knob, 1.0, ACCENT);

    // Rotation knob — highlight yellow when hovered.
    let knob_color = if matches!(hovered, Some(GizmoHandle::Rotate)) { HOVER } else { ACCENT };
    fill_square(window, knob, 5.0, knob_color);

    // Corner scale handles (small accent squares) — highlight the hovered corner.
    for (ci, &c) in corners.iter().enumerate() {
        // Scale(u8) where 0=TL, 1=TR, 2=BR, 3=BL.
        let corner_hovered = matches!(hovered, Some(GizmoHandle::Scale(n)) if n as usize == ci);
        let handle_color = if corner_hovered { HOVER } else { ACCENT };
        fill_square(window, c, 3.5, HANDLE_BG);
        // A thin ring around the handle.
        let r = 3.5;
        stroke_line(window, (c.0 - r, c.1 - r), (c.0 + r, c.1 - r), 1.5, handle_color);
        stroke_line(window, (c.0 + r, c.1 - r), (c.0 + r, c.1 + r), 1.5, handle_color);
        stroke_line(window, (c.0 + r, c.1 + r), (c.0 - r, c.1 + r), 1.5, handle_color);
        stroke_line(window, (c.0 - r, c.1 + r), (c.0 - r, c.1 - r), 1.5, handle_color);
    }

    // Anchor cross (the scale/rotation pivot).
    let anchor_color = if matches!(hovered, Some(GizmoHandle::Anchor)) { HOVER } else { ACCENT };
    let s = 7.0;
    stroke_line(window, (anchor.0 - s, anchor.1), (anchor.0 + s, anchor.1), 1.5, anchor_color);
    stroke_line(window, (anchor.0, anchor.1 - s), (anchor.0, anchor.1 + s), 1.5, anchor_color);
}

/// Midpoint of two screen points.
fn mid(a: (f32, f32), b: (f32, f32)) -> (f32, f32) {
    ((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5)
}

/// Stroke a line segment as a thin filled quad (gpui paths fill, not stroke), so
/// arbitrary-angle gizmo edges render at a constant width.
fn stroke_line(window: &mut gpui::Window, a: (f32, f32), b: (f32, f32), width: f32, color: u32) {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-4 {
        return;
    }
    // Unit normal, scaled to half-width.
    let (nx, ny) = (-dy / len * width * 0.5, dx / len * width * 0.5);
    let p0 = point(px(a.0 + nx), px(a.1 + ny));
    let p1 = point(px(b.0 + nx), px(b.1 + ny));
    let p2 = point(px(b.0 - nx), px(b.1 - ny));
    let p3 = point(px(a.0 - nx), px(a.1 - ny));
    let mut path = Path::new(p0);
    path.line_to(p1);
    path.line_to(p2);
    path.line_to(p3);
    path.line_to(p0);
    window.paint_path(path, rgb(color));
}

/// Fill a small axis-aligned square centred at `c` with half-extent `r`.
fn fill_square(window: &mut gpui::Window, c: (f32, f32), r: f32, color: u32) {
    let bounds: Bounds<Pixels> = Bounds {
        origin: point(px(c.0 - r), px(c.1 - r)),
        size: gpui::size(px(r * 2.0), px(r * 2.0)),
    };
    window.paint_quad(fill(bounds, rgb(color)));
}
