//! Output Module dialog — a floating panel that configures render-queue output
//! modules (After Effects' *Output Module Settings*).
//!
//! Lists every [`OutputModule`](crate::app_state::OutputModule) in
//! `app.output_modules` and, for each, exposes format / codec / depth / scale /
//! range / audio controls that emit the existing `SetOutputModule*` actions. A
//! "+ Add Module" button appends a default module via `AddOutputModule`; each
//! module row has a remove (×) button (`RemoveOutputModule`).
//!
//! Like the other Pulse panels this is a `render(app, cx)` free function that
//! reads `&App` and emits [`Action`](crate::app_state::Action)s through
//! `cx.listener`. It is shown as a floating `egui::Window`-equivalent: a panel
//! gated by `app.output_module_open`, toggled from the toolbar.

use gpui::{
    div, px, rgb, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};

use prism_ui::{colors, section_header};

use crate::app_state::{
    Action, App, ColorDepth, OutputCodec, OutputModule, OutputModuleFormat,
};
use crate::panels::BG_ACTIVE;
use crate::Pulse;

/// The codecs offered in the codec picker (all variants except `None`, which is
/// implied by image-sequence formats).
const CODECS: [OutputCodec; 5] = [
    OutputCodec::H264,
    OutputCodec::H265,
    OutputCodec::ProRes422,
    OutputCodec::ProRes4444,
    OutputCodec::DnxHd,
];

/// Render the Output Module dialog body (only called when the panel is open).
pub fn render(app: &App, cx: &mut Context<Pulse>) -> impl IntoElement {
    let header = div()
        .flex()
        .items_center()
        .px_3()
        .py_2()
        .border_b_1()
        .border_color(colors::surface_border())
        .text_color(colors::text_primary())
        .child(div().flex_1().child(section_header("Output Modules")))
        .child(
            div()
                .id("om-add")
                .px_2()
                .py_1()
                .rounded_md()
                .bg(colors::accent())
                .text_size(px(10.0))
                .text_color(colors::text_primary())
                .cursor_pointer()
                .child("+ Add Module")
                .on_click(cx.listener(|root, _ev, _win, cx| {
                    root.app.apply(Action::AddOutputModule(OutputModule::default()));
                    cx.notify();
                })),
        )
        .child(
            div()
                .id("om-close")
                .ml_2()
                .w(px(18.0))
                .h(px(18.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded_md()
                .bg(rgb(BG_ACTIVE))
                .text_color(colors::text_primary())
                .text_size(px(12.0))
                .cursor_pointer()
                .child("×")
                .on_click(cx.listener(|root, _ev, _win, cx| {
                    root.app.output_module_open = false;
                    cx.notify();
                })),
        );

    let body = if app.output_modules.is_empty() {
        div()
            .px_3()
            .py_3()
            .text_color(colors::text_secondary())
            .text_size(px(11.0))
            .child("No output modules. Use + Add Module to create one.")
            .into_any_element()
    } else {
        let cards: Vec<gpui::AnyElement> = app
            .output_modules
            .iter()
            .enumerate()
            .map(|(i, m)| module_card(cx, i, m))
            .collect();
        div().flex().flex_col().children(cards).into_any_element()
    };

    div()
        .flex()
        .flex_col()
        .border_b_1()
        .border_color(colors::surface_border())
        .child(header)
        .child(body)
}

/// One output module: a card with format / codec / depth chips, scale &
/// range steppers, audio toggle, and a remove button.
fn module_card(cx: &mut Context<Pulse>, i: usize, m: &OutputModule) -> gpui::AnyElement {
    // Format chips.
    let fmt_chips: Vec<gpui::AnyElement> = OutputModuleFormat::ALL
        .iter()
        .copied()
        .map(|fmt| {
            let active = m.format == fmt;
            chip(cx, ("om-fmt", i * 16 + fmt as usize), fmt.label(), active,
                Action::SetOutputModuleFormat { idx: i, format: fmt })
        })
        .collect();

    // Codec chips (only meaningful for movie containers).
    let codec_chips: Vec<gpui::AnyElement> = CODECS
        .iter()
        .copied()
        .map(|c| {
            let active = m.codec == c;
            chip(cx, ("om-codec", i * 16 + c as usize), c.label(), active,
                Action::SetOutputModuleCodec { idx: i, codec: c })
        })
        .collect();

    // Depth chips.
    let depth_chips: Vec<gpui::AnyElement> = ColorDepth::ALL
        .iter()
        .copied()
        .map(|d| {
            let active = m.depth == d;
            chip(cx, ("om-depth", i * 16 + d as usize), d.label(), active,
                Action::SetOutputModuleDepth { idx: i, depth: d })
        })
        .collect();

    let scale = m.scale;
    let (start, end) = (m.start_frame, m.end_frame);
    let audio_on = m.audio_enabled;

    div()
        .flex()
        .flex_col()
        .mx_2()
        .my_1()
        .rounded_md()
        .bg(colors::surface_raised())
        .child(
            // Title row: name + extension + remove.
            div()
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .py_1()
                .child(
                    div()
                        .flex_1()
                        .text_color(colors::text_primary())
                        .text_size(px(11.0))
                        .child(format!("{} (.{})", m.name, m.format.extension())),
                )
                .child(
                    div()
                        .id(("om-remove", i))
                        .w(px(18.0))
                        .h(px(18.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_md()
                        .bg(rgb(BG_ACTIVE))
                        .text_color(colors::text_primary())
                        .text_size(px(12.0))
                        .cursor_pointer()
                        .child("×")
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            root.app.apply(Action::RemoveOutputModule(i));
                            cx.notify();
                        })),
                ),
        )
        .child(label_row("Format"))
        .child(div().flex().flex_wrap().gap_1().px_2().pb_1().children(fmt_chips))
        .child(label_row("Codec"))
        .child(div().flex().flex_wrap().gap_1().px_2().pb_1().children(codec_chips))
        .child(label_row("Depth"))
        .child(div().flex().flex_wrap().gap_1().px_2().pb_1().children(depth_chips))
        // Scale stepper.
        .child(stepper_row(
            cx,
            "Scale",
            format!("{:.0}%", scale * 100.0),
            ("om-scale-dec", i),
            ("om-scale-inc", i),
            Action::SetOutputModuleScale { idx: i, scale: (scale - 0.25).max(0.01) },
            Action::SetOutputModuleScale { idx: i, scale: (scale + 0.25).min(8.0) },
        ))
        // Range steppers (start / end frame).
        .child(stepper_row(
            cx,
            "Start frame",
            start.to_string(),
            ("om-start-dec", i),
            ("om-start-inc", i),
            Action::SetOutputModuleRange { idx: i, start: start.saturating_sub(1), end },
            Action::SetOutputModuleRange { idx: i, start: start + 1, end: end.max(start + 1) },
        ))
        .child(stepper_row(
            cx,
            "End frame",
            end.to_string(),
            ("om-end-dec", i),
            ("om-end-inc", i),
            Action::SetOutputModuleRange { idx: i, start, end: end.saturating_sub(1) },
            Action::SetOutputModuleRange { idx: i, start, end: end + 1 },
        ))
        // Audio toggle.
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .py_1()
                .child(
                    div()
                        .w(px(80.0))
                        .text_color(colors::text_secondary())
                        .text_size(px(10.0))
                        .child("Audio"),
                )
                .child(
                    div()
                        .id(("om-audio", i))
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .bg(if audio_on { rgb(BG_ACTIVE) } else { colors::surface_overlay() })
                        .text_color(colors::text_primary())
                        .text_size(px(10.0))
                        .cursor_pointer()
                        .child(if audio_on { "On" } else { "Off" })
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            root.app.apply(Action::SetOutputModuleAudio { idx: i, enabled: !audio_on });
                            cx.notify();
                        })),
                ),
        )
        .into_any_element()
}

/// A small section label.
fn label_row(text: &str) -> impl IntoElement {
    div()
        .px_2()
        .pt_1()
        .text_color(colors::text_secondary())
        .text_size(px(9.0))
        .child(text.to_string())
}

/// A selectable chip emitting `action` on click; highlighted when `active`.
fn chip(
    cx: &mut Context<Pulse>,
    id: (&'static str, usize),
    label: &'static str,
    active: bool,
    action: Action,
) -> gpui::AnyElement {
    div()
        .id(id)
        .px_2()
        .py_1()
        .rounded_md()
        .bg(if active { rgb(BG_ACTIVE) } else { colors::surface_overlay() })
        .text_size(px(9.0))
        .text_color(if active { colors::text_primary() } else { colors::text_secondary() })
        .cursor_pointer()
        .child(label)
        .on_click(cx.listener(move |root, _ev, _win, cx| {
            root.app.apply(action.clone());
            cx.notify();
        }))
        .into_any_element()
}

/// A labeled value with −/+ steppers emitting `dec`/`inc`.
fn stepper_row(
    cx: &mut Context<Pulse>,
    label: &'static str,
    value: String,
    dec_id: (&'static str, usize),
    inc_id: (&'static str, usize),
    dec: Action,
    inc: Action,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .py_1()
        .child(
            div()
                .w(px(80.0))
                .text_color(colors::text_secondary())
                .text_size(px(10.0))
                .child(label),
        )
        .child(step_btn(cx, dec_id, "−", dec))
        .child(
            div()
                .flex_1()
                .text_color(colors::text_primary())
                .text_size(px(10.0))
                .child(value),
        )
        .child(step_btn(cx, inc_id, "+", inc))
}

/// A single stepper button.
fn step_btn(
    cx: &mut Context<Pulse>,
    id: (&'static str, usize),
    glyph: &'static str,
    action: Action,
) -> impl IntoElement {
    div()
        .id(id)
        .w(px(20.0))
        .h(px(18.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .bg(rgb(BG_ACTIVE))
        .text_color(colors::text_primary())
        .text_size(px(12.0))
        .cursor_pointer()
        .child(glyph)
        .on_click(cx.listener(move |root, _ev, _win, cx| {
            root.app.apply(action.clone());
            cx.notify();
        }))
}

#[cfg(test)]
mod tests {
    use crate::app_state::{Action, App, OutputModule};

    #[test]
    fn panel_toggles_default_closed() {
        let app = App::new();
        assert!(!app.output_module_open);
        assert!(!app.expr_editor_open);
        assert!(!app.keylight_open);
        assert!(!app.preferences_open);
        assert_eq!(app.expr_editor_prop, "X");
    }

    #[test]
    fn add_and_configure_module_through_actions() {
        // Exercises exactly the action surface the panel emits.
        let mut app = App::new();
        assert!(app.output_modules.is_empty());
        app.apply(Action::AddOutputModule(OutputModule::default()));
        assert_eq!(app.output_modules.len(), 1);
        app.apply(Action::SetOutputModuleScale { idx: 0, scale: 0.5 });
        assert!((app.output_modules[0].scale - 0.5).abs() < 1e-6);
        app.apply(Action::SetOutputModuleRange { idx: 0, start: 0, end: 48 });
        assert_eq!(app.output_modules[0].end_frame, 48);
        app.apply(Action::RemoveOutputModule(0));
        assert!(app.output_modules.is_empty());
    }
}
