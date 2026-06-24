//! Inspector — appearance stack, graphic styles, width presets. Split out of `inspector.rs`.

use super::*;

/// Appearance panel: shows the fill/stroke/effect stack for the selected shape.
/// Migrates a legacy shape to an explicit stack on first interaction.
pub fn appearance_section(
    shape: &crate::document::Shape,
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
    styles: &crate::graphic_styles::GraphicStyles,
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
pub(super) fn width_preset_section(cx: &mut Context<Contour>) -> impl IntoElement {
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
