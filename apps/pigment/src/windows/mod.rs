//! Floating child windows for the GPUI host.
//!
//! Each window here is its own GPUI [`Render`] view that holds a
//! `WeakEntity<Pigment>` (the same pattern the welcome screen uses). Its
//! listeners dispatch [`crate::app_state::Action`]s to the main `Pigment`
//! entity via `entity.update(cx, |root, cx| { root.app.apply(..) })`, then
//! `cx.notify()` so the main editor re-renders.
//!
//! Windows are OPENED from `main.rs` and from toolbar menu items. Every child
//! window MUST be opened with `kind: WindowKind::Floating` so it floats above
//! all other OS windows (see the `open_*` free functions below — they are the
//! single place that constructs the `WindowOptions`).
//!
//! These views never mutate `App` directly and never read `App` outside the
//! `update` closure; they capture only plain values (or a weak handle) so they
//! can live in an independent window render tree.

pub mod new_document;
pub mod preferences;
pub mod script_editor;
pub mod shortcuts;

use gpui::{
    div, px, AppContext, Bounds, IntoElement, ParentElement, Styled, WeakEntity, WindowBounds,
    WindowKind, WindowOptions,
};
use gpui::size as gpui_size;
use prism_ui::{colors, font_size};

use crate::Pigment;

/// Open the floating Preferences window, wired to the given main entity.
pub fn open_preferences(cx: &mut gpui::App, main: WeakEntity<Pigment>) {
    let _ = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                gpui_size(px(620.0), px(560.0)),
                cx,
            ))),
            kind: WindowKind::Floating,
            ..Default::default()
        },
        |win, cx| {
            let focus = cx.focus_handle();
            win.focus(&focus);
            cx.new(|_cx| preferences::PreferencesView::new(focus, main))
        },
    );
}

/// Open the floating Keyboard Shortcuts editor window.
pub fn open_shortcuts(cx: &mut gpui::App, main: WeakEntity<Pigment>) {
    let _ = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                gpui_size(px(680.0), px(600.0)),
                cx,
            ))),
            kind: WindowKind::Floating,
            ..Default::default()
        },
        |win, cx| {
            let focus = cx.focus_handle();
            win.focus(&focus);
            cx.new(|_cx| shortcuts::ShortcutsView::new(focus, main))
        },
    );
}

/// Open the floating Script Editor window (multi-line script source + log),
/// wired to the given main entity.
pub fn open_script_editor(cx: &mut gpui::App, main: WeakEntity<Pigment>) {
    let _ = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                gpui_size(px(680.0), px(640.0)),
                cx,
            ))),
            kind: WindowKind::Floating,
            ..Default::default()
        },
        |win, cx| {
            let focus = cx.focus_handle();
            win.focus(&focus);
            cx.new(|cx| script_editor::ScriptEditorView::new(focus, main, cx))
        },
    );
}

/// Open the floating New / Image Size dialog (typeable width/height/resolution),
/// wired to the given main entity.
pub fn open_new_document(cx: &mut gpui::App, main: WeakEntity<Pigment>) {
    let _ = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                gpui_size(px(440.0), px(340.0)),
                cx,
            ))),
            kind: WindowKind::Floating,
            ..Default::default()
        },
        |win, cx| {
            let focus = cx.focus_handle();
            win.focus(&focus);
            cx.new(|cx| new_document::NewDocumentView::new(focus, main, cx))
        },
    );
}

// ── shared chrome helpers (used by both child windows) ───────────────────────

/// A window header bar with a title and a close button.
pub(crate) fn window_header(
    title: &'static str,
    on_close: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    use gpui::{InteractiveElement, StatefulInteractiveElement};
    div()
        .w_full()
        .h(px(40.0))
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .px_4()
        .bg(colors::surface_raised())
        .border_b_1()
        .border_color(colors::surface_border())
        .child(
            div()
                .text_size(px(font_size::TITLE))
                .text_color(colors::text_primary())
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child(title),
        )
        .child(
            div()
                .id("win-close")
                .w(px(24.0))
                .h(px(24.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded_md()
                .text_color(colors::text_secondary())
                .cursor_pointer()
                .hover(|s| s.bg(colors::surface_overlay()))
                .on_click(on_close)
                .child("✕"),
        )
}

/// A section heading inside a window pane.
pub(crate) fn section_label(text: &str) -> impl IntoElement {
    div()
        .text_size(px(font_size::XS))
        .text_color(colors::text_disabled())
        .mt_2()
        .mb_1()
        .child(text.to_string())
}

/// A single labeled row: caption on the left, control element on the right.
pub(crate) fn labeled_row(caption: &str, control: impl IntoElement + 'static) -> impl IntoElement {
    div()
        .w_full()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .py(px(4.0))
        .child(
            div()
                .text_size(px(font_size::SM))
                .text_color(colors::text_secondary())
                .child(caption.to_string()),
        )
        .child(control)
}
