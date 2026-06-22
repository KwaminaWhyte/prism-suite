//! AI panel — motion generation prompt, style tags, and document property readout.

use crate::app_state::{self, Action, App};
use crate::Drift;
use gpui::{div, px, Context, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled};
use prism_ui::{colors, font_size};

/// Full AI panel with outer shell (width, border, background). Used when the
/// panel is rendered standalone in the flex row.
pub fn render_ai_panel(app: &App, editing_prompt: bool, cx: &mut Context<Drift>) -> impl IntoElement {
    div()
        .id("ai-panel")
        .w(px(260.0))
        .bg(colors::surface_raised())
        .border_l_1()
        .border_color(colors::surface_border())
        .flex()
        .flex_col()
        .overflow_y_scroll()
        // Header
        .child(
            div()
                .w_full()
                .h(px(32.0))
                .px_3()
                .flex()
                .items_center()
                .border_b_1()
                .border_color(colors::surface_border())
                .child(
                    div()
                        .text_size(px(font_size::XS))
                        .text_color(colors::text_secondary())
                        .child("AI TOOLS"),
                ),
        )
        // Motion generate section
        .child(
            div()
                .px_3()
                .py_2()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .text_size(px(font_size::XS))
                        .text_color(colors::text_secondary())
                        .child("MOTION GENERATE"),
                )
                // Prompt textarea — click to type, Escape/Enter to finish
                .child(
                    div()
                        .id("ai-prompt-area")
                        .w_full()
                        .h(px(60.0))
                        .bg(colors::surface_bg())
                        .rounded(px(3.0))
                        .border_1()
                        .border_color(if editing_prompt {
                            colors::accent()
                        } else {
                            colors::surface_border()
                        })
                        .p_2()
                        .text_size(px(font_size::XS))
                        .text_color(if app.ai_motion_prompt.is_empty() && !editing_prompt {
                            colors::text_disabled()
                        } else {
                            colors::text_primary()
                        })
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _ev, _win, cx| {
                            this.editing_prompt = true;
                            cx.notify();
                        }))
                        .child(if app.ai_motion_prompt.is_empty() && !editing_prompt {
                            "Click to type prompt...".to_string()
                        } else if editing_prompt {
                            format!("{}|", app.ai_motion_prompt)
                        } else {
                            app.ai_motion_prompt.clone()
                        }),
                )
                // Style tags — clicking appends the tag to the prompt
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_1()
                        .children(
                            ["Smooth", "Bouncy", "Cinematic", "Fast", "Slow"]
                                .iter()
                                .enumerate()
                                .map(|(idx, tag)| {
                                    let tag_str = tag.to_string();
                                    let tag_lower = tag.to_lowercase();
                                    div()
                                        .id(("style-tag", idx))
                                        .px_2()
                                        .py(px(2.0))
                                        .bg(colors::surface_overlay())
                                        .rounded(px(10.0))
                                        .text_size(px(font_size::XS))
                                        .text_color(colors::text_secondary())
                                        .cursor_pointer()
                                        .on_click(cx.listener(move |this, _ev, _win, cx| {
                                            let mut p = this.app.ai_motion_prompt.clone();
                                            if !p.is_empty() { p.push(' '); }
                                            p.push_str(&tag_lower);
                                            this.app.apply(Action::SetAiMotionPrompt(p));
                                            cx.notify();
                                        }))
                                        .child(tag_str)
                                }),
                        ),
                )
                // Generate button — queues an AnimateDiff job
                .child(
                    div()
                        .id("drift-generate-btn")
                        .w_full()
                        .h(px(30.0))
                        .bg(colors::accent())
                        .rounded(px(3.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(font_size::SM))
                        .text_color(gpui::white())
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _ev, _win, cx| {
                            let prompt = this.app.ai_motion_prompt.clone();
                            let layer_id = this.app.active_layer.unwrap_or(0);
                            this.app.apply(Action::QueueAnimateDiff {
                                layer_id,
                                prompt: if prompt.is_empty() { "motion animation".to_string() } else { prompt },
                                num_frames: this.app.document.duration_frames,
                                guidance_scale: 7.5,
                            });
                            this.editing_prompt = false;
                            cx.notify();
                        }))
                        .child(
                            if app.animatediff_jobs.iter().any(|j| matches!(j.status, app_state::OnnxJobStatus::Queued | app_state::OnnxJobStatus::Running)) {
                                "Generating..."
                            } else {
                                "Generate Motion"
                            }
                        ),
                ),
        )
        // Properties section
        .child(
            div()
                .px_3()
                .py_2()
                .border_t_1()
                .border_color(colors::surface_border())
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_size(px(font_size::XS))
                        .text_color(colors::text_secondary())
                        .child("PROPERTIES"),
                )
                .child(prop_row("Width", &format!("{}px", app.document.width)))
                .child(prop_row("Height", &format!("{}px", app.document.height)))
                .child(prop_row("FPS", &format!("{}", app.document.fps)))
                .child(prop_row(
                    "Duration",
                    &format!("{} frames", app.document.duration_frames),
                )),
        )
}

fn prop_row(label: &str, value: &str) -> impl IntoElement {
    div()
        .w_full()
        .h(px(22.0))
        .flex()
        .items_center()
        .justify_between()
        .child(
            div()
                .text_size(px(font_size::XS))
                .text_color(colors::text_secondary())
                .child(label.to_string()),
        )
        .child(
            div()
                .text_size(px(font_size::XS))
                .text_color(colors::text_primary())
                .child(value.to_string()),
        )
}
