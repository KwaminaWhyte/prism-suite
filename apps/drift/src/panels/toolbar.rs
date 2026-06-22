//! Toolbar panel for Drift — tool buttons + playback controls.

use crate::app_state::{Action, App, DriftTool};
use crate::Drift;
use gpui::{div, svg, px, Context, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled};
use prism_ui::{colors, font_size, Icon};

pub fn render_toolbar(app: &App, cx: &mut Context<Drift>) -> impl IntoElement {
    let playing = app.playing;
    let current = app.current_frame;
    let total = app.document.duration_frames;
    let fps = app.document.fps as u32;
    let doc_name = app.document.name.clone();
    let active_tool = app.active_tool;

    let tool_name = match active_tool {
        DriftTool::Select  => "Select",
        DriftTool::Move    => "Move",
        DriftTool::Pen     => "Pen",
        DriftTool::Rect    => "Rect",
        DriftTool::Ellipse => "Ellipse",
        DriftTool::Lasso   => "Lasso",
    };

    div()
        .w_full()
        .bg(colors::surface_raised())
        .border_b_1()
        .border_color(colors::surface_border())
        .flex()
        .flex_col()
        // ── Row 1: tool buttons + playback ─────────────────────────────────
        .child(
            div()
                .w_full()
                .h(px(44.0))
                .flex()
                .items_center()
                .px_3()
                .gap_2()
                // Tool buttons — icon variants
                .child(tool_btn("tb-select",  Icon::Cursor,  DriftTool::Select,  active_tool, cx))
                .child(tool_btn("tb-move",    Icon::Move,    DriftTool::Move,    active_tool, cx))
                .child(tool_btn("tb-pen",     Icon::Pen,     DriftTool::Pen,     active_tool, cx))
                .child(tool_btn("tb-rect",    Icon::Rect,    DriftTool::Rect,    active_tool, cx))
                .child(tool_btn("tb-ellipse", Icon::Ellipse, DriftTool::Ellipse, active_tool, cx))
                // Divider
                .child(
                    div().w(px(1.0)).h(px(24.0)).bg(colors::surface_border()).mx_2(),
                )
                // Transport — icon buttons
                .child(icon_btn("tb-first",  Icon::Rewind,       || Action::GoToFirstFrame, cx))
                .child(icon_btn("tb-prev",   Icon::ArrowLeft,    || Action::StepBackward,   cx))
                .child(play_pause_btn(playing, cx))
                .child(icon_btn("tb-next",   Icon::ArrowRight,   || Action::StepForward,    cx))
                .child(icon_btn("tb-last",   Icon::FastForward,  || Action::GoToLastFrame,  cx))
                // Frame counter
                .child(
                    div()
                        .mx_3()
                        .text_size(px(font_size::SM))
                        .text_color(colors::text_primary())
                        .child(format!("{current} / {total}")),
                )
                // FPS
                .child(
                    div()
                        .text_size(px(font_size::XS))
                        .text_color(colors::text_secondary())
                        .child(format!("{fps}fps")),
                )
                .child(div().flex_1())
                // Right: undo / redo
                .child(icon_btn("tb-undo", Icon::Undo, || Action::Undo, cx))
                .child(icon_btn("tb-redo", Icon::Redo, || Action::Redo, cx))
                .child(
                    div()
                        .text_size(px(font_size::SM))
                        .text_color(colors::text_secondary())
                        .child(doc_name),
                ),
        )
        // ── Row 2: hint bar ──────────────────────────────────────────────
        .child(
            div()
                .w_full()
                .h(px(22.0))
                .px_3()
                .flex()
                .items_center()
                .gap_4()
                .bg(colors::surface_bg())
                .border_b_1()
                .border_color(colors::surface_border())
                .child(
                    div()
                        .text_size(px(font_size::XS))
                        .text_color(colors::text_primary())
                        .child(format!("Tool: {tool_name}")),
                )
                .child(
                    div()
                        .text_size(px(font_size::XS))
                        .text_color(colors::text_disabled())
                        .child("V=Select  M=Move  P=Pen  R=Rect  E=Ellipse"),
                ),
        )
}

fn tool_btn(
    id: &'static str,
    icon: Icon,
    tool: DriftTool,
    active: DriftTool,
    cx: &mut Context<Drift>,
) -> impl IntoElement {
    let is_active = tool == active;
    div()
        .id(SharedString::from(id))
        .w(px(30.0))
        .h(px(30.0))
        .bg(if is_active { colors::tool_active() } else { colors::surface_overlay() })
        .rounded(px(4.0))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .on_click(cx.listener(move |this, _ev, _win, cx| {
            this.app.apply(Action::SetActiveTool(tool));
            cx.notify();
        }))
        .child(
            svg()
                .path(icon.path())
                .w(px(15.0))
                .h(px(15.0))
                .text_color(if is_active {
                    colors::text_primary()
                } else {
                    colors::text_secondary()
                }),
        )
}

fn icon_btn(
    id: &'static str,
    icon: Icon,
    make_action: fn() -> Action,
    cx: &mut Context<Drift>,
) -> impl IntoElement {
    div()
        .id(SharedString::from(id))
        .w(px(28.0))
        .h(px(28.0))
        .bg(colors::surface_overlay())
        .rounded(px(3.0))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .on_click(cx.listener(move |this, _ev, _win, cx| {
            this.app.apply(make_action());
            cx.notify();
        }))
        .child(
            svg()
                .path(icon.path())
                .w(px(13.0))
                .h(px(13.0))
                .text_color(colors::text_primary()),
        )
}

fn play_pause_btn(playing: bool, cx: &mut Context<Drift>) -> impl IntoElement {
    let icon = if playing { Icon::Pause } else { Icon::Play };
    div()
        .id("tb-play-pause")
        .w(px(36.0))
        .h(px(28.0))
        .bg(colors::accent())
        .rounded(px(3.0))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .on_click(cx.listener(move |this, _ev, _win, cx| {
            if playing { this.app.apply(Action::Pause); } else { this.app.apply(Action::Play); }
            cx.notify();
        }))
        .child(
            svg()
                .path(icon.path())
                .w(px(15.0))
                .h(px(15.0))
                .text_color(gpui::white()),
        )
}
