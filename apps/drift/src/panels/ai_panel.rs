//! AI panel — motion generation prompt, style tags, and document property readout.

use crate::app_state::{self, Action, App, DriftOnnxModel, OnnxModelStatus};
use crate::model_manager::{self, DriftModelId};
use crate::panels::inspector::editable_row;
use crate::text_fields::{NumericField, TextFields};
use crate::Drift;
use gpui::{div, px, Context, Entity, Focusable, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled};
use prism_ui::{colors, font_size, TextArea};

fn animatediff_status(app: &App) -> OnnxModelStatus {
    app.drift_onnx_models.iter()
        .find(|m| m.model == DriftOnnxModel::AnimateDiff)
        .map(|m| m.status.clone())
        .unwrap_or(OnnxModelStatus::NotDownloaded)
}
fn animatediff_progress(app: &App) -> f32 {
    app.drift_onnx_models.iter()
        .find(|m| m.model == DriftOnnxModel::AnimateDiff)
        .map(|m| m.download_progress)
        .unwrap_or(0.0)
}

/// Full AI panel with outer shell (width, border, background). Used when the
/// panel is rendered standalone in the flex row. `fields` carries the root
/// view's real editable inputs: the motion prompt, AI-script prompt, and script
/// source editor (all multi-line [`prism_ui::TextArea`]s), plus the document
/// numeric [`prism_ui::TextField`]s (fps / width / height / duration).
pub fn render_ai_panel(
    app: &App,
    editing: Option<NumericField>,
    fields: &TextFields,
    cx: &mut Context<Drift>,
) -> impl IntoElement {
    let animdiff_status = animatediff_status(app);
    let animdiff_progress = animatediff_progress(app);
    let is_generating = app.animatediff_jobs.iter().any(|j| matches!(j.status, app_state::OnnxJobStatus::Queued | app_state::OnnxJobStatus::Running));
    let motion_field = fields.motion_prompt.clone();
    let script_prompt_field = fields.script_prompt.clone();
    let script_source_field = fields.script_source.clone();
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
                // Prompt — REAL editable TextField (type the AnimateDiff prompt)
                .child(
                    div()
                        .id("ai-prompt-area")
                        .w_full()
                        .on_click(cx.listener({
                            let f = motion_field.clone();
                            move |_this, _ev, win, cx| {
                                win.focus(&f.focus_handle(cx));
                                cx.notify();
                            }
                        }))
                        .child(motion_field.clone()),
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
                                    let f = motion_field.clone();
                                    div()
                                        .id(("style-tag", idx))
                                        .px_2()
                                        .py(px(2.0))
                                        .bg(colors::surface_overlay())
                                        .rounded(px(10.0))
                                        .text_size(px(font_size::XS))
                                        .text_color(colors::text_secondary())
                                        .cursor_pointer()
                                        .on_click(cx.listener(move |_this, _ev, win, cx| {
                                            // Append into the real TextField; its
                                            // on_change keeps app.ai_motion_prompt in sync.
                                            let tl = tag_lower.clone();
                                            f.update(cx, |field, cx| {
                                                let mut p = field.text().to_string();
                                                if !p.is_empty() { p.push(' '); }
                                                p.push_str(&tl);
                                                field.set_text(p, win, cx);
                                            });
                                        }))
                                        .child(tag_str)
                                }),
                        ),
                )
                // Generate / Download button — adapts to AnimateDiff model state
                .child(animdiff_action_button(cx, &animdiff_status, animdiff_progress, is_generating)),
        )
        // AI Script generation section
        .child(
            div()
                .px_3()
                .py_2()
                .border_t_1()
                .border_color(colors::surface_border())
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .text_size(px(font_size::XS))
                        .text_color(colors::text_secondary())
                        .child("AI SCRIPT"),
                )
                // Natural-language prompt — REAL editable TextField
                .child(
                    div()
                        .id("ai-script-prompt-area")
                        .w_full()
                        .on_click(cx.listener({
                            let f = script_prompt_field.clone();
                            move |_this, _ev, win, cx| {
                                win.focus(&f.focus_handle(cx));
                                cx.notify();
                            }
                        }))
                        .child(script_prompt_field.clone()),
                )
                // Generate Script button — queues + completes a stub job whose
                // generated code lands in the editable source field below.
                .child(generate_script_button(cx, script_source_field.clone()))
                // Editable script source — REAL TextField wired to SetScriptSource
                .child(
                    div()
                        .text_size(px(font_size::XS))
                        .text_color(colors::text_secondary())
                        .child("SOURCE"),
                )
                .child(
                    div()
                        .id("ai-script-source-area")
                        .w_full()
                        .on_click(cx.listener({
                            let f = script_source_field.clone();
                            move |_this, _ev, win, cx| {
                                win.focus(&f.focus_handle(cx));
                                cx.notify();
                            }
                        }))
                        .child(script_source_field.clone()),
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
                // Editable document numeric fields — click a value to type a new
                // one; Enter commits via SetDocument*.
                .child(editable_row("Width", format!("{}px", app.document.width), NumericField::DocWidth, editing, fields, cx))
                .child(editable_row("Height", format!("{}px", app.document.height), NumericField::DocHeight, editing, fields, cx))
                .child(editable_row("FPS", format!("{:.0}", app.document.fps), NumericField::DocFps, editing, fields, cx))
                .child(editable_row("Duration", format!("{} frames", app.document.duration_frames), NumericField::DocDuration, editing, fields, cx)),
        )
}

fn animdiff_action_button(
    cx: &mut gpui::Context<Drift>,
    status: &OnnxModelStatus,
    progress: f32,
    is_generating: bool,
) -> impl IntoElement {
    match status {
        OnnxModelStatus::NotDownloaded => div()
            .id("animdiff-download-btn")
            .w_full().h(px(30.0))
            .bg(colors::surface_overlay()).rounded(px(3.0))
            .flex().items_center().justify_center()
            .text_size(px(font_size::XS)).text_color(colors::text_primary())
            .cursor_pointer()
            .on_click(cx.listener(|this, _ev, _win, cx| {
                let handle = model_manager::start_download(DriftModelId::AnimateDiff);
                this.app.apply(Action::StartDriftModelDownload { model: DriftOnnxModel::AnimateDiff });
                this.model_downloads.push(handle);
                cx.notify();
            }))
            .child(format!("Download AnimateDiff ({}MB)", DriftModelId::AnimateDiff.size_mb())),

        OnnxModelStatus::Downloading => div()
            .id("animdiff-progress")
            .w_full().h(px(30.0))
            .bg(colors::surface_bg()).rounded(px(3.0))
            .border_1().border_color(colors::accent())
            .flex().items_center().px_2().gap_2()
            .child(
                div().flex_1().h(px(6.0)).bg(colors::surface_overlay()).rounded(px(3.0))
                    .child(
                        div().h_full().w(gpui::relative(progress.clamp(0.0, 1.0)))
                            .bg(colors::accent()).rounded(px(3.0))
                    )
            )
            .child(
                div().text_size(px(7.0)).text_color(colors::text_secondary())
                    .child(format!("{:.0}%", progress * 100.0))
            ),

        OnnxModelStatus::Ready => div()
            .id("animdiff-generate-btn")
            .w_full().h(px(30.0))
            .bg(colors::accent()).rounded(px(3.0))
            .flex().items_center().justify_center()
            .text_size(px(font_size::SM)).text_color(gpui::white())
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
                cx.notify();
            }))
            .child(if is_generating { "Generating..." } else { "Generate Motion" }),

        OnnxModelStatus::Error => div()
            .id("animdiff-error-btn")
            .w_full().h(px(30.0))
            .bg(gpui::rgb(0x7f1d1d)).rounded(px(3.0))
            .flex().items_center().justify_center()
            .text_size(px(font_size::XS)).text_color(gpui::rgb(0xfca5a5))
            .cursor_pointer()
            .on_click(cx.listener(|this, _ev, _win, cx| {
                let handle = model_manager::start_download(DriftModelId::AnimateDiff);
                this.app.apply(Action::StartDriftModelDownload { model: DriftOnnxModel::AnimateDiff });
                this.model_downloads.push(handle);
                cx.notify();
            }))
            .child("Download failed — retry"),
    }
}

/// "Generate Script" button. Reads the typed AI-script prompt, queues + completes
/// a stub AI-script job, ensures a `Script` exists, writes the derived stub code
/// into it (via `SetScriptSource`), and pushes that code into the editable
/// `source_field` so the user immediately sees (and can edit) the result.
fn generate_script_button(
    cx: &mut gpui::Context<Drift>,
    source_field: Entity<TextArea>,
) -> impl IntoElement {
    div()
        .id("ai-script-generate-btn")
        .w_full().h(px(28.0))
        .bg(colors::accent()).rounded(px(3.0))
        .flex().items_center().justify_center()
        .text_size(px(font_size::XS)).text_color(gpui::white())
        .cursor_pointer()
        .on_click(cx.listener(move |this, _ev, win, cx| {
            let prompt = this.app.ai_script_prompt.clone();
            // Derive a tiny deterministic stub script from the (real, typed) prompt.
            let code = if prompt.trim().is_empty() {
                "// describe a script above, then Generate".to_string()
            } else {
                format!("// {prompt}\nlayer.opacity = 1.0;")
            };
            // Record the job, then complete it with the generated code.
            this.app.apply(Action::QueueAiScript { prompt: prompt.clone() });
            if let Some(job_id) = this.app.ai_script_jobs.last().map(|j| j.id) {
                this.app.apply(Action::CompleteAiScript { job_id, code: code.clone() });
            }
            // Ensure a Script object exists and set its source.
            if this.app.scripts.is_empty() {
                this.app.apply(Action::AddScript {
                    name: "Generated Script".to_string(),
                    language: app_state::ScriptLanguage::JavaScript,
                });
            }
            if let Some(sid) = this.app.scripts.first().map(|s| s.id) {
                this.app.apply(Action::SetScriptSource { script_id: sid, source: code.clone() });
            }
            // Reflect the generated code in the editable source TextField.
            source_field.update(cx, |field, cx| {
                field.set_text(code.clone(), win, cx);
            });
            cx.notify();
        }))
        .child("Generate Script")
}
