//! Export dialog — a floating OS-level child window.
//!
//! Holds a [`WeakEntity<Contour>`] (the welcome / Reel export pattern). It picks
//! an [`ExportFormat`] (PNG / SVG / EPS / PDF) and a destination filename, then
//! emits [`Action::ExportDocument`] and closes.
//!
//! The format selection is window-local UI state (`selected` field); the path is
//! built from a fixed stem plus the chosen format's extension. There is no text
//! input subsystem, so the stem is fixed and the extension tracks the format —
//! the actual write target is `~/Desktop/<stem>.<ext>` resolved at export time.

use std::path::PathBuf;

use gpui::{
    ClickEvent, Context, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Render, StatefulInteractiveElement, Styled, WeakEntity, Window, div, px,
};
use prism_ui::{colors, font_size};

use crate::app_state::Action;
use crate::export_formats::ExportFormat;
use crate::Contour;

pub struct ExportView {
    focus: FocusHandle,
    app_entity: WeakEntity<Contour>,
    /// Window-local: the format currently selected in the picker.
    selected: ExportFormat,
    /// Window-local: the filename stem (without extension).
    stem: String,
}

impl Focusable for ExportView {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl ExportView {
    pub fn new(focus: FocusHandle, app_entity: WeakEntity<Contour>) -> Self {
        Self {
            focus,
            app_entity,
            selected: ExportFormat::Svg,
            stem: "Untitled".to_string(),
        }
    }

    fn dispatch(&self, cx: &mut Context<Self>, action: Action) {
        if let Some(entity) = self.app_entity.upgrade() {
            entity.update(cx, |c, cx| {
                c.app.apply(action);
                cx.notify();
            });
        }
    }

    /// The full output path: `<home or cwd>/<stem>.<ext>`.
    fn output_path(&self) -> String {
        let mut p: PathBuf = dirs_desktop().unwrap_or_else(|| PathBuf::from("."));
        p.push(format!("{}.{}", self.stem, self.selected.extension()));
        p.to_string_lossy().into_owned()
    }
}

/// Best-effort Desktop directory (falls back to home, then cwd).
fn dirs_desktop() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| {
        let mut p = PathBuf::from(h);
        p.push("Desktop");
        if p.is_dir() {
            p
        } else {
            PathBuf::from(std::env::var_os("HOME").unwrap())
        }
    })
}

const FORMATS: [(ExportFormat, &str, &str); 4] = [
    (ExportFormat::Png, "PNG", "Raster image (lossless)"),
    (ExportFormat::Svg, "SVG", "Scalable vector graphics"),
    (ExportFormat::Eps, "EPS", "Encapsulated PostScript"),
    (ExportFormat::Pdf, "PDF", "Portable Document Format"),
];

impl Render for ExportView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self.selected;
        let out_path = self.output_path();

        // --- Left: format list ---
        let format_rows: Vec<gpui::AnyElement> = FORMATS
            .iter()
            .enumerate()
            .map(|(i, (fmt, label, desc))| {
                let fmt = *fmt;
                let active = fmt == selected;
                div()
                    .id(("ex-fmt", i))
                    .flex().flex_col()
                    .px(px(12.0)).py(px(8.0))
                    .rounded(px(4.0))
                    .bg(if active { colors::accent() } else { colors::surface_raised() })
                    .border_1()
                    .border_color(if active { colors::accent() } else { colors::surface_border() })
                    .cursor_pointer()
                    .hover(|s| if active { s } else { s.bg(colors::tool_hover()) })
                    .on_click(cx.listener(move |this, _e: &ClickEvent, _w, cx| {
                        this.selected = fmt;
                        cx.notify();
                    }))
                    .child(
                        div().text_color(colors::text_primary())
                            .text_size(px(font_size::SM))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(*label),
                    )
                    .child(
                        div().text_color(colors::text_secondary())
                            .text_size(px(font_size::XS)).mt(px(2.0))
                            .child(*desc),
                    )
                    .into_any_element()
            })
            .collect();

        let left = div()
            .w(px(220.0)).h_full().flex_shrink_0()
            .flex().flex_col().gap(px(6.0))
            .pr(px(16.0))
            .border_r_1().border_color(colors::surface_border())
            .child(
                div().text_size(px(font_size::SM)).text_color(colors::text_secondary())
                    .mb(px(4.0)).child("FORMAT"),
            )
            .children(format_rows);

        // --- Right: destination + export button ---
        let right = div()
            .flex_1().h_full()
            .flex().flex_col()
            .pl(px(16.0))
            .child(
                div().text_size(px(font_size::SM)).text_color(colors::text_secondary())
                    .mb(px(8.0)).child("DESTINATION"),
            )
            .child(
                div()
                    .px(px(10.0)).py(px(8.0))
                    .rounded(px(4.0)).bg(colors::surface_overlay())
                    .border_1().border_color(colors::surface_border())
                    .text_color(colors::text_primary()).text_size(px(font_size::SM))
                    .child(out_path.clone()),
            )
            .child(
                div().mt(px(6.0)).text_size(px(font_size::XS)).text_color(colors::text_disabled())
                    .child("Exports to your Desktop. The extension follows the selected format."),
            )
            .child(div().flex_1());

        // --- Buttons ---
        let cancel_btn = div()
            .id("ex-cancel")
            .px(px(14.0)).py(px(6.0))
            .rounded(px(4.0)).bg(colors::surface_raised())
            .border_1().border_color(colors::surface_border())
            .text_color(colors::text_secondary()).text_size(px(font_size::SM))
            .cursor_pointer()
            .hover(|s| s.bg(colors::tool_hover()))
            .on_click(cx.listener(move |_this, _e: &ClickEvent, win, _cx| {
                win.remove_window();
            }))
            .child("Cancel");

        let export_btn = div()
            .id("ex-export")
            .px(px(16.0)).py(px(6.0))
            .rounded(px(4.0)).bg(colors::accent())
            .text_color(colors::text_primary()).text_size(px(font_size::SM))
            .cursor_pointer()
            .hover(|s| s.bg(colors::accent_hover()))
            .on_click(cx.listener(move |this, _e: &ClickEvent, win, cx| {
                let path = this.output_path();
                let format = this.selected;
                this.dispatch(cx, Action::ExportDocument { path, format });
                win.remove_window();
            }))
            .child("Export");

        div()
            .size_full()
            .flex().flex_col()
            .bg(colors::surface_bg())
            .text_color(colors::text_primary())
            .font_family(".SystemUIFont")
            .p(px(20.0))
            .child(
                div()
                    .text_size(px(22.0))
                    .font_weight(gpui::FontWeight::BOLD)
                    .mb(px(12.0))
                    .child("Export"),
            )
            .child(
                div()
                    .flex_1()
                    .flex().flex_row()
                    .min_h(px(0.0))
                    .child(left)
                    .child(right),
            )
            .child(
                div()
                    .flex().flex_row().justify_end().gap(px(8.0))
                    .mt(px(12.0))
                    .child(cancel_btn)
                    .child(export_btn),
            )
    }
}
