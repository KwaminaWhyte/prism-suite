//! Inspector — text and text-on-path sections. Split out of `inspector.rs`.

use super::*;

/// The Type inspector: edit the primary text object's font size (stepper),
/// alignment (Left / Center / Right toggles), and family (dropdown).
pub(super) fn text_section(
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
        .unwrap_or_else(|| crate::fonts::DEFAULT_FONT_FAMILY.to_string());
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
    let families = crate::fonts::families();
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
                let f = if fam_clone == crate::fonts::DEFAULT_FONT_FAMILY {
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
    let fams = crate::fonts::families();
    if fams.is_empty() {
        return None;
    }
    let current = root
        .app
        .primary_text_params()
        .and_then(|p| p.font_family.clone())
        .unwrap_or_else(|| crate::fonts::DEFAULT_FONT_FAMILY.to_string());
    let pos = fams.iter().position(|f| *f == current).unwrap_or(0);
    let next = &fams[(pos + 1) % fams.len()];
    (next != crate::fonts::DEFAULT_FONT_FAMILY).then(|| next.clone())
}

pub(super) fn text_on_path_section(
    text_id: usize,
    path_id: usize,
    attached: bool,
    top: crate::text_on_path::TextOnPathParams,
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
    dec: impl Fn(&mut crate::text_on_path::TextOnPathParams) + 'static,
    inc: impl Fn(&mut crate::text_on_path::TextOnPathParams) + 'static,
) -> impl IntoElement {
    let mk = |id: &'static str,
              glyph: &'static str,
              f: std::rc::Rc<dyn Fn(&mut crate::text_on_path::TextOnPathParams)>,
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
    let dec: std::rc::Rc<dyn Fn(&mut crate::text_on_path::TextOnPathParams)> =
        std::rc::Rc::new(dec);
    let inc: std::rc::Rc<dyn Fn(&mut crate::text_on_path::TextOnPathParams)> =
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
