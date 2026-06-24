//! PSD export panel — a real, typeable output-path field plus the existing
//! encoding / compatibility chips and an Export button.
//!
//! Wave: replaced the path-only model state (no UI surfaced `SetPsdExportPath`)
//! with a focusable `prism_ui::TextField`. The field's `on_submit` feeds the
//! existing `Action::SetPsdExportPath`; the Export button fires
//! `Action::ExportAsPsd { path }` with the field's current text so a typed-then-
//! Export round-trip works without first pressing Enter. The "Browse…" chip is
//! kept as an augmenting affordance (opens a native save dialog).

use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, Context, Entity, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, Window,
};
use prism_ui::{colors, divider, font_size, section_header, TextField};

use crate::app_state::{Action, App, PsdEncoding};
use crate::Pigment;

pub fn render(
    app: &App,
    path_field: &Entity<TextField>,
    cx: &mut Context<Pigment>,
) -> impl IntoElement {
    let cfg = &app.psd_export_config;
    let last = app.last_psd_export_path.clone();
    let maximize = cfg.maximize_compatibility;
    let encoding = cfg.encoding;

    // A small labeled chip button. The click callback gets the window so handlers
    // that drive a `TextField` (which needs `&mut Window`) work.
    let chip = |id: &'static str, label: String, accent: bool, cx: &mut Context<Pigment>,
                on_click: fn(&mut Pigment, &mut Window, &mut Context<Pigment>)| {
        let (bg, txt) = if accent {
            (colors::accent(), colors::text_primary())
        } else {
            (colors::surface_overlay(), colors::text_primary())
        };
        div()
            .id(id)
            .px_2()
            .py_1()
            .rounded_md()
            .bg(bg)
            .border_1()
            .border_color(colors::surface_border())
            .text_color(txt)
            .text_size(px(font_size::XS))
            .cursor_pointer()
            .hover(|s| s.bg(colors::tool_hover()))
            .on_click(cx.listener(move |root, _ev, win, cx| {
                on_click(root, win, cx);
                cx.notify();
            }))
            .child(label)
    };

    div()
        .w_full()
        .flex()
        .flex_col()
        .gap_2()
        .p_2()
        .bg(colors::surface_raised())
        .border_b_1()
        .border_color(colors::surface_border())
        .text_color(colors::text_primary())
        .text_size(px(font_size::SM))
        .child(section_header("Export PSD"))
        .child(divider())
        // Output path — real typing.
        .child(
            div()
                .text_color(colors::text_secondary())
                .text_size(px(font_size::XS))
                .child("Output path (.psd)"),
        )
        .child(div().w_full().child(path_field.clone()))
        // Row: Browse… (native dialog) — sets both the field and config path.
        .child(
            div()
                .w_full()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(chip("psd-browse", "Browse…".to_string(), false, cx, |root, win, cx| {
                    if let Some(file) = rfd::FileDialog::new()
                        .set_title("Export PSD As")
                        .add_filter("Photoshop", &["psd"])
                        .save_file()
                    {
                        let p = file.to_string_lossy().to_string();
                        root.app.apply(Action::SetPsdExportPath(p.clone()));
                        root.set_psd_path_field(p, win, cx);
                    }
                }))
                // Export — uses the field's CURRENT text (typed but maybe not submitted).
                .child(chip("psd-export", "Export".to_string(), true, cx, |root, _win, cx| {
                    let p = root.psd_path_field.read(cx).text().to_string();
                    if !p.trim().is_empty() {
                        root.app.apply(Action::ExportAsPsd { path: p });
                    }
                })),
        )
        // Encoding toggle (RLE / Raw) — existing config action.
        .child(
            div()
                .w_full()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .flex_shrink_0()
                        .text_color(colors::text_secondary())
                        .text_size(px(font_size::XS))
                        .child("Encoding"),
                )
                .child(chip(
                    "psd-enc-rle",
                    "RLE".to_string(),
                    matches!(encoding, PsdEncoding::Rle),
                    cx,
                    |root, _win, _cx| root.app.apply(Action::SetPsdEncoding(PsdEncoding::Rle)),
                ))
                .child(chip(
                    "psd-enc-raw",
                    "Raw".to_string(),
                    matches!(encoding, PsdEncoding::Raw),
                    cx,
                    |root, _win, _cx| root.app.apply(Action::SetPsdEncoding(PsdEncoding::Raw)),
                )),
        )
        // Maximize compatibility toggle.
        .child(chip(
            "psd-maxcompat",
            format!("Maximize compatibility: {}", if maximize { "On" } else { "Off" }),
            maximize,
            cx,
            move |root, _win, _cx| {
                let v = !root.app.psd_export_config.maximize_compatibility;
                root.app.apply(Action::SetPsdMaximizeCompatibility(v));
            },
        ))
        // Last-export confirmation line.
        .when_some(last, |d, p| {
            d.child(
                div()
                    .text_color(colors::text_disabled())
                    .text_size(px(font_size::XS))
                    .child(format!("Last exported: {p}")),
            )
        })
}
