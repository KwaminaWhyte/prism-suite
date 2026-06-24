//! Inspector — live-shape (polygon/star) section. Split out of `inspector.rs`.

use super::*;

/// The Live Shape inspector: edit the primary polygon / star's parameters
/// (sides / points / radius / inner-ratio). Each `−/＋` click rebuilds the
/// `LiveShape` and emits `SetLiveShape`, which regenerates the outline live —
/// the GPUI mirror of the egui app's `live_shape_section`.
pub(super) fn live_shape_section(live: LiveShape, cx: &mut Context<Contour>) -> impl IntoElement {
    let mut col = div()
        .flex()
        .flex_col()
        .gap_2()
        .child(ui_section_header("Live Shape"))
        .child(ui_divider());

    match live {
        LiveShape::Polygon { sides, radius, corner_radius } => {
            col = col
                .child(live_count_row(
                    "Sides",
                    sides,
                    cx,
                    move |n| LiveShape::Polygon { sides: n, radius, corner_radius },
                ))
                .child(live_radius_row(radius, cx, move |r| LiveShape::Polygon {
                    sides,
                    radius: r,
                    corner_radius,
                }));
        }
        LiveShape::Star {
            points,
            radius,
            inner_ratio,
        } => {
            col = col
                .child(live_count_row("Points", points, cx, move |n| {
                    LiveShape::Star {
                        points: n,
                        radius,
                        inner_ratio,
                    }
                }))
                .child(live_radius_row(radius, cx, move |r| LiveShape::Star {
                    points,
                    radius: r,
                    inner_ratio,
                }))
                .child(live_ratio_row(inner_ratio, cx, move |ir| LiveShape::Star {
                    points,
                    radius,
                    inner_ratio: ir,
                }));
        }
    }
    col
}

/// A `−  LABEL  value  ＋` row for an integer live-shape count (sides / points),
/// clamped to `[MIN_SIDES, MAX_SIDES]`. `build` maps a new count → the full
/// `LiveShape` so the regen stays parametric.
fn live_count_row(
    label: &'static str,
    value: u32,
    cx: &mut Context<Contour>,
    build: impl Fn(u32) -> LiveShape + Copy + 'static,
) -> impl IntoElement {
    let mut mk = move |suffix: &'static str, glyph: &'static str, delta: i64| {
        live_step_btn(suffix, label, glyph, cx, move |_root| {
            let next = (value as i64 + delta).clamp(MIN_SIDES as i64, MAX_SIDES as i64) as u32;
            build(next)
        })
    };
    param_row(label, format!("{value}"), mk("lc-dec", "−", -1), mk("lc-inc", "＋", 1))
}

/// A radius stepper row (document units, step 2, clamped ≥ 0).
fn live_radius_row(
    value: f32,
    cx: &mut Context<Contour>,
    build: impl Fn(f32) -> LiveShape + Copy + 'static,
) -> impl IntoElement {
    const STEP: f32 = 2.0;
    let mut mk = move |suffix: &'static str, glyph: &'static str, delta: f32| {
        live_step_btn(suffix, "Radius", glyph, cx, move |_root| {
            build((value + delta).max(0.0))
        })
    };
    param_row(
        "Radius",
        format!("{value:.0}"),
        mk("lr-dec", "−", -STEP),
        mk("lr-inc", "＋", STEP),
    )
}

/// A star inner-ratio stepper row (step 0.05, clamped [0.05, 1.0]).
fn live_ratio_row(
    value: f32,
    cx: &mut Context<Contour>,
    build: impl Fn(f32) -> LiveShape + Copy + 'static,
) -> impl IntoElement {
    const STEP: f32 = 0.05;
    let mut mk = move |suffix: &'static str, glyph: &'static str, delta: f32| {
        live_step_btn(suffix, "Inner", glyph, cx, move |_root| {
            build((value + delta).clamp(0.05, 1.0))
        })
    };
    param_row(
        "Inner",
        format!("{value:.2}"),
        mk("ir-dec", "−", -STEP),
        mk("ir-inc", "＋", STEP),
    )
}

/// A small `−/＋` button that builds a new `LiveShape` from the current params
/// and emits `SetLiveShape` (the regen choke point).
fn live_step_btn(
    suffix: &'static str,
    label: &'static str,
    glyph: &'static str,
    cx: &mut Context<Contour>,
    build: impl Fn(&Contour) -> LiveShape + 'static,
) -> impl IntoElement {
    let id = (suffix, label.as_bytes()[0] as u64);
    step_button(id, glyph, cx, move |root, cx| {
        let next = build(root);
        root.app.apply(Action::SetLiveShape(next));
        cx.notify();
    })
}
