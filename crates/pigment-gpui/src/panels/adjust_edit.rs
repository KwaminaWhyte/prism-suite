//! Per-adjustment parameter editor for the layers panel.
//!
//! When the active layer is an adjustment layer, the layers panel renders this
//! editor under its row. gpui 0.2.2 has no native slider, so each scalar param
//! is a `−  LABEL value  +` stepper (the same idiom as the tool-options bar). A
//! nudge emits [`Action::SetAdjustment`] carrying the WHOLE updated `Adjustment`
//! with that one field changed — `App::apply` re-uploads any LUT and re-composites
//! (the engine owns all adjustment math; this file only nudges params).
//!
//! Parameterless kinds (Invert / Black & White) show a short note; the rich
//! editors the egui app has (a draggable Curves canvas, per-range Color-Balance
//! sliders) reduce here to their headline scalars or a note this wave — the
//! adjustment still composites correctly at its defaults and any LUT-based kind
//! stays editable through the steppers that do apply.

use std::{cell::Cell, rc::Rc};

use gpui::{
    canvas, div, fill, px, size, Bounds, Context, InteractiveElement, IntoElement,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Point,
    SharedString, StatefulInteractiveElement, Styled,
};
use prism_core::{Adjustment, LayerId};
use prism_ui::colors;

use crate::app_state::{Action, App};
use crate::Pigment;

/// Render the parameter editor for adjustment `adj` on layer `id`. Returns an
/// element with one stepper row per scalar param.
pub fn adjustment_editor(
    id: LayerId,
    adj: &Adjustment,
    app: &App,
    cx: &mut Context<Pigment>,
) -> impl IntoElement {
    let mut col = div()
        .flex()
        .flex_col()
        .gap_1()
        .px_2()
        .py_1()
        .bg(colors::surface_bg())
        .border_t_1()
        .border_color(colors::surface_border());

    // Each closure clones `adj`, mutates one field, and wraps it in a SetAdjustment
    // action. `f` produces the nudged adjustment from a signed step.
    match adj {
        Adjustment::BrightnessContrast {
            brightness,
            contrast,
        } => {
            let (b, c) = (*brightness, *contrast);
            col = col
                .child(stepper(
                    id,
                    "bc-bri",
                    "Brightness",
                    format!("{b:+.2}"),
                    Adjustment::BrightnessContrast {
                        brightness: (b - 0.05).clamp(-0.5, 0.5),
                        contrast: c,
                    },
                    Adjustment::BrightnessContrast {
                        brightness: (b + 0.05).clamp(-0.5, 0.5),
                        contrast: c,
                    },
                    cx,
                ))
                .child(stepper(
                    id,
                    "bc-con",
                    "Contrast",
                    format!("{c:+.2}"),
                    Adjustment::BrightnessContrast {
                        brightness: b,
                        contrast: (c - 0.05).clamp(-0.5, 1.0),
                    },
                    Adjustment::BrightnessContrast {
                        brightness: b,
                        contrast: (c + 0.05).clamp(-0.5, 1.0),
                    },
                    cx,
                ));
        }
        Adjustment::Levels {
            in_black,
            in_white,
            gamma,
        } => {
            let (ib, iw, g) = (*in_black, *in_white, *gamma);
            col = col
                .child(stepper(
                    id,
                    "lv-blk",
                    "Black",
                    format!("{ib:.2}"),
                    Adjustment::Levels {
                        in_black: (ib - 0.02).clamp(0.0, 1.0),
                        in_white: iw,
                        gamma: g,
                    },
                    Adjustment::Levels {
                        in_black: (ib + 0.02).clamp(0.0, 1.0),
                        in_white: iw,
                        gamma: g,
                    },
                    cx,
                ))
                .child(stepper(
                    id,
                    "lv-wht",
                    "White",
                    format!("{iw:.2}"),
                    Adjustment::Levels {
                        in_black: ib,
                        in_white: (iw - 0.02).clamp(0.0, 1.0),
                        gamma: g,
                    },
                    Adjustment::Levels {
                        in_black: ib,
                        in_white: (iw + 0.02).clamp(0.0, 1.0),
                        gamma: g,
                    },
                    cx,
                ))
                .child(stepper(
                    id,
                    "lv-gam",
                    "Gamma",
                    format!("{g:.2}"),
                    Adjustment::Levels {
                        in_black: ib,
                        in_white: iw,
                        gamma: (g - 0.1).clamp(0.1, 4.0),
                    },
                    Adjustment::Levels {
                        in_black: ib,
                        in_white: iw,
                        gamma: (g + 0.1).clamp(0.1, 4.0),
                    },
                    cx,
                ));
        }
        Adjustment::HueSaturation {
            hue,
            saturation,
            lightness,
        } => {
            let (h, s, l) = (*hue, *saturation, *lightness);
            col = col
                .child(stepper(
                    id,
                    "hs-hue",
                    "Hue",
                    format!("{h:.0}"),
                    Adjustment::HueSaturation {
                        hue: (h - 10.0).clamp(-180.0, 180.0),
                        saturation: s,
                        lightness: l,
                    },
                    Adjustment::HueSaturation {
                        hue: (h + 10.0).clamp(-180.0, 180.0),
                        saturation: s,
                        lightness: l,
                    },
                    cx,
                ))
                .child(stepper(
                    id,
                    "hs-sat",
                    "Saturation",
                    format!("{s:+.2}"),
                    Adjustment::HueSaturation {
                        hue: h,
                        saturation: (s - 0.05).clamp(-1.0, 1.0),
                        lightness: l,
                    },
                    Adjustment::HueSaturation {
                        hue: h,
                        saturation: (s + 0.05).clamp(-1.0, 1.0),
                        lightness: l,
                    },
                    cx,
                ))
                .child(stepper(
                    id,
                    "hs-lit",
                    "Lightness",
                    format!("{l:+.2}"),
                    Adjustment::HueSaturation {
                        hue: h,
                        saturation: s,
                        lightness: (l - 0.05).clamp(-0.5, 0.5),
                    },
                    Adjustment::HueSaturation {
                        hue: h,
                        saturation: s,
                        lightness: (l + 0.05).clamp(-0.5, 0.5),
                    },
                    cx,
                ));
        }
        Adjustment::Exposure { stops } => {
            let s = *stops;
            col = col.child(stepper(
                id,
                "ex-stp",
                "Stops",
                format!("{s:+.2}"),
                Adjustment::Exposure {
                    stops: (s - 0.25).clamp(-3.0, 3.0),
                },
                Adjustment::Exposure {
                    stops: (s + 0.25).clamp(-3.0, 3.0),
                },
                cx,
            ));
        }
        Adjustment::Vibrance { amount } => {
            let a = *amount;
            col = col.child(stepper(
                id,
                "vi-amt",
                "Vibrance",
                format!("{a:+.2}"),
                Adjustment::Vibrance {
                    amount: (a - 0.05).clamp(-1.0, 1.0),
                },
                Adjustment::Vibrance {
                    amount: (a + 0.05).clamp(-1.0, 1.0),
                },
                cx,
            ));
        }
        Adjustment::Threshold { level } => {
            let l = *level;
            col = col.child(stepper(
                id,
                "th-lvl",
                "Level",
                format!("{l:.2}"),
                Adjustment::Threshold {
                    level: (l - 0.02).clamp(0.0, 1.0),
                },
                Adjustment::Threshold {
                    level: (l + 0.02).clamp(0.0, 1.0),
                },
                cx,
            ));
        }
        Adjustment::Posterize { levels } => {
            let n = *levels;
            col = col.child(stepper(
                id,
                "po-lvl",
                "Levels",
                format!("{n}"),
                Adjustment::Posterize {
                    levels: n.saturating_sub(1).max(2),
                },
                Adjustment::Posterize {
                    levels: (n + 1).min(32),
                },
                cx,
            ));
        }
        Adjustment::PhotoFilter { color, density } => {
            let (c, d) = (*color, *density);
            col = col.child(stepper(
                id,
                "pf-den",
                "Density",
                format!("{d:.2}"),
                Adjustment::PhotoFilter {
                    color: c,
                    density: (d - 0.05).clamp(0.0, 1.0),
                },
                Adjustment::PhotoFilter {
                    color: c,
                    density: (d + 0.05).clamp(0.0, 1.0),
                },
                cx,
            ));
        }
        // LUT-based kinds with rich egui editors (a draggable Curves canvas /
        // per-range Color-Balance sliders) compose at their defaults here; a
        // headline note keeps the panel honest until those editors are ported.
        Adjustment::Curves(cp) => {
            let dragging = app.dragging_curve_point.and_then(|(lid, i)| if lid == id { Some(i) } else { None });
            let hovered = app.hovered_curve_point.and_then(|(lid, i)| if lid == id { Some(i) } else { None });
            col = col.child(curves_editor(id, cp.rgb.clone(), dragging, hovered, cx));
        }
        Adjustment::GradientMap { .. } => {
            col = col.child(note("Gradient Map — shadows→highlights ramp"));
        }
        Adjustment::ColorBalance { .. } => {
            col = col.child(note("Color Balance — per-range shifts (sliders TBD)"));
        }
        Adjustment::ChannelMixer { .. } => {
            col = col.child(note("Channel Mixer — identity matrix (sliders TBD)"));
        }
        Adjustment::Invert | Adjustment::BlackWhite => {
            col = col.child(note("(no parameters)"));
        }
    }
    col
}

/// A `−  LABEL value  +` stepper whose buttons carry pre-built adjustments
/// (`dec`/`inc`) wrapped into [`Action::SetAdjustment`] on click. `key` makes the
/// element ids unique within the row.
#[allow(clippy::too_many_arguments)]
fn stepper(
    id: LayerId,
    key: &'static str,
    label: &'static str,
    value: String,
    dec: Adjustment,
    inc: Adjustment,
    cx: &mut Context<Pigment>,
) -> impl IntoElement {
    let btn = |suffix: &'static str, glyph: &'static str, adj: Adjustment, cx: &mut Context<Pigment>| {
        div()
            .id(SharedString::from(format!("adj-{}-{key}-{suffix}", id.0)))
            .size(px(18.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded_sm()
            .bg(colors::surface_overlay())
            .border_1()
            .border_color(colors::surface_border())
            .text_color(colors::text_primary())
            .text_size(px(12.0))
            .cursor_pointer()
            .hover(|s| s.bg(colors::tool_hover()))
            .on_click(cx.listener(move |root, _ev, _win, cx| {
                root.app.apply(Action::SetAdjustment(id, adj.clone()));
                cx.notify();
            }))
            .child(glyph)
    };

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .text_size(px(10.0))
        .child(
            div()
                .flex_1()
                .text_color(colors::text_secondary())
                .child(label),
        )
        .child(btn("dec", "−", dec, cx))
        .child(
            div()
                .min_w(px(34.0))
                .flex()
                .justify_center()
                .text_color(colors::text_primary())
                .child(value),
        )
        .child(btn("inc", "+", inc, cx))
}

/// A muted one-line note (parameterless kinds / editors not yet ported).
fn note(text: &'static str) -> impl IntoElement {
    div()
        .text_size(px(10.0))
        .text_color(colors::text_secondary())
        .child(text)
}

/// Curves canvas editor: a 200×200 GPUI `canvas()` drawing the identity
/// diagonal and the current RGB control points, with mouse interaction for
/// dragging existing points and clicking to add new ones.
///
/// Uses an `Rc<Cell<_>>` to share the canvas bounding rect between the
/// canvas prepare callback and the mouse event handlers — same pattern as the
/// marching-ants canvas in `main.rs`.
fn curves_editor(
    id: LayerId,
    pts: Vec<(f32, f32)>,
    dragging_idx: Option<usize>,
    hovered_idx: Option<usize>,
    cx: &mut Context<Pigment>,
) -> impl IntoElement {
    const SZ: f32 = 200.0;
    const N: usize = 50;
    const HIT_RADIUS: f32 = 6.0;

    // Shared canvas origin (window px) so event handlers can map to [0,1].
    let bounds_cell: Rc<Cell<Option<[f32; 2]>>> = Rc::new(Cell::new(None));
    let bounds_prepare = bounds_cell.clone();
    let bounds_down = bounds_cell.clone();
    let bounds_move = bounds_cell.clone();

    let pts_paint = pts.clone();
    let pts_click = pts.clone();

    let curves_canvas = canvas(
        move |bounds, _win, _cx| {
            let arr = [f32::from(bounds.origin.x), f32::from(bounds.origin.y)];
            bounds_prepare.set(Some(arr));
            (bounds, pts_paint.clone())
        },
        move |_bounds, (b, curve_pts), win, _cx| {
            let ox = f32::from(b.origin.x);
            let oy = f32::from(b.origin.y);

            // Background.
            win.paint_quad(fill(
                Bounds { origin: Point { x: px(ox), y: px(oy) }, size: size(px(SZ), px(SZ)) },
                colors::surface_bg(),
            ));

            // Diagonal identity line.
            for i in 0..N {
                let t = i as f32 / (N - 1) as f32;
                win.paint_quad(fill(
                    Bounds {
                        origin: Point { x: px(ox + t * SZ), y: px(oy + (1.0 - t) * SZ) },
                        size: size(px(2.0), px(2.0)),
                    },
                    colors::surface_border(),
                ));
            }

            // RGB curve line.
            if !curve_pts.is_empty() {
                let mut sorted = curve_pts.clone();
                sorted.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
                if sorted.first().is_none_or(|p| p.0 > 0.01) { sorted.insert(0, (0.0, 0.0)); }
                if sorted.last().is_none_or(|p| p.0 < 0.99) { sorted.push((1.0, 1.0)); }
                for i in 0..N {
                    let t = i as f32 / (N - 1) as f32;
                    let y = interp_curve(&sorted, t);
                    win.paint_quad(fill(
                        Bounds {
                            origin: Point { x: px(ox + t * SZ), y: px(oy + (1.0 - y) * SZ) },
                            size: size(px(2.0), px(2.0)),
                        },
                        colors::text_primary(),
                    ));
                }
            }

            // Control-point markers: warning if hovered, danger if dragging, accent otherwise.
            for (i, &(cx_pt, cy_pt)) in curve_pts.iter().enumerate() {
                let color = if dragging_idx == Some(i) {
                    colors::danger()
                } else if hovered_idx == Some(i) {
                    colors::warning()
                } else {
                    colors::accent()
                };
                let qx = ox + cx_pt * SZ - 3.0;
                let qy = oy + (1.0 - cy_pt) * SZ - 3.0;
                win.paint_quad(fill(
                    Bounds {
                        origin: Point { x: px(qx), y: px(qy) },
                        size: size(px(6.0), px(6.0)),
                    },
                    color,
                ));
            }
        },
    )
    .w(px(SZ))
    .h(px(SZ));

    // Overlay div captures mouse events for drag and add-point.
    let overlay = div()
        .id(SharedString::from(format!("curves-overlay-{}", id.0)))
        .absolute()
        .w(px(SZ))
        .h(px(SZ))
        .cursor_pointer()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |root, ev: &MouseDownEvent, _win, cx| {
                let Some([ox, oy]) = bounds_down.get() else { return };
                let lx = (f32::from(ev.position.x) - ox).clamp(0.0, SZ) / SZ;
                let ly = 1.0 - (f32::from(ev.position.y) - oy).clamp(0.0, SZ) / SZ;

                // Check if close to an existing point.
                let pts = if let Some(l) = root.app.doc.layers.get(id) {
                    if let prism_core::LayerKind::Adjustment(prism_core::Adjustment::Curves(ref cp)) = l.kind {
                        cp.rgb.clone()
                    } else { return }
                } else { return };

                for (i, &(px_pt, py_pt)) in pts.iter().enumerate() {
                    let dx = (lx - px_pt) * SZ;
                    let dy = (ly - py_pt) * SZ;
                    if (dx * dx + dy * dy).sqrt() <= HIT_RADIUS {
                        root.app.apply(Action::BeginCurveDrag(id, i));
                        cx.notify();
                        return;
                    }
                }
                // No hit — add a new point at this normalized position.
                let mut new_pts = pts;
                new_pts.push((lx, ly));
                root.app.apply(Action::SetAdjustmentCurve(id, new_pts));
                cx.notify();
            }),
        )
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |root, ev: &MouseDownEvent, _win, cx| {
                let Some([ox, oy]) = bounds_cell.get() else { return };
                let lx = (f32::from(ev.position.x) - ox).clamp(0.0, SZ) / SZ;
                let ly = 1.0 - (f32::from(ev.position.y) - oy).clamp(0.0, SZ) / SZ;

                let pts = if let Some(l) = root.app.doc.layers.get(id) {
                    if let prism_core::LayerKind::Adjustment(prism_core::Adjustment::Curves(ref cp)) = l.kind {
                        cp.rgb.clone()
                    } else { return }
                } else { return };

                for (i, &(px_pt, py_pt)) in pts.iter().enumerate() {
                    let dx = (lx - px_pt) * SZ;
                    let dy = (ly - py_pt) * SZ;
                    if (dx * dx + dy * dy).sqrt() <= HIT_RADIUS {
                        root.app.apply(Action::RemoveCurvePoint(id, i));
                        cx.notify();
                        return;
                    }
                }
            }),
        )
        .on_mouse_move(cx.listener(move |root, ev: &MouseMoveEvent, _win, cx| {
            if !ev.dragging() {
                return;
            }
            let Some([ox, oy]) = bounds_move.get() else { return };
            let lx = (f32::from(ev.position.x) - ox).clamp(0.0, SZ) / SZ;
            let ly = 1.0 - (f32::from(ev.position.y) - oy).clamp(0.0, SZ) / SZ;

            if let Some((lid, idx)) = root.app.dragging_curve_point {
                if lid == id {
                    root.app.apply(Action::MoveCurvePoint(lid, idx, (lx, ly)));
                    cx.notify();
                }
            }
        }))
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(move |root, _ev: &MouseUpEvent, _win, cx| {
                if root.app.dragging_curve_point.is_some_and(|(lid, _)| lid == id) {
                    root.app.apply(Action::EndCurveDrag);
                    cx.notify();
                }
            }),
        )
        // pts_click used below for the legacy path — captured already
        .child(div().absolute().w(px(SZ)).h(px(SZ)));

    let _ = pts_click; // consumed by overlay closures above

    div()
        .relative()
        .w(px(SZ))
        .h(px(SZ))
        .child(curves_canvas)
        .child(overlay)
}

/// Linear interpolation across sorted curve control points at position `t`
/// (0..1). Returns the interpolated y value. Falls back to identity (y = t)
/// at the extremes if the point list is empty.
fn interp_curve(pts: &[(f32, f32)], t: f32) -> f32 {
    if pts.is_empty() {
        return t;
    }
    if t <= pts[0].0 {
        return pts[0].1;
    }
    let last = pts[pts.len() - 1];
    if t >= last.0 {
        return last.1;
    }
    for i in 0..pts.len() - 1 {
        let (x0, y0) = pts[i];
        let (x1, y1) = pts[i + 1];
        if t >= x0 && t <= x1 {
            let span = (x1 - x0).max(1e-6);
            let frac = (t - x0) / span;
            return y0 + (y1 - y0) * frac;
        }
    }
    t
}
