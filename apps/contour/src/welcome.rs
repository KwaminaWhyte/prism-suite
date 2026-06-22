//! Welcome screen — OS-level floating child window for Contour.

use gpui::{
    div, px, rgb, Context, FocusHandle, Focusable, InteractiveElement,
    IntoElement, ParentElement, Render, StatefulInteractiveElement, Styled,
    WeakEntity, Window,
};
use prism_ui::colors;
use crate::app_state::Action;
use crate::Contour;

const ACCENT: u32 = 0x2e7d5e;        // teal-green for Contour (Illustrator analog)
const SIDEBAR_BG: u32 = 0x141a18;
const PANEL_BG: u32 = 0x1a2320;
const CARD_BG: u32 = 0x243028;

struct Preset { label: &'static str, sub: &'static str }

const PRESETS: &[Preset] = &[
    Preset { label: "Web",       sub: "1920×1080 px" },
    Preset { label: "A4",        sub: "210×297 mm / 300 dpi" },
    Preset { label: "Letter",    sub: "215.9×279.4 mm" },
    Preset { label: "Social",    sub: "1080×1080 px" },
    Preset { label: "Print",     sub: "297×420 mm (A3)" },
    Preset { label: "Custom…",   sub: "Set your own size" },
];

pub struct WelcomeView {
    focus: FocusHandle,
    app_entity: WeakEntity<Contour>,
}

impl WelcomeView {
    pub fn new(focus: FocusHandle, app_entity: WeakEntity<Contour>) -> Self {
        Self { focus, app_entity }
    }
}

impl Focusable for WelcomeView {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for WelcomeView {
    fn render(&mut self, _win: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = self.app_entity.clone();

        macro_rules! btn_click {
            ($action:expr) => {{
                let e = entity.clone();
                let act = $action;
                cx.listener(move |_this, _ev, win, cx| {
                    if let Some(contour) = e.upgrade() {
                        contour.update(cx, |c, cx| { c.app.apply(act.clone()); cx.notify(); });
                    }
                    win.remove_window();
                })
            }};
        }

        div()
            .size_full().flex().flex_row()
            .bg(rgb(SIDEBAR_BG))
            // ── Left sidebar ──
            .child(
                div()
                    .w(px(240.0)).h_full().flex_shrink_0()
                    .bg(rgb(SIDEBAR_BG)).flex().flex_col().p(px(24.0)).gap_4()
                    .child(div().text_size(px(32.0))
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(rgb(ACCENT)).child("Contour"))
                    .child(div().text_size(px(11.0))
                        .text_color(colors::text_secondary()).mt(px(-12.0))
                        .child("Professional vector design"))
                    .child(div().w_full().h(px(1.0)).bg(colors::surface_border()))
                    .child(div().id("contour-new-doc")
                        .w_full().h(px(36.0)).rounded(px(6.0)).bg(rgb(ACCENT))
                        .flex().items_center().justify_center().cursor_pointer()
                        .text_size(px(13.0)).text_color(rgb(0xffffff))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .on_click(btn_click!(Action::NewDocument))
                        .child("New Document"))
                    .child(div().id("contour-open-file")
                        .w_full().h(px(36.0)).rounded(px(6.0)).bg(rgb(CARD_BG))
                        .border_1().border_color(colors::surface_border())
                        .flex().items_center().justify_center().cursor_pointer()
                        .text_size(px(13.0)).text_color(colors::text_primary())
                        .on_click(btn_click!(Action::OpenFile))
                        .child("Open File…"))
                    .child(div().w_full().h(px(1.0)).bg(colors::surface_border()))
                    .child(div().text_size(px(10.0))
                        .text_color(colors::text_secondary())
                        .font_weight(gpui::FontWeight::BOLD).child("RECENT FILES"))
                    .child(div().text_size(px(12.0))
                        .text_color(colors::text_secondary()).child("(No recent files)"))
                    .child(div().flex_1())
                    .child(div().id("contour-skip").cursor_pointer()
                        .text_size(px(11.0)).text_color(colors::text_secondary())
                        .on_click(cx.listener(move |_this, _ev, win, _cx| {
                            win.remove_window();
                        }))
                        .child("Skip")),
            )
            // ── Right panel — presets ──
            .child(
                div()
                    .flex_1().h_full().bg(rgb(PANEL_BG))
                    .flex().flex_col().p(px(28.0)).gap_4()
                    .child(div().text_size(px(14.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(colors::text_primary())
                        .child("NEW DOCUMENT PRESETS"))
                    .child(div().flex().flex_wrap().gap_3()
                        .children(PRESETS.iter().enumerate().map(|(i, preset)| {
                            let e2 = entity.clone();
                            div()
                                .id(("contour-preset", i))
                                .w(px(175.0)).h(px(72.0)).rounded(px(8.0))
                                .bg(rgb(CARD_BG)).border_1()
                                .border_color(colors::surface_border())
                                .flex().flex_col().justify_center().px(px(14.0))
                                .cursor_pointer()
                                .on_click(cx.listener(move |_this, _ev, win, cx| {
                                    if let Some(contour) = e2.upgrade() {
                                        contour.update(cx, |c, cx| {
                                            c.app.apply(Action::NewDocument);
                                            cx.notify();
                                        });
                                    }
                                    win.remove_window();
                                }))
                                .child(div().text_size(px(14.0))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(colors::text_primary())
                                    .child(preset.label))
                                .child(div().text_size(px(11.0))
                                    .text_color(colors::text_secondary()).mt(px(2.0))
                                    .child(preset.sub))
                        })))
                    .child(div().flex_1())
                    .child(div().text_size(px(10.0))
                        .text_color(colors::text_secondary())
                        .child("All settings can be changed later via Document > Document Setup.")),
            )
    }
}
