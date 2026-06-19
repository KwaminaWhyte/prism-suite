//! Camera Raw–style develop dialog (Batch 3).
//!
//! A floating overlay with Basic, Detail, and HSL/Color sections that map
//! to the existing `camera_raw_params` HashMap and adjustment actions.
//! Opened via Filter ▸ Camera Raw Filter… (Shift+Cmd+A) or `Action::ToggleCameraRawDialog`.

use gpui::{
    deferred, div, px, Context, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled,
};
use prism_ui::colors;

use crate::app_state::{Action, App};
use crate::Pigment;

/// A single slider row: label + decrement/value/increment controls.
fn slider_row(
    key: &'static str,
    label: &'static str,
    val: f32,
    step: f32,
    min: f32,
    max: f32,
    cx: &mut Context<Pigment>,
) -> impl IntoElement {
    let dec_id = SharedString::from(format!("cr-dec-{key}"));
    let inc_id = SharedString::from(format!("cr-inc-{key}"));
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .py(px(1.0))
        .child(
            div()
                .w(px(100.0))
                .text_size(px(10.5))
                .text_color(colors::text_secondary())
                .child(label),
        )
        .child(
            div()
                .id(dec_id)
                .w(px(16.0))
                .h(px(16.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded_sm()
                .bg(colors::surface_overlay())
                .text_color(colors::text_primary())
                .text_size(px(10.0))
                .cursor_pointer()
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    let cur = root.app.camera_raw_params.get(key).copied().unwrap_or(0.0);
                    let nv = (cur - step).clamp(min, max);
                    root.app.apply(Action::SetCameraRawParam(key, nv));
                    cx.notify();
                }))
                .child("−"),
        )
        .child(
            div()
                .w(px(36.0))
                .text_size(px(10.0))
                .text_color(colors::text_primary())
                .child(format!("{val:.1}")),
        )
        .child(
            div()
                .id(inc_id)
                .w(px(16.0))
                .h(px(16.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded_sm()
                .bg(colors::surface_overlay())
                .text_color(colors::text_primary())
                .text_size(px(10.0))
                .cursor_pointer()
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    let cur = root.app.camera_raw_params.get(key).copied().unwrap_or(0.0);
                    let nv = (cur + step).clamp(min, max);
                    root.app.apply(Action::SetCameraRawParam(key, nv));
                    cx.notify();
                }))
                .child("+"),
        )
}

fn section_header(
    label: &'static str,
    section_key: &'static str,
    expanded: bool,
    cx: &mut Context<Pigment>,
) -> impl IntoElement {
    let arrow = if expanded { "▼" } else { "▶" };
    div()
        .id(SharedString::from(format!("cr-sec-{section_key}")))
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .py(px(3.0))
        .cursor_pointer()
        .on_click(cx.listener(move |root, _ev, _win, cx| {
            root.app.apply(Action::ToggleCameraRawSection(section_key));
            cx.notify();
        }))
        .child(
            div()
                .text_size(px(9.0))
                .text_color(colors::text_secondary())
                .child(arrow),
        )
        .child(
            div()
                .text_size(px(11.0))
                .text_color(colors::text_primary())
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child(label),
        )
}

/// Render the Camera Raw dialog as a deferred overlay.
pub fn render(app: &App, cx: &mut Context<Pigment>) -> impl IntoElement {
    let get = |k: &'static str| app.camera_raw_params.get(k).copied().unwrap_or(0.0);

    // Section open state
    let sec_basic  = app.camera_raw_section_basic;
    let sec_detail = app.camera_raw_section_detail;
    let sec_hsl    = app.camera_raw_section_hsl;

    // Pre-build all rows before building the div chain.
    let mut content_rows: Vec<gpui::AnyElement> = Vec::new();

    // --- Basic section ---
    content_rows.push(section_header("Basic", "basic", sec_basic, cx).into_any_element());
    if sec_basic {
        let basic_params: &[(&'static str, &'static str, f32, f32, f32)] = &[
            ("exposure",   "Exposure",   5.0, -5.0, 5.0),
            ("contrast",   "Contrast",   1.0, -100.0, 100.0),
            ("highlights", "Highlights", 1.0, -100.0, 100.0),
            ("shadows",    "Shadows",    1.0, -100.0, 100.0),
            ("whites",     "Whites",     1.0, -100.0, 100.0),
            ("blacks",     "Blacks",     1.0, -100.0, 100.0),
        ];
        for (key, label, step, min, max) in basic_params {
            let val = get(key);
            content_rows.push(slider_row(key, label, val, *step, *min, *max, cx).into_any_element());
        }
    }

    // --- Detail section ---
    content_rows.push(section_header("Detail", "detail", sec_detail, cx).into_any_element());
    if sec_detail {
        let detail_params: &[(&'static str, &'static str, f32, f32, f32)] = &[
            ("sharpening_amount", "Sharpening",     1.0, 0.0, 150.0),
            ("sharpening_radius", "Radius",         0.1, 0.5, 3.0),
            ("noise_luminance",   "Noise Reduction", 1.0, 0.0, 100.0),
        ];
        for (key, label, step, min, max) in detail_params {
            let val = get(key);
            content_rows.push(slider_row(key, label, val, *step, *min, *max, cx).into_any_element());
        }
    }

    // --- HSL/Color section (collapsed by default, 24 sliders) ---
    content_rows.push(section_header("HSL / Color", "hsl", sec_hsl, cx).into_any_element());
    if sec_hsl {
        let colors_list = ["Red", "Orange", "Yellow", "Green", "Aqua", "Blue", "Purple", "Magenta"];
        let keys_hue = ["hue_red","hue_orange","hue_yellow","hue_green","hue_aqua","hue_blue","hue_purple","hue_magenta"];
        let keys_sat = ["sat_red","sat_orange","sat_yellow","sat_green","sat_aqua","sat_blue","sat_purple","sat_magenta"];
        let keys_lum = ["lum_red","lum_orange","lum_yellow","lum_green","lum_aqua","lum_blue","lum_purple","lum_magenta"];

        // Hue sub-header
        content_rows.push(
            div()
                .text_size(px(10.0))
                .text_color(colors::text_secondary())
                .py(px(2.0))
                .child("Hue")
                .into_any_element(),
        );
        for (i, color_name) in colors_list.iter().enumerate() {
            let key = keys_hue[i];
            let label: &'static str = Box::leak(format!("  {}", color_name).into_boxed_str());
            let val = get(key);
            content_rows.push(slider_row(key, label, val, 1.0, -100.0, 100.0, cx).into_any_element());
        }
        content_rows.push(
            div()
                .text_size(px(10.0))
                .text_color(colors::text_secondary())
                .py(px(2.0))
                .child("Saturation")
                .into_any_element(),
        );
        for (i, color_name) in colors_list.iter().enumerate() {
            let key = keys_sat[i];
            let label: &'static str = Box::leak(format!("  {}", color_name).into_boxed_str());
            let val = get(key);
            content_rows.push(slider_row(key, label, val, 1.0, -100.0, 100.0, cx).into_any_element());
        }
        content_rows.push(
            div()
                .text_size(px(10.0))
                .text_color(colors::text_secondary())
                .py(px(2.0))
                .child("Luminance")
                .into_any_element(),
        );
        for (i, color_name) in colors_list.iter().enumerate() {
            let key = keys_lum[i];
            let label: &'static str = Box::leak(format!("  {}", color_name).into_boxed_str());
            let val = get(key);
            content_rows.push(slider_row(key, label, val, 1.0, -100.0, 100.0, cx).into_any_element());
        }
    }

    deferred(
        div()
            .absolute()
            .top(px(60.0))
            .left(px(120.0))
            .w(px(340.0))
            .bg(colors::surface_raised())
            .border_1()
            .border_color(colors::surface_border())
            .rounded_md()
            .shadow_md()
            .flex()
            .flex_col()
            // Title bar
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(colors::surface_border())
                    .child(
                        div()
                            .text_color(colors::text_primary())
                            .text_size(px(12.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("Camera Raw Filter"),
                    )
                    .child(
                        div()
                            .id("cr-close-btn")
                            .px_2()
                            .py(px(2.0))
                            .rounded_sm()
                            .bg(colors::surface_overlay())
                            .text_color(colors::text_secondary())
                            .text_size(px(10.0))
                            .cursor_pointer()
                            .on_click(cx.listener(|root, _ev, _win, cx| {
                                root.app.apply(Action::CloseCameraRaw);
                                cx.notify();
                            }))
                            .child("✕"),
                    ),
            )
            // Scrollable body
            .child(
                div()
                    .id("cr-body")
                    .flex_1()
                    .overflow_y_scroll()
                    .max_h(px(440.0))
                    .flex()
                    .flex_col()
                    .px_3()
                    .py_2()
                    .children(content_rows),
            )
            // Footer buttons
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_end()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .border_t_1()
                    .border_color(colors::surface_border())
                    .child(
                        div()
                            .id("cr-apply-btn")
                            .px_3()
                            .py(px(4.0))
                            .rounded_md()
                            .bg(colors::accent())
                            .text_color(colors::text_primary())
                            .text_size(px(11.0))
                            .cursor_pointer()
                            .on_click(cx.listener(|root, _ev, _win, cx| {
                                root.app.apply(Action::ApplyCameraRaw);
                                cx.notify();
                            }))
                            .child("Apply"),
                    )
                    .child(
                        div()
                            .id("cr-cancel-btn")
                            .px_3()
                            .py(px(4.0))
                            .rounded_md()
                            .bg(colors::surface_overlay())
                            .text_color(colors::text_secondary())
                            .text_size(px(11.0))
                            .cursor_pointer()
                            .on_click(cx.listener(|root, _ev, _win, cx| {
                                root.app.apply(Action::CloseCameraRaw);
                                cx.notify();
                            }))
                            .child("Cancel"),
                    ),
            ),
    )
    .with_priority(400)
}
