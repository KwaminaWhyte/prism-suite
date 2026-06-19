//! Inspector panel — edit the selected shape's appearance.
//!
//! Fully wired: every clickable routes through `root.app.apply(...) + cx.notify()`
//! (the single choke point — see `panels/mod.rs`). For the selected shape it shows
//! and EDITS:
//!   - fill colour   → `Action::SetFillColor`
//!   - stroke colour → `Action::SetStrokeColor`
//!   - stroke width  → `Action::SetStrokeWidth`
//!   - opacity       → `Action::SetOpacity` (the fill-alpha channel; Contour's
//!     legacy shape model has no separate opacity field)
//!
//! gpui 0.2.2 has no native slider, so numeric/colour editing uses compact `−/＋`
//! stepper rows plus clickable preset swatches — the same idiom the Pigment host's
//! Color panel uses. Pen/path editing, gradients, text, boolean ops and transforms
//! are out of scope for this pass.

use gpui::prelude::FluentBuilder;
use gpui::{
    deferred, div, px, rgb, rgba, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, ScrollWheelEvent,
};

use contour_app::gradient::{Gradient, GradientKind};
use contour_app::liveshape::{LiveShape, MAX_SIDES, MIN_SIDES};
use contour_app::text::TextAlign;

use crate::app_state::{Action, App, WidthProfilePreset};
use crate::panels::{ui_divider, ui_section_header};
use prism_ui::colors;
use crate::Contour;

const SWATCH_BORDER: u32 = 0x555555;

/// 8/255 ≈ 0.0314 per colour-channel stepper click.
const COLOR_STEP: f32 = 8.0 / 255.0;
/// Stroke-width step (document units) per click.
const WIDTH_STEP: f32 = 0.5;
/// Opacity step per click.
const OPACITY_STEP: f32 = 0.05;

/// Which colour an edit targets — fill or stroke.
#[derive(Clone, Copy)]
enum Target {
    Fill,
    Stroke,
}

#[inline]
fn to_u8(x: f32) -> u32 {
    (x.clamp(0.0, 1.0) * 255.0).round() as u32
}

/// Pack a straight sRGB RGBA `[f32;4]` into 0xRRGGBBAA for `gpui::rgba` (RGBA byte order).
#[inline]
fn pack(c: [f32; 4]) -> u32 {
    let (r, g, b, a) = (to_u8(c[0]), to_u8(c[1]), to_u8(c[2]), to_u8(c[3]));
    (r << 24) | (g << 16) | (b << 8) | a
}

pub fn render(app: &App, cx: &mut Context<Contour>) -> impl IntoElement {
    let header = div()
        .flex()
        .flex_col()
        .child(ui_section_header("Inspector"))
        .child(ui_divider());

    // No selection → an empty-state hint. Selection is set by clicking the canvas
    // or a Layers row.
    let Some(idx) = app.selected else {
        return div()
            .flex()
            .flex_col()
            .bg(colors::surface_bg())
            .child(header)
            .child(
                div()
                    .p_3()
                    .text_color(colors::text_secondary())
                    .text_size(px(11.0))
                    .child("No selection — click a shape on the canvas."),
            );
    };

    let Some(shape) = app.doc.shapes.get(idx) else {
        // Stale selection (shouldn't happen — apply keeps it in range), degrade
        // to the empty state rather than panic.
        return div()
            .flex()
            .flex_col()
            .bg(colors::surface_bg())
            .child(header)
            .child(div().p_3().text_color(colors::text_secondary()).child("No selection."));
    };

    let fill = shape.fill_color();
    let stroke = shape.stroke_color();
    let stroke_w = shape.stroke_width();
    // Opacity mirrors the fill-alpha channel (see module docs); shapes without a
    // fill region (Line) expose no opacity control.
    let opacity = fill.map(|c| c[3]);

    let mut body = div()
        .flex()
        .flex_col()
        .gap_3()
        .p_3()
        .bg(colors::surface_bg())
        .text_color(colors::text_primary())
        .text_size(px(11.0))
        // Shape type label.
        .child(
            div()
                .text_color(colors::text_secondary())
                .text_size(px(10.0))
                .child(format!("{} (#{idx})", shape.label())),
        );

    // ---- Live Shape section (primary polygon / star only) --------------
    if let Some(live) = app.primary_live_shape() {
        body = body.child(live_shape_section(live, cx));
    }

    // ---- Type section (primary text object only) -----------------------
    if let Some(params) = app.primary_text_params() {
        body = body.child(text_section(
            params.font_size,
            params.align,
            params.font_family.clone(),
            app.font_dropdown_open,
            cx,
        ));
    }

    // ---- Fill section (shapes with a fill region only) -----------------
    if let Some(c) = fill {
        body = body.child(color_section("Fill", Target::Fill, c, cx));
    }

    // ---- Gradient: seed button (solid fill, no gradient) or full editor ----
    if shape.fill_gradient().is_none() && fill.is_some() {
        body = body.child(add_gradient_button(idx, cx));
    } else if let Some(gradient) = shape.fill_gradient() {
        body = body.child(gradient_type_section(idx, gradient.kind, cx));
        body = body.child(gradient_editor_section(idx, gradient, app.selected_gradient_stop, cx));
    }

    // ---- Text on Path (Wave 9): shown when selection is text + path ----
    if app.selection.len() == 2 {
        let text_idx = app.selection.iter().copied().find(|&i| {
            app.doc.shapes.get(i).map(|s| s.text_params().is_some()).unwrap_or(false)
        });
        let path_idx = app.selection.iter().copied().find(|&i| {
            app.doc
                .shapes
                .get(i)
                .map(|s| matches!(s, contour_app::document::Shape::Path { .. }))
                .unwrap_or(false)
        });
        if let (Some(tid), Some(pid)) = (text_idx, path_idx) {
            let attached = app.text_on_path.contains_key(&tid);
            let top_params = app.text_on_path_params.get(&tid).copied().unwrap_or_default();
            body = body.child(text_on_path_section(tid, pid, attached, top_params, cx));
        }
    }

    // ---- Stroke section (every variant has a stroke) -------------------
    if let Some(c) = stroke {
        body = body.child(color_section("Stroke", Target::Stroke, c, cx));
    }

    // ---- Stroke width --------------------------------------------------
    body = body.child(width_row(stroke_w, cx));

    // ---- Opacity -------------------------------------------------------
    if let Some(o) = opacity {
        body = body.child(opacity_row(o, cx));
    }

    // ---- Appearance panel (fills / strokes / effects stack) ------------
    body = body.child(appearance_section(shape, cx));

    // ---- Graphic styles ------------------------------------------------
    body = body.child(graphic_styles_section(&app.doc.graphic_styles, cx));

    // ---- Width profile presets -----------------------------------------
    body = body.child(width_preset_section(cx));

    div().flex().flex_col().child(header).child(body)
}

/// A labelled colour block: swatch + hex readout, R/G/B steppers, and a row of
/// preset swatches. Edits emit `SetFillColor` / `SetStrokeColor` per `target`.
fn color_section(
    title: &'static str,
    target: Target,
    c: [f32; 4],
    cx: &mut Context<Contour>,
) -> impl IntoElement {
    let (r8, g8, b8) = (to_u8(c[0]), to_u8(c[1]), to_u8(c[2]));

    let channels = [("R", 0usize), ("G", 1usize), ("B", 2usize)]
        .into_iter()
        .map(|(label, idx)| channel_row(title, target, label, idx, c, cx))
        .collect::<Vec<_>>();

    let presets: [[f32; 4]; 8] = [
        [0.0, 0.0, 0.0, c[3]],   // black
        [1.0, 1.0, 1.0, c[3]],   // white
        [0.5, 0.5, 0.5, c[3]],   // gray
        [1.0, 0.0, 0.0, c[3]],   // red
        [0.0, 0.8, 0.0, c[3]],   // green
        [0.0, 0.4, 1.0, c[3]],   // blue
        [1.0, 0.85, 0.0, c[3]],  // yellow
        [1.0, 0.0, 0.85, c[3]],  // magenta
    ];
    let preset_cells = presets
        .into_iter()
        .enumerate()
        .map(|(i, p)| swatch_cell((preset_id(title), i as u64), target, p, cx))
        .collect::<Vec<_>>();

    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(ui_section_header(title))
        .child(ui_divider())
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .size(px(40.0))
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(SWATCH_BORDER))
                        .bg(rgba(pack(c))),
                )
                .child(
                    div()
                        .text_color(colors::text_primary())
                        .child(format!("#{r8:02X}{g8:02X}{b8:02X}")),
                ),
        )
        .child(div().flex().flex_col().gap_1().children(channels))
        .child(
            div()
                .text_color(colors::text_secondary())
                .text_size(px(10.0))
                .child("Presets"),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap_1()
                .children(preset_cells),
        )
}

/// One R/G/B stepper row: `−  LABEL  value  ＋`, clamped to [0,1]. The new colour
/// is built from the current channels (alpha preserved) and emitted per `target`.
fn channel_row(
    section: &'static str,
    target: Target,
    label: &'static str,
    idx: usize,
    c: [f32; 4],
    cx: &mut Context<Contour>,
) -> impl IntoElement {
    let val = to_u8(c[idx]);

    let mk = |suffix: &'static str, glyph: &'static str, delta: f32| {
        // Distinct element id per (section, channel, button) so gpui's stateful
        // interactivity never collides between the Fill and Stroke sections.
        let id = ((section.as_bytes()[0] as u64) << 8) | (idx as u64) << 1;
        div()
            .id((suffix, id))
            .size(px(20.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .bg(colors::surface_raised())
            .border_1()
            .border_color(colors::surface_border())
            .text_color(colors::text_primary())
            .cursor_pointer()
            .hover(|s| s.bg(colors::surface_overlay()))
            .on_click(cx.listener(move |root, _ev, _win, cx| {
                let cur = match target {
                    Target::Fill => root.app.selected_fill(),
                    Target::Stroke => root.app.selected_stroke(),
                };
                if let Some(mut next) = cur {
                    next[idx] = (next[idx] + delta).clamp(0.0, 1.0);
                    root.app.apply(match target {
                        Target::Fill => Action::SetFillColor(next),
                        Target::Stroke => Action::SetStrokeColor(next),
                    });
                    cx.notify();
                }
            }))
            .child(glyph)
    };

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .child(div().w(px(14.0)).text_color(colors::text_secondary()).child(label))
        .child(mk("ch-dec", "−", -COLOR_STEP))
        .child(
            div()
                .flex_1()
                .flex()
                .justify_center()
                .text_color(colors::text_primary())
                .child(format!("{val}")),
        )
        .child(mk("ch-inc", "＋", COLOR_STEP))
}

/// A clickable preset swatch that sets the target colour (alpha preserved by the
/// preset itself). Distinct id-prefix per section keeps Fill / Stroke swatches
/// from colliding.
fn swatch_cell(
    id: (&'static str, u64),
    target: Target,
    color: [f32; 4],
    cx: &mut Context<Contour>,
) -> impl IntoElement {
    div()
        .id(id)
        .size(px(20.0))
        .rounded_md()
        .border_1()
        .border_color(rgb(SWATCH_BORDER))
        .bg(rgba(pack(color)))
        .cursor_pointer()
        .hover(|s| s.border_color(colors::text_primary()))
        .on_click(cx.listener(move |root, _ev, _win, cx| {
            // Keep the shape's existing alpha so a preset only changes hue.
            let cur = match target {
                Target::Fill => root.app.selected_fill(),
                Target::Stroke => root.app.selected_stroke(),
            };
            let mut next = color;
            if let Some(prev) = cur {
                next[3] = prev[3];
            }
            root.app.apply(match target {
                Target::Fill => Action::SetFillColor(next),
                Target::Stroke => Action::SetStrokeColor(next),
            });
            cx.notify();
        }))
}

/// Stroke-width stepper row: `−  Stroke W  value  ＋`, clamped ≥ 0.
fn width_row(w: f32, cx: &mut Context<Contour>) -> impl IntoElement {
    let mk = |suffix: &'static str, glyph: &'static str, delta: f32| {
        div()
            .id((suffix, 0u64))
            .size(px(20.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .bg(colors::surface_raised())
            .border_1()
            .border_color(colors::surface_border())
            .text_color(colors::text_primary())
            .cursor_pointer()
            .hover(|s| s.bg(colors::surface_overlay()))
            .on_click(cx.listener(move |root, _ev, _win, cx| {
                if let Some(s) = root.app.selected_shape() {
                    let next = (s.stroke_width() + delta).max(0.0);
                    root.app.apply(Action::SetStrokeWidth(next));
                    cx.notify();
                }
            }))
            .child(glyph)
    };

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
        .child(mk("sw-dec", "−", -WIDTH_STEP))
        .child(
            div()
                .flex_1()
                .flex()
                .justify_center()
                .text_color(colors::text_primary())
                .child(format!("{w:.1}")),
        )
        .child(mk("sw-inc", "＋", WIDTH_STEP))
}

/// Opacity stepper row: `−  Opacity  value  ＋`, clamped [0,1]. Mirrors the fill
/// alpha (see module docs).
fn opacity_row(o: f32, cx: &mut Context<Contour>) -> impl IntoElement {
    let pct = (o.clamp(0.0, 1.0) * 100.0).round() as u32;

    let mk = |suffix: &'static str, glyph: &'static str, delta: f32| {
        div()
            .id((suffix, 0u64))
            .size(px(20.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .bg(colors::surface_raised())
            .border_1()
            .border_color(colors::surface_border())
            .text_color(colors::text_primary())
            .cursor_pointer()
            .hover(|s| s.bg(colors::surface_overlay()))
            .on_click(cx.listener(move |root, _ev, _win, cx| {
                if let Some(cur) = root.app.selected_fill() {
                    let next = (cur[3] + delta).clamp(0.0, 1.0);
                    root.app.apply(Action::SetOpacity(next));
                    cx.notify();
                }
            }))
            .child(glyph)
    };

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .child(div().w(px(56.0)).text_color(colors::text_secondary()).child("Opacity"))
        .child(mk("op-dec", "−", -OPACITY_STEP))
        .child(
            div()
                .flex_1()
                .flex()
                .justify_center()
                .text_color(colors::text_primary())
                .child(format!("{pct}%")),
        )
        .child(mk("op-inc", "＋", OPACITY_STEP))
}

/// The Live Shape inspector: edit the primary polygon / star's parameters
/// (sides / points / radius / inner-ratio). Each `−/＋` click rebuilds the
/// `LiveShape` and emits `SetLiveShape`, which regenerates the outline live —
/// the GPUI mirror of the egui app's `live_shape_section`.
fn live_shape_section(live: LiveShape, cx: &mut Context<Contour>) -> impl IntoElement {
    let mut col = div()
        .flex()
        .flex_col()
        .gap_2()
        .child(ui_section_header("Live Shape"))
        .child(ui_divider());

    match live {
        LiveShape::Polygon { sides, radius } => {
            col = col
                .child(live_count_row(
                    "Sides",
                    sides,
                    cx,
                    move |n| LiveShape::Polygon { sides: n, radius },
                ))
                .child(live_radius_row(radius, cx, move |r| LiveShape::Polygon {
                    sides,
                    radius: r,
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

/// The Type inspector: edit the primary text object's font size (stepper),
/// alignment (Left / Center / Right toggles), and family (dropdown).
fn text_section(
    size: f32,
    align: TextAlign,
    font_family: Option<String>,
    font_dropdown_open: bool,
    cx: &mut Context<Contour>,
) -> impl IntoElement {
    // Font size stepper (step 2, clamped by the action).
    const SIZE_STEP: f32 = 2.0;
    let size_dec = step_button(("ts-dec", 0u64), "−", cx, move |root, cx| {
        if let Some(p) = root.app.primary_text_params() {
            let next = (p.font_size - SIZE_STEP).max(1.0);
            root.app.apply(Action::SetTextSize(next));
            cx.notify();
        }
    });
    let size_inc = step_button(("ts-inc", 0u64), "＋", cx, move |root, cx| {
        if let Some(p) = root.app.primary_text_params() {
            let next = p.font_size + SIZE_STEP;
            root.app.apply(Action::SetTextSize(next));
            cx.notify();
        }
    });
    let size_row = param_row("Size", format!("{size:.0}"), size_dec, size_inc);

    // Alignment toggles.
    let align_btns = TextAlign::ALL
        .into_iter()
        .map(|a| align_toggle(a, align == a, cx))
        .collect::<Vec<_>>();
    let align_row = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .child(div().w(px(40.0)).text_color(colors::text_secondary()).child("Align"))
        .child(div().flex().flex_row().gap_1().children(align_btns));

    // Font family: dropdown button + list.
    let current = font_family
        .clone()
        .unwrap_or_else(|| contour_app::fonts::DEFAULT_FONT_FAMILY.to_string());
    let current_label = current.clone();

    // Trigger button — shows current font, opens/closes the dropdown.
    let font_btn = div()
        .id("ts-font")
        .flex_1()
        .px_2()
        .h(px(20.0))
        .flex()
        .items_center()
        .justify_between()
        .rounded_md()
        .bg(colors::surface_raised())
        .border_1()
        .border_color(if font_dropdown_open {
            colors::accent()
        } else {
            colors::surface_border()
        })
        .text_color(colors::text_primary())
        .text_size(px(11.0))
        .cursor_pointer()
        .hover(|s| s.bg(colors::surface_overlay()))
        .on_click(cx.listener(|root, _ev, _win, cx| {
            root.app.apply(Action::ToggleFontDropdown);
            cx.notify();
        }))
        .child(current_label)
        .child(
            div()
                .text_color(colors::text_secondary())
                .text_size(px(9.0))
                .child(if font_dropdown_open { "▲" } else { "▼" }),
        );

    // Dropdown list — rendered only when open.
    let families = contour_app::fonts::families();
    let current_for_list = current.clone();
    let font_items = families.iter().enumerate().map(|(i, fam)| {
        let fam_clone = fam.clone();
        let is_active = *fam == current_for_list;
        div()
            .id(("ts-font-item", i as u64))
            .w_full()
            .px_2()
            .h(px(20.0))
            .flex()
            .items_center()
            .cursor_pointer()
            .bg(if is_active {
                colors::surface_overlay()
            } else {
                colors::surface_raised()
            })
            .text_color(if is_active {
                colors::text_primary()
            } else {
                colors::text_secondary()
            })
            .text_size(px(11.0))
            .hover(|s| s.bg(colors::tool_hover()).text_color(colors::text_primary()))
            .on_click(cx.listener(move |root, _ev, _win, cx| {
                let f = if fam_clone == contour_app::fonts::DEFAULT_FONT_FAMILY {
                    None
                } else {
                    Some(fam_clone.clone())
                };
                root.app.apply(Action::SetTextFont(f));
                cx.notify();
            }))
            .child(fam.clone())
    });

    // Floating popup: absolute-positioned relative to the font_row, painted
    // via deferred() so it draws on top of other inspector content.
    let font_popup = deferred(
        div()
            .id("font-popup-list")
            .absolute()
            .top(px(22.0))
            .left_0()
            .w(px(180.0))
            .h(px(160.0))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .bg(colors::surface_raised())
            .border_1()
            .border_color(colors::accent())
            .rounded_b_md()
            .shadow_md()
            // Consume scroll events so they don't bubble to the parent panel.
            .on_scroll_wheel(|_ev: &ScrollWheelEvent, _win, _cx| {})
            .children(font_items),
    ).with_priority(200);

    let font_row = div()
        .relative()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .child(div().w(px(40.0)).text_color(colors::text_secondary()).child("Font"))
        .child(
            div()
                .relative()
                .flex_1()
                .child(font_btn)
                .when(font_dropdown_open, |d| d.child(font_popup)),
        );

    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(ui_section_header("Type"))
        .child(ui_divider())
        .child(size_row)
        .child(align_row)
        .child(font_row)
}

/// One alignment toggle (Left / Center / Right), highlighted when active.
fn align_toggle(a: TextAlign, active: bool, cx: &mut Context<Contour>) -> impl IntoElement {
    let (bg, fg) = if active {
        (colors::surface_overlay(), colors::text_primary())
    } else {
        (colors::surface_raised(), colors::text_secondary())
    };
    div()
        .id(("ts-align", a as u64))
        .px_2()
        .h(px(20.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .bg(bg)
        .border_1()
        .border_color(colors::surface_border())
        .text_color(fg)
        .cursor_pointer()
        .hover(|s| s.bg(colors::surface_overlay()))
        .on_click(cx.listener(move |root, _ev, _win, cx| {
            root.app.apply(Action::SetTextAlign(a));
            cx.notify();
        }))
        .child(a.label())
}

/// The font family that follows the primary text object's current one in
/// `fonts::families()` (wrapping). `None` is stored for the bundled default so
/// new objects serialize identically.
fn next_font(root: &Contour) -> Option<String> {
    let fams = contour_app::fonts::families();
    if fams.is_empty() {
        return None;
    }
    let current = root
        .app
        .primary_text_params()
        .and_then(|p| p.font_family.clone())
        .unwrap_or_else(|| contour_app::fonts::DEFAULT_FONT_FAMILY.to_string());
    let pos = fams.iter().position(|f| *f == current).unwrap_or(0);
    let next = &fams[(pos + 1) % fams.len()];
    (next != contour_app::fonts::DEFAULT_FONT_FAMILY).then(|| next.clone())
}

/// A `−  LABEL  value  ＋` row shell shared by the live-shape and text steppers.
fn param_row(
    label: &'static str,
    value: String,
    dec: impl IntoElement,
    inc: impl IntoElement,
) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .child(div().w(px(40.0)).text_color(colors::text_secondary()).child(label))
        .child(dec)
        .child(
            div()
                .flex_1()
                .flex()
                .justify_center()
                .text_color(colors::text_primary())
                .child(value),
        )
        .child(inc)
}

/// A 20px square stepper button shell with the panel's hover idiom. `on_click`
/// routes through the choke point.
fn step_button(
    id: (&'static str, u64),
    glyph: &'static str,
    cx: &mut Context<Contour>,
    on_click: impl Fn(&mut Contour, &mut Context<Contour>) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .size(px(20.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .bg(colors::surface_raised())
        .border_1()
        .border_color(colors::surface_border())
        .text_color(colors::text_primary())
        .cursor_pointer()
        .hover(|s| s.bg(colors::surface_overlay()))
        .on_click(cx.listener(move |root, _ev, _win, cx| on_click(root, cx)))
        .child(glyph)
}

/// Stable id-prefix for a section's preset swatches (avoids Fill/Stroke clashes).
fn preset_id(section: &str) -> &'static str {
    match section {
        "Fill" => "insp-fill-preset",
        _ => "insp-stroke-preset",
    }
}

// ============================================================================
// Text on Path (Wave 9)
// ============================================================================

fn text_on_path_section(
    text_id: usize,
    path_id: usize,
    attached: bool,
    top: contour_app::text_on_path::TextOnPathParams,
    cx: &mut Context<Contour>,
) -> impl IntoElement {
    let btn_label = if attached { "Detach" } else { "Text on Path" };
    let mut col = div()
        .flex()
        .flex_col()
        .gap_2()
        .child(ui_section_header("Path Text"))
        .child(ui_divider())
        .child(
            div()
                .id(("top-btn", text_id as u64))
                .h(px(22.0))
                .px_2()
                .flex()
                .items_center()
                .justify_center()
                .rounded_md()
                .bg(colors::surface_raised())
                .border_1()
                .border_color(colors::surface_border())
                .text_color(colors::text_primary())
                .text_size(px(11.0))
                .cursor_pointer()
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    if attached {
                        root.app.apply(Action::DetachTextFromPath(text_id));
                    } else {
                        root.app.apply(Action::AttachTextToPath { text_id, path_id });
                    }
                    cx.notify();
                }))
                .child(btn_label),
        );
    // When attached, expose offset / start / flip controls (Batch 5).
    if attached {
        col = col
            .child(top_stepper_row(
                "Offset",
                text_id,
                format!("{:.0}", top.offset),
                cx,
                move |p| p.offset -= 4.0,
                move |p| p.offset += 4.0,
            ))
            .child(top_stepper_row(
                "Start",
                text_id,
                format!("{:.0}", top.start),
                cx,
                move |p| p.start = (p.start - 8.0).max(0.0),
                move |p| p.start += 8.0,
            ))
            .child(
                div()
                    .id(("top-flip", text_id as u64))
                    .h(px(22.0))
                    .px_2()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_md()
                    .bg(if top.flip {
                        colors::accent()
                    } else {
                        colors::surface_raised()
                    })
                    .border_1()
                    .border_color(colors::surface_border())
                    .text_color(colors::text_primary())
                    .text_size(px(11.0))
                    .cursor_pointer()
                    .on_click(cx.listener(move |root, _ev, _win, cx| {
                        let mut p = root
                            .app
                            .text_on_path_params
                            .get(&text_id)
                            .copied()
                            .unwrap_or_default();
                        p.flip = !p.flip;
                        root.app
                            .apply(Action::SetTextOnPathParams { text_id, params: p });
                        cx.notify();
                    }))
                    .child("Flip"),
            );
    }
    col
}

/// A `label  − value ＋` row that mutates the text-on-path params for `text_id`
/// (decrement / increment closures applied to a fresh copy), re-baking the glyphs.
fn top_stepper_row(
    label: &'static str,
    text_id: usize,
    value: String,
    cx: &mut Context<Contour>,
    dec: impl Fn(&mut contour_app::text_on_path::TextOnPathParams) + 'static,
    inc: impl Fn(&mut contour_app::text_on_path::TextOnPathParams) + 'static,
) -> impl IntoElement {
    let mk = |id: &'static str,
              glyph: &'static str,
              f: std::rc::Rc<dyn Fn(&mut contour_app::text_on_path::TextOnPathParams)>,
              cx: &mut Context<Contour>| {
        div()
            .id((id, text_id as u64))
            .size(px(20.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .bg(colors::surface_raised())
            .border_1()
            .border_color(colors::surface_border())
            .text_color(colors::text_primary())
            .cursor_pointer()
            .hover(|s| s.bg(colors::surface_overlay()))
            .on_click(cx.listener(move |root, _ev, _win, cx| {
                let mut p = root
                    .app
                    .text_on_path_params
                    .get(&text_id)
                    .copied()
                    .unwrap_or_default();
                f(&mut p);
                root.app
                    .apply(Action::SetTextOnPathParams { text_id, params: p });
                cx.notify();
            }))
            .child(glyph)
    };
    let dec: std::rc::Rc<dyn Fn(&mut contour_app::text_on_path::TextOnPathParams)> =
        std::rc::Rc::new(dec);
    let inc: std::rc::Rc<dyn Fn(&mut contour_app::text_on_path::TextOnPathParams)> =
        std::rc::Rc::new(inc);
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .child(
            div()
                .w(px(48.0))
                .text_color(colors::text_secondary())
                .text_size(px(10.0))
                .child(label),
        )
        .child(mk("top-dec", "−", dec, cx))
        .child(div().flex_1().flex().justify_center().text_size(px(11.0)).child(value))
        .child(mk("top-inc", "＋", inc, cx))
}

// ============================================================================
// Gradient-stop editor (Wave 8)
// ============================================================================

/// "Add Gradient" button — seeds a Linear gradient from the shape's current solid fill.
fn add_gradient_button(shape_id: usize, cx: &mut Context<Contour>) -> impl IntoElement {
    use crate::app_state::Action;
    use contour_app::gradient::GradientKind;
    div()
        .flex()
        .px_2()
        .pb_1()
        .child(
            div()
                .px_2()
                .py_1()
                .rounded_sm()
                .bg(colors::surface_raised())
                .text_color(colors::text_secondary())
                .text_size(px(10.0))
                .cursor_pointer()
                .on_mouse_down(gpui::MouseButton::Left, cx.listener(move |root, _, _, cx| {
                    root.app.apply(Action::SetGradientType {
                        shape_id,
                        kind: GradientKind::Linear,
                    });
                    cx.notify();
                }))
                .child("+ Add Gradient"),
        )
}

/// Gradient type toggle: Linear / Radial / Angle buttons (Wave 11).
fn gradient_type_section(
    shape_id: usize,
    current: GradientKind,
    cx: &mut Context<Contour>,
) -> impl IntoElement {
    let kinds = [GradientKind::Linear, GradientKind::Radial, GradientKind::Angle];
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(ui_section_header("Gradient Type"))
        .child(ui_divider())
        .child(
            div()
                .flex()
                .gap_1()
                .px_3()
                .py_1()
                .children(kinds.into_iter().enumerate().map(|(i, kind)| {
                    let is_active = kind == current;
                    let bg = if is_active { colors::accent() } else { colors::surface_raised() };
                    div()
                        .id(("grad-type", i))
                        .px_2()
                        .h(px(22.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(bg)
                        .rounded_md()
                        .text_color(colors::text_primary())
                        .text_size(px(10.0))
                        .cursor_pointer()
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            root.app.apply(Action::SetGradientType { shape_id, kind });
                            cx.notify();
                        }))
                        .child(kind.label())
                })),
        )
}

/// The gradient-stop editor section: a horizontal bar visualising the gradient,
/// stop diamonds to select a stop, and controls to add / move / delete / recolour
/// the selected stop.
fn gradient_editor_section(
    shape_id: usize,
    gradient: &Gradient,
    selected_stop: Option<usize>,
    cx: &mut Context<Contour>,
) -> impl IntoElement {
    const BAR_W: f32 = 220.0;
    const BAR_H: f32 = 16.0;
    const STOP_STEP: f32 = 0.05;

    // ---- Gradient bar: divide into segments coloured by interpolated sample --
    let sorted = gradient.sorted_stops();
    let n_seg = (sorted.len() * 4).max(8).min(32) as usize;
    let segments = (0..n_seg)
        .map(|k| {
            let t = k as f32 / n_seg as f32;
            let c = gradient.color_at(t);
            div()
                .flex_1()
                .h(px(BAR_H))
                .bg(rgba(pack_rgba(c)))
        })
        .collect::<Vec<_>>();

    let bar = div()
        .id(("grad-bar", shape_id as u64))
        .w(px(BAR_W))
        .h(px(BAR_H))
        .flex()
        .flex_row()
        .rounded_md()
        .overflow_hidden()
        .border_1()
        .border_color(rgb(SWATCH_BORDER))
        .cursor_pointer()
        // Click on the bar → add a stop at the clicked position (approximated
        // as 0.5 since GPUI 0.2.x click events don't expose local offset).
        .on_click(cx.listener(move |root, _ev, _win, cx| {
            if let Some(g) = root.app.doc.shapes.get(shape_id).and_then(|s| s.fill_gradient()) {
                let color = g.color_at(0.5);
                root.app.apply(Action::AddGradientStop {
                    shape_id,
                    pos: 0.5,
                    color,
                });
                cx.notify();
            }
        }))
        .children(segments);

    // ---- Stop diamonds (small coloured squares below the bar) ---------------
    let stop_diamonds = sorted
        .iter()
        .enumerate()
        .map(|(si, stop)| {
            let is_sel = Some(si) == selected_stop;
            let border_col = if is_sel { colors::accent() } else { rgb(0x555555u32) };
            div()
                .id(("grad-stop", si as u64))
                .size(px(12.0))
                .rounded_sm()
                .border_2()
                .border_color(border_col)
                .bg(rgba(pack_rgba(stop.color)))
                .cursor_pointer()
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    let next = if root.app.selected_gradient_stop == Some(si) {
                        None
                    } else {
                        Some(si)
                    };
                    root.app.apply(Action::SelectGradientStop(next));
                    cx.notify();
                }))
        })
        .collect::<Vec<_>>();

    let stops_row = div()
        .flex()
        .flex_row()
        .gap_1()
        .items_center()
        .children(stop_diamonds);

    // ---- Control buttons (move left/right, add, delete) --------------------
    let move_left = step_button(
        ("gd-ml", shape_id as u64),
        "◀",
        cx,
        move |root, cx| {
            if let Some(si) = root.app.selected_gradient_stop {
                let g = root.app.doc.shapes.get(shape_id).and_then(|s| s.fill_gradient()).cloned();
                if let Some(g) = g {
                    if let Some(s) = g.stops.get(si) {
                        let new_pos = (s.offset - STOP_STEP).max(0.0);
                        root.app.apply(Action::MoveGradientStop { shape_id, stop_idx: si, new_pos });
                        cx.notify();
                    }
                }
            }
        },
    );
    let move_right = step_button(
        ("gd-mr", shape_id as u64),
        "▶",
        cx,
        move |root, cx| {
            if let Some(si) = root.app.selected_gradient_stop {
                let g = root.app.doc.shapes.get(shape_id).and_then(|s| s.fill_gradient()).cloned();
                if let Some(g) = g {
                    if let Some(s) = g.stops.get(si) {
                        let new_pos = (s.offset + STOP_STEP).min(1.0);
                        root.app.apply(Action::MoveGradientStop { shape_id, stop_idx: si, new_pos });
                        cx.notify();
                    }
                }
            }
        },
    );

    let n_stops = gradient.stops.len();
    let can_delete = n_stops > 2 && selected_stop.is_some();
    let del_fg = if can_delete { colors::text_primary() } else { colors::text_secondary() };
    let del_btn = {
        let mut b = div()
            .id(("gd-del", shape_id as u64))
            .size(px(20.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .bg(colors::surface_raised())
            .border_1()
            .border_color(colors::surface_border())
            .text_color(del_fg)
            .text_size(px(11.0))
            .child("✕");
        if can_delete {
            let si = selected_stop.unwrap();
            b = b.cursor_pointer().on_click(cx.listener(move |root, _ev, _win, cx| {
                root.app.apply(Action::DeleteGradientStop { shape_id, stop_idx: si });
                root.app.apply(Action::SelectGradientStop(None));
                cx.notify();
            }));
        }
        b
    };

    let controls = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .child(div().flex_1().text_color(colors::text_secondary()).text_size(px(10.0)).child("Stops:"))
        .child(move_left)
        .child(move_right)
        .child(del_btn);

    // ---- Colour steppers for the selected stop ----------------------------
    let stop_color_editor = selected_stop.and_then(|si| {
        gradient.stops.get(si).map(|stop| {
            let c = stop.color;
            let channels = [("R", 0usize), ("G", 1usize), ("B", 2usize)]
                .into_iter()
                .map(|(lbl, ch)| {
                    let val = (c[ch].clamp(0.0, 1.0) * 255.0).round() as u32;
                    const CSTEP: f32 = 8.0 / 255.0;
                    let dec = div()
                        .id(("gd-ch-dec", (si * 3 + ch) as u64))
                        .size(px(20.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_md()
                        .bg(colors::surface_raised())
                        .border_1()
                        .border_color(colors::surface_border())
                        .text_color(colors::text_primary())
                        .cursor_pointer()
                        .hover(|s| s.bg(colors::surface_overlay()))
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            let g = root.app.doc.shapes.get(shape_id).and_then(|s| s.fill_gradient()).cloned();
                            if let Some(g) = g {
                                if let Some(s) = g.stops.get(si) {
                                    let mut nc = s.color;
                                    nc[ch] = (nc[ch] - CSTEP).clamp(0.0, 1.0);
                                    root.app.apply(Action::SetGradientStopColor { shape_id, stop_idx: si, color: nc });
                                    cx.notify();
                                }
                            }
                        }))
                        .child("−");
                    let inc = div()
                        .id(("gd-ch-inc", (si * 3 + ch) as u64))
                        .size(px(20.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_md()
                        .bg(colors::surface_raised())
                        .border_1()
                        .border_color(colors::surface_border())
                        .text_color(colors::text_primary())
                        .cursor_pointer()
                        .hover(|s| s.bg(colors::surface_overlay()))
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            let g = root.app.doc.shapes.get(shape_id).and_then(|s| s.fill_gradient()).cloned();
                            if let Some(g) = g {
                                if let Some(s) = g.stops.get(si) {
                                    let mut nc = s.color;
                                    nc[ch] = (nc[ch] + CSTEP).clamp(0.0, 1.0);
                                    root.app.apply(Action::SetGradientStopColor { shape_id, stop_idx: si, color: nc });
                                    cx.notify();
                                }
                            }
                        }))
                        .child("＋");
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(div().w(px(14.0)).text_color(colors::text_secondary()).child(lbl))
                        .child(dec)
                        .child(div().flex_1().flex().justify_center().text_color(colors::text_primary()).child(format!("{val}")))
                        .child(inc)
                })
                .collect::<Vec<_>>();
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(div().text_color(colors::text_secondary()).text_size(px(10.0)).child("Stop colour:"))
                .children(channels)
        })
    });

    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(ui_section_header("Gradient Stops"))
        .child(ui_divider())
        .child(bar)
        .child(stops_row)
        .child(controls)
        .children(stop_color_editor)
}

/// Pack straight sRGB RGBA `[f32;4]` → `0xAARRGGBB` for `gpui::rgba`.
fn pack_rgba(c: [f32; 4]) -> u32 {
    let to_u8 = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u32;
    let (r, g, b, a) = (to_u8(c[0]), to_u8(c[1]), to_u8(c[2]), to_u8(c[3]));
    (r << 24) | (g << 16) | (b << 8) | a
}

// ── Appearance panel ──────────────────────────────────────────────────────────

/// Appearance panel: shows the fill/stroke/effect stack for the selected shape.
/// Migrates a legacy shape to an explicit stack on first interaction.
pub fn appearance_section(
    shape: &contour_app::document::Shape,
    cx: &mut Context<Contour>,
) -> impl IntoElement {
    let appearance = shape.effective_appearance();

    // Fill rows
    let fill_rows = appearance.fills.iter().cloned().enumerate().map(|(i, fill)| {
        let swatch_color = fill.paint.swatch();
        let vis = fill.visible;
        let label = if vis { "●" } else { "○" };
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .px_1()
            .rounded_sm()
            .bg(colors::surface_overlay())
            .child(
                // Visibility toggle
                div()
                    .id(("fill-vis", i as u64))
                    .w(px(14.0))
                    .text_size(px(10.0))
                    .text_color(if vis { colors::text_primary() } else { colors::text_disabled() })
                    .cursor_pointer()
                    .on_click(cx.listener(move |root, _ev, _win, cx| {
                        root.app.apply(Action::AppearanceToggleFill(i));
                        cx.notify();
                    }))
                    .child(label),
            )
            .child(
                // Color swatch
                div()
                    .id(("fill-swatch", i as u64))
                    .size(px(14.0))
                    .rounded_sm()
                    .border_1()
                    .border_color(rgb(0x555555u32))
                    .bg(rgba(pack_rgba(swatch_color)))
                    .cursor_pointer()
                    .on_click(cx.listener(move |root, _ev, _win, cx| {
                        // Cycle color for demo (real: open color picker)
                        let c = [
                            (swatch_color[0] + 0.2) % 1.0,
                            (swatch_color[1] + 0.3) % 1.0,
                            (swatch_color[2] + 0.1) % 1.0,
                            1.0,
                        ];
                        root.app.apply(Action::AppearanceSetFillColor { idx: i, color: c });
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .flex_1()
                    .text_size(px(10.0))
                    .text_color(colors::text_primary())
                    .child(format!("Fill {}", i + 1)),
            )
            .child(
                // Remove button
                div()
                    .id(("fill-rm", i as u64))
                    .px_1()
                    .text_size(px(10.0))
                    .text_color(colors::text_disabled())
                    .cursor_pointer()
                    .on_click(cx.listener(move |root, _ev, _win, cx| {
                        root.app.apply(Action::AppearanceRemoveFill(i));
                        cx.notify();
                    }))
                    .child("×"),
            )
    }).collect::<Vec<_>>();

    // Stroke rows
    let stroke_rows = appearance.strokes.iter().cloned().enumerate().map(|(i, stroke)| {
        let swatch_color = stroke.paint.swatch();
        let vis = stroke.visible;
        let label = if vis { "●" } else { "○" };
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .px_1()
            .rounded_sm()
            .bg(colors::surface_overlay())
            .child(
                div()
                    .id(("stroke-vis", i as u64))
                    .w(px(14.0))
                    .text_size(px(10.0))
                    .text_color(if vis { colors::text_primary() } else { colors::text_disabled() })
                    .cursor_pointer()
                    .on_click(cx.listener(move |root, _ev, _win, cx| {
                        root.app.apply(Action::AppearanceToggleStroke(i));
                        cx.notify();
                    }))
                    .child(label),
            )
            .child(
                div()
                    .id(("stroke-swatch", i as u64))
                    .size(px(14.0))
                    .rounded_sm()
                    .border_1()
                    .border_color(rgb(0x555555u32))
                    .bg(rgba(pack_rgba(swatch_color)))
                    .cursor_pointer()
                    .on_click(cx.listener(move |root, _ev, _win, cx| {
                        let c = [
                            (swatch_color[0] + 0.2) % 1.0,
                            (swatch_color[1] + 0.3) % 1.0,
                            (swatch_color[2] + 0.1) % 1.0,
                            1.0,
                        ];
                        root.app.apply(Action::AppearanceSetStrokeColor { idx: i, color: c });
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .flex_1()
                    .text_size(px(10.0))
                    .text_color(colors::text_primary())
                    .child(format!("Stroke {} ({:.1}px)", i + 1, stroke.width)),
            )
            .child(
                div()
                    .id(("stroke-rm", i as u64))
                    .px_1()
                    .text_size(px(10.0))
                    .text_color(colors::text_disabled())
                    .cursor_pointer()
                    .on_click(cx.listener(move |root, _ev, _win, cx| {
                        root.app.apply(Action::AppearanceRemoveStroke(i));
                        cx.notify();
                    }))
                    .child("×"),
            )
    }).collect::<Vec<_>>();

    // Effect rows
    let effect_rows = appearance.effects.iter().enumerate().map(|(i, effect)| {
        let label = effect.label().to_string();
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .px_1()
            .rounded_sm()
            .bg(colors::surface_overlay())
            .child(
                div()
                    .flex_1()
                    .text_size(px(10.0))
                    .text_color(colors::text_primary())
                    .child(label),
            )
            .child(
                div()
                    .id(("fx-rm", i as u64))
                    .px_1()
                    .text_size(px(10.0))
                    .text_color(colors::text_disabled())
                    .cursor_pointer()
                    .on_click(cx.listener(move |root, _ev, _win, cx| {
                        root.app.apply(Action::AppearanceRemoveEffect(i));
                        cx.notify();
                    }))
                    .child("×"),
            )
    }).collect::<Vec<_>>();

    // Add-entry buttons row
    let add_fill_btn = div()
        .id("ap-add-fill")
        .px_2()
        .py(px(2.0))
        .rounded_sm()
        .bg(colors::surface_overlay())
        .text_color(colors::text_primary())
        .text_size(px(10.0))
        .cursor_pointer()
        .on_click(cx.listener(|root, _ev, _win, cx| {
            root.app.apply(Action::AppearanceAddFill);
            cx.notify();
        }))
        .child("+ Fill");

    let add_stroke_btn = div()
        .id("ap-add-stroke")
        .px_2()
        .py(px(2.0))
        .rounded_sm()
        .bg(colors::surface_overlay())
        .text_color(colors::text_primary())
        .text_size(px(10.0))
        .cursor_pointer()
        .on_click(cx.listener(|root, _ev, _win, cx| {
            root.app.apply(Action::AppearanceAddStroke);
            cx.notify();
        }))
        .child("+ Stroke");

    let add_shadow_btn = div()
        .id("ap-add-shadow")
        .px_2()
        .py(px(2.0))
        .rounded_sm()
        .bg(colors::surface_overlay())
        .text_color(colors::text_primary())
        .text_size(px(10.0))
        .cursor_pointer()
        .on_click(cx.listener(|root, _ev, _win, cx| {
            root.app.apply(Action::AppearanceAddDropShadow);
            cx.notify();
        }))
        .child("+ Shadow");

    let add_blur_btn = div()
        .id("ap-add-blur")
        .px_2()
        .py(px(2.0))
        .rounded_sm()
        .bg(colors::surface_overlay())
        .text_color(colors::text_primary())
        .text_size(px(10.0))
        .cursor_pointer()
        .on_click(cx.listener(|root, _ev, _win, cx| {
            root.app.apply(Action::AppearanceAddBlur);
            cx.notify();
        }))
        .child("+ Blur");

    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(ui_section_header("Appearance"))
        .child(ui_divider())
        .children(fill_rows)
        .children(stroke_rows)
        .children(effect_rows)
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap_1()
                .child(add_fill_btn)
                .child(add_stroke_btn)
                .child(add_shadow_btn)
                .child(add_blur_btn),
        )
}

// ── Graphic styles panel ───────────────────────────────────────────────────────

/// Graphic styles strip: shows the document's style thumbnails in a scrollable row.
/// Clicking a style applies it to the selected shape. "New" saves the current appearance.
pub fn graphic_styles_section(
    styles: &contour_app::graphic_styles::GraphicStyles,
    cx: &mut Context<Contour>,
) -> impl IntoElement {
    let style_chips = styles.list.iter().cloned().map(|style| {
        let swatch = style.appearance.fills.first()
            .map(|f| f.paint.swatch())
            .unwrap_or([0.4, 0.4, 0.4, 1.0]);
        let id = style.id;
        let name = style.name.clone();
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap_1()
            .w(px(44.0))
            .child(
                div()
                    .id(("gs-chip", id))
                    .size(px(36.0))
                    .rounded_sm()
                    .border_1()
                    .border_color(colors::surface_border())
                    .bg(rgba(pack_rgba(swatch)))
                    .cursor_pointer()
                    .on_click(cx.listener(move |root, _ev, _win, cx| {
                        root.app.apply(Action::ApplyGraphicStyle(id));
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .w(px(44.0))
                    .text_size(px(9.0))
                    .text_color(colors::text_secondary())
                    .overflow_hidden()
                    .child(name),
            )
    }).collect::<Vec<_>>();

    let new_style_btn = div()
        .id("gs-new")
        .px_2()
        .py(px(2.0))
        .rounded_sm()
        .bg(colors::accent())
        .text_color(colors::text_primary())
        .text_size(px(10.0))
        .cursor_pointer()
        .on_click(cx.listener(|root, _ev, _win, cx| {
            root.app.apply(Action::SaveGraphicStyle("Style".into()));
            cx.notify();
        }))
        .child("New Style");

    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(ui_section_header("Graphic Styles"))
        .child(ui_divider())
        .child(
            div()
                .flex()
                .flex_row()
                .gap_1()
                .child(new_style_btn),
        )
        .when(!styles.is_empty(), |el| {
            el.child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .gap_2()
                    .children(style_chips),
            )
        })
}

/// Width Profile Presets row in the inspector.
fn width_preset_section(cx: &mut Context<Contour>) -> impl IntoElement {
    let btns = WidthProfilePreset::ALL.iter().enumerate().map(|(i, &preset)| {
        div()
            .id(("wp-preset", i as u64))
            .px_2()
            .py(px(2.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded_sm()
            .bg(colors::surface_raised())
            .border_1()
            .border_color(colors::surface_border())
            .text_color(colors::text_primary())
            .text_size(px(10.0))
            .cursor_pointer()
            .hover(|s| s.bg(colors::surface_overlay()))
            .on_click(cx.listener(move |root, _ev, _win, cx| {
                root.app.apply(Action::ApplyWidthPreset(preset));
                cx.notify();
            }))
            .child(preset.label())
    }).collect::<Vec<_>>();

    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(ui_section_header("Width Presets"))
        .child(ui_divider())
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap_1()
                .px_2()
                .pb_2()
                .children(btns),
        )
}
