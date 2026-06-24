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
    AppContext, ClickEvent, Context, Entity, FocusHandle, Focusable, InteractiveElement,
    IntoElement, ParentElement, Render, StatefulInteractiveElement, Styled, WeakEntity, Window,
    div, px,
};
use prism_ui::{colors, font_size, TextField};

use crate::app_state::Action;
use crate::export_formats::ExportFormat;
use crate::Contour;

pub struct ExportView {
    focus: FocusHandle,
    app_entity: WeakEntity<Contour>,
    /// Window-local: the format currently selected in the picker.
    selected: ExportFormat,
    /// Real typing field for the full destination path.
    path_field: Entity<TextField>,
}

impl Focusable for ExportView {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl ExportView {
    pub fn new(
        focus: FocusHandle,
        app_entity: WeakEntity<Contour>,
        cx: &mut Context<Self>,
    ) -> Self {
        let selected = ExportFormat::Svg;
        let initial = default_output_path("Untitled", selected);
        let path_field = cx.new(|cx| {
            TextField::new(cx)
                .placeholder("/path/to/export.svg")
                .initial_value(initial)
                .width(px(360.0))
        });
        Self {
            focus,
            app_entity,
            selected,
            path_field,
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

    /// The current destination path: whatever the user typed into the field.
    fn output_path(&self, cx: &Context<Self>) -> String {
        self.path_field.read(cx).text().trim().to_string()
    }
}

/// The default destination: `<Desktop>/<stem>.<ext>`.
fn default_output_path(stem: &str, fmt: ExportFormat) -> String {
    let mut p: PathBuf = dirs_desktop().unwrap_or_else(|| PathBuf::from("."));
    p.push(format!("{}.{}", stem, fmt.extension()));
    p.to_string_lossy().into_owned()
}

/// Replace the file extension of `path` with `ext`, preserving the directory and
/// stem. An extension-less path simply gains `.ext`.
fn swap_extension(path: &str, ext: &str) -> String {
    let p = PathBuf::from(path);
    p.with_extension(ext).to_string_lossy().into_owned()
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
        let path_field = self.path_field.clone();

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
                    .on_click(cx.listener(move |this, _e: &ClickEvent, win, cx| {
                        this.selected = fmt;
                        // Swap the typed path's extension to match the new format,
                        // preserving the directory + stem the user has entered.
                        let cur = this.path_field.read(cx).text().to_string();
                        let new_path = swap_extension(&cur, fmt.extension());
                        this.path_field.update(cx, |f, cx| f.set_text(new_path, win, cx));
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
            // Editable destination path — type the full output path here.
            .child(path_field)
            .child(
                div().mt(px(6.0)).text_size(px(font_size::XS)).text_color(colors::text_disabled())
                    .child("Type the full output path. Selecting a format updates the extension."),
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
                let path = this.output_path(cx);
                if path.is_empty() {
                    return; // Nothing typed — keep the dialog open.
                }
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
