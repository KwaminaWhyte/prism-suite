//! Inspector — dimensions (typeable X / Y / W / H), rotation, stroke-width and
//! opacity rows. Split out of `inspector.rs`.
//!
//! Each numeric value is a [`numeric_cell`](super::numeric_cell): click it to
//! type an exact value (parsed → existing `Set*` action), with `−/＋` steppers
//! kept alongside for coarse adjustment. The steppers remain the live display.

use super::*;
use crate::numeric_edit::{NumericEdit, NumericTarget};
use crate::panels::numeric_cell;

/// Document-units step for the W / H / X / Y steppers.
const DIM_STEP: f32 = 1.0;
/// Degrees per rotation stepper click.
const ROT_STEP: f32 = 15.0;

/// A `LABEL  −  <typeable value>  ＋` row whose value cell types `target`.
/// `dec` / `inc` apply the coarse stepper deltas.
fn typeable_row(
    label: &'static str,
    id: (&'static str, u64),
    target: NumericTarget,
    display: String,
    edit_seed: String,
    numeric: Option<&NumericEdit>,
    cx: &mut Context<Contour>,
    dec: impl Fn(&mut Contour, &mut Context<Contour>) + 'static,
    inc: impl Fn(&mut Contour, &mut Context<Contour>) + 'static,
) -> impl IntoElement {
    let cell = numeric_cell(id, target, display, edit_seed, numeric, cx);
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .child(div().w(px(56.0)).text_color(colors::text_secondary()).child(label))
        .child(step_button((id.0, id.1 * 10 + 1), "−", cx, dec))
        .child(div().flex_1().flex().justify_center().child(cell))
        .child(step_button((id.0, id.1 * 10 + 2), "＋", cx, inc))
}

/// The Dimensions section: typeable X / Y / W / H seeded from the live selection
/// bounding box `[x, y, w, h]`.
pub(super) fn dimensions_section(
    bbox: [f32; 4],
    numeric: Option<&NumericEdit>,
    cx: &mut Context<Contour>,
) -> impl IntoElement {
    let [x, y, w, h] = bbox;

    let x_row = typeable_row(
        "X",
        ("dim-x", 0),
        NumericTarget::X,
        format!("{x:.1}"),
        format!("{x:.1}"),
        numeric,
        cx,
        move |root, cx| {
            root.app.apply(Action::SetSelectionX(x - DIM_STEP));
            cx.notify();
        },
        move |root, cx| {
            root.app.apply(Action::SetSelectionX(x + DIM_STEP));
            cx.notify();
        },
    );
    let y_row = typeable_row(
        "Y",
        ("dim-y", 0),
        NumericTarget::Y,
        format!("{y:.1}"),
        format!("{y:.1}"),
        numeric,
        cx,
        move |root, cx| {
            root.app.apply(Action::SetSelectionY(y - DIM_STEP));
            cx.notify();
        },
        move |root, cx| {
            root.app.apply(Action::SetSelectionY(y + DIM_STEP));
            cx.notify();
        },
    );
    let w_row = typeable_row(
        "W",
        ("dim-w", 0),
        NumericTarget::Width,
        format!("{w:.1}"),
        format!("{w:.1}"),
        numeric,
        cx,
        move |root, cx| {
            root.app.apply(Action::SetSelectionWidth((w - DIM_STEP).max(0.0)));
            cx.notify();
        },
        move |root, cx| {
            root.app.apply(Action::SetSelectionWidth(w + DIM_STEP));
            cx.notify();
        },
    );
    let h_row = typeable_row(
        "H",
        ("dim-h", 0),
        NumericTarget::Height,
        format!("{h:.1}"),
        format!("{h:.1}"),
        numeric,
        cx,
        move |root, cx| {
            root.app.apply(Action::SetSelectionHeight((h - DIM_STEP).max(0.0)));
            cx.notify();
        },
        move |root, cx| {
            root.app.apply(Action::SetSelectionHeight(h + DIM_STEP));
            cx.notify();
        },
    );

    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(x_row)
        .child(y_row)
        .child(w_row)
        .child(h_row)
}

/// Rotation row: the value is a *relative* rotate-by (the model stores no
/// absolute per-shape rotation), so the cell always displays `0`. Typing N spins
/// the selection N degrees; steppers nudge by ±15°.
pub(super) fn rotation_row(
    numeric: Option<&NumericEdit>,
    cx: &mut Context<Contour>,
) -> impl IntoElement {
    typeable_row(
        "Rotate",
        ("dim-rot", 0),
        NumericTarget::Rotation,
        "0°".to_string(),
        "0".to_string(),
        numeric,
        cx,
        move |root, cx| {
            root.app.apply(Action::RotateSelection(-ROT_STEP));
            cx.notify();
        },
        move |root, cx| {
            root.app.apply(Action::RotateSelection(ROT_STEP));
            cx.notify();
        },
    )
}

/// Stroke-width stepper row with a typeable value cell, clamped ≥ 0.
pub(super) fn width_row(
    w: f32,
    numeric: Option<&NumericEdit>,
    cx: &mut Context<Contour>,
) -> impl IntoElement {
    let cell = numeric_cell(
        ("sw-cell", 0),
        NumericTarget::StrokeWidth,
        format!("{w:.1}"),
        format!("{w:.1}"),
        numeric,
        cx,
    );
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .child(
            div()
                .w(px(56.0))
                .text_color(colors::text_secondary())
                .child("Stroke W"),
        )
        .child(step_button(("sw-dec", 0), "−", cx, move |root, cx| {
            if let Some(s) = root.app.selected_shape() {
                let next = (s.stroke_width() - WIDTH_STEP).max(0.0);
                root.app.apply(Action::SetStrokeWidth(next));
                cx.notify();
            }
        }))
        .child(div().flex_1().flex().justify_center().child(cell))
        .child(step_button(("sw-inc", 0), "＋", cx, move |root, cx| {
            if let Some(s) = root.app.selected_shape() {
                let next = s.stroke_width() + WIDTH_STEP;
                root.app.apply(Action::SetStrokeWidth(next));
                cx.notify();
            }
        }))
}

/// Opacity stepper row with a typeable percentage cell, clamped [0,100]%.
/// Mirrors the fill alpha (see module docs).
pub(super) fn opacity_row(
    o: f32,
    numeric: Option<&NumericEdit>,
    cx: &mut Context<Contour>,
) -> impl IntoElement {
    let pct = (o.clamp(0.0, 1.0) * 100.0).round() as u32;
    let cell = numeric_cell(
        ("op-cell", 0),
        NumericTarget::Opacity,
        format!("{pct}%"),
        format!("{pct}"),
        numeric,
        cx,
    );
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .child(div().w(px(56.0)).text_color(colors::text_secondary()).child("Opacity"))
        .child(step_button(("op-dec", 0), "−", cx, move |root, cx| {
            if let Some(cur) = root.app.selected_fill() {
                let next = (cur[3] - OPACITY_STEP).clamp(0.0, 1.0);
                root.app.apply(Action::SetOpacity(next));
                cx.notify();
            }
        }))
        .child(div().flex_1().flex().justify_center().child(cell))
        .child(step_button(("op-inc", 0), "＋", cx, move |root, cx| {
            if let Some(cur) = root.app.selected_fill() {
                let next = (cur[3] + OPACITY_STEP).clamp(0.0, 1.0);
                root.app.apply(Action::SetOpacity(next));
                cx.notify();
            }
        }))
}
