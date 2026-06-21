//! Welcome panel — shown on launch, dismisses on New / Open / template selection.
//!
//! Rendered as a centered semi-transparent modal overlay inside the GPUI canvas.
//! Returns `None` when `app.show_welcome` is false (root view skips it with
//! `.children(welcome_opt)`).
//!
//! ## Layout (860 × 540 pt)
//! ```
//! ┌─ backdrop (full screen, semi-transparent) ────────────────────────────────┐
//! │  ┌─ card (860 × 540, dark) ───────────────────────────────────────────┐   │
//! │  │  ┌─ left col (260 pt) ─────────┐  ┌─ right col (flex) ──────────┐ │   │
//! │  │  │  Contour  (big purple)       │  │  Start from a template       │ │   │
//! │  │  │  Professional vector editor  │  │  [Logo]  [Web]               │ │   │
//! │  │  │  ─────────────────────────   │  │  [Icon]  [Print]             │ │   │
//! │  │  │  [New Document]              │  │  [Info]  [Custom]            │ │   │
//! │  │  │  [Open File…]                │  └─────────────────────────────┘ │   │
//! │  │  │  ─────────────────────────   │                                   │   │
//! │  │  │  RECENT FILES                │                                   │   │
//! │  │  │  (No recent files)  × 5      │                                   │   │
//! │  │  └─────────────────────────────┘                                   │   │
//! │  └─────────────────────────────────────────────────────────────────────┘   │
//! └───────────────────────────────────────────────────────────────────────────┘
//! ```

use gpui::{
    div, px, rgb, rgba, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};

use crate::app_state::{Action, App};
use crate::Contour;
use prism_ui::colors;

// ── palette ───────────────────────────────────────────────────────────────────
/// App-name accent colour (purple, matches the tool-strip accent).
const PURPLE: u32 = 0x7c5af5;
/// Card background — slightly lighter than the host surface so it pops.
const CARD_BG: u32 = 0x1a1a1c;
/// Separator / muted text.
const MUTED: u32 = 0x555566;
/// Template button fill.
const TMPL_BG: u32 = 0x28282e;
/// Template button border.
const TMPL_BORDER: u32 = 0x3a3a44;

// ── panel entry point ─────────────────────────────────────────────────────────

/// Returns `None` when the welcome panel is hidden.
pub fn render(app: &App, cx: &mut Context<Contour>) -> Option<impl IntoElement> {
    if !app.show_welcome {
        return None;
    }

    // ── left column ──────────────────────────────────────────────────────────
    let new_btn = action_btn("wc-new", "New Document", cx, |root, cx| {
        root.app.apply(Action::DismissWelcome);
        root.app.apply(Action::NewDocument);
        cx.notify();
    });
    let open_btn = action_btn("wc-open", "Open File...", cx, |root, cx| {
        root.app.apply(Action::DismissWelcome);
        root.app.apply(Action::OpenFile);
        cx.notify();
    });

    // Recent files — placeholder rows (no real MRU yet).
    let recent_rows: Vec<gpui::AnyElement> = (0..5)
        .map(|i| {
            div()
                .py(px(3.0))
                .text_color(rgb(MUTED))
                .text_size(px(11.0))
                .child(if i == 0 {
                    "(No recent files)".to_string()
                } else {
                    String::new()
                })
                .into_any_element()
        })
        .collect();

    let left_col = div()
        .w(px(260.0))
        .h_full()
        .flex()
        .flex_col()
        .px(px(28.0))
        .py(px(32.0))
        .border_r_1()
        .border_color(rgb(TMPL_BORDER))
        // App name
        .child(
            div()
                .text_color(rgb(PURPLE))
                .text_size(px(24.0))
                .font_weight(gpui::FontWeight::BOLD)
                .child("Contour"),
        )
        // Subtitle
        .child(
            div()
                .mt(px(4.0))
                .text_color(colors::text_secondary())
                .text_size(px(12.0))
                .child("Professional vector editor"),
        )
        // Separator
        .child(separator())
        // Buttons
        .child(new_btn)
        .child(div().h(px(6.0)))
        .child(open_btn)
        // Separator
        .child(separator())
        // Recent files header
        .child(
            div()
                .text_color(rgb(MUTED))
                .text_size(px(10.0))
                .font_weight(gpui::FontWeight::BOLD)
                .mb(px(4.0))
                .child("RECENT FILES"),
        )
        .children(recent_rows);

    // ── right column — template grid ─────────────────────────────────────────
    const TEMPLATES: &[(&str, &str)] = &[
        ("wc-tmpl-logo",  "Logo Design"),
        ("wc-tmpl-web",   "Web Layout"),
        ("wc-tmpl-icon",  "Icon Set"),
        ("wc-tmpl-print", "Print"),
        ("wc-tmpl-info",  "Infographic"),
        ("wc-tmpl-cust",  "Custom Size"),
    ];

    // Build pairs of template buttons for a 2-column grid.
    let mut grid_rows: Vec<gpui::AnyElement> = Vec::new();
    for chunk in TEMPLATES.chunks(2) {
        let mut row = div().flex().flex_row().gap(px(10.0)).mb(px(10.0));
        for &(id, label) in chunk {
            row = row.child(template_btn(id, label, cx));
        }
        grid_rows.push(row.into_any_element());
    }

    let right_col = div()
        .flex_1()
        .h_full()
        .flex()
        .flex_col()
        .px(px(32.0))
        .py(px(32.0))
        // Section header
        .child(
            div()
                .text_color(colors::text_secondary())
                .text_size(px(12.0))
                .mb(px(16.0))
                .child("Start from a template"),
        )
        .children(grid_rows);

    // ── card ─────────────────────────────────────────────────────────────────
    let card = div()
        .w(px(860.0))
        .h(px(540.0))
        .bg(rgb(CARD_BG))
        .rounded_lg()
        .border_1()
        .border_color(rgb(TMPL_BORDER))
        .overflow_hidden()
        .flex()
        .flex_row()
        .child(left_col)
        .child(right_col);

    // ── backdrop ─────────────────────────────────────────────────────────────
    Some(
        div()
            .id("welcome-backdrop")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000099))
            .on_click(cx.listener(|_root, _ev, _win, _cx| {
                // Clicking the backdrop does NOT dismiss — user must make an
                // explicit choice (New / Open / template).
            }))
            .child(card),
    )
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Full-width action button (New Document / Open File…).
fn action_btn(
    id: &'static str,
    label: &'static str,
    cx: &mut Context<Contour>,
    on_click: impl Fn(&mut Contour, &mut Context<Contour>) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .w_full()
        .h(px(30.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .bg(rgb(PURPLE))
        .text_color(rgb(0xffffff))
        .text_size(px(12.0))
        .cursor_pointer()
        .hover(|s| s.bg(rgb(0x9a7ef7)))
        .on_click(cx.listener(move |root, _ev, _win, cx| on_click(root, cx)))
        .child(label)
}

/// Square template button (used in the 2-column right-column grid).
fn template_btn(
    id: &'static str,
    label: &'static str,
    cx: &mut Context<Contour>,
) -> impl IntoElement {
    div()
        .id(id)
        .flex_1()
        .h(px(80.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .bg(rgb(TMPL_BG))
        .border_1()
        .border_color(rgb(TMPL_BORDER))
        .text_color(colors::text_secondary())
        .text_size(px(11.0))
        .cursor_pointer()
        .hover(|s| s.bg(rgb(0x33333c)).border_color(rgb(PURPLE)))
        .on_click(cx.listener(move |root, _ev, _win, cx| {
            root.app.apply(Action::DismissWelcome);
            cx.notify();
        }))
        .child(label)
}

/// Thin horizontal rule used between the left-column sections.
fn separator() -> impl IntoElement {
    div()
        .my(px(16.0))
        .h(px(1.0))
        .w_full()
        .bg(rgb(TMPL_BORDER))
}
