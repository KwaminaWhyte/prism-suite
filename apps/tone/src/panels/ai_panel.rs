//! AI panel — MusicGen, Demucs stem split, chord suggestions, and AI mastering.

use gpui::{div, px, Context, Entity, Focusable, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled};
use gpui::prelude::FluentBuilder;
use prism_ui::{colors, font_size, TextField};

use crate::app_state::{Action, App, MusicGenStatus, DemucsStatus, ModelDownloadStatus, OnnxModelKind};
use crate::model_manager::{self, ToneModelId};
use crate::Tone;

fn musicgen_model_status(app: &App) -> ModelDownloadStatus {
    app.onnx_models.iter()
        .find(|m| m.kind == OnnxModelKind::MusicGen)
        .map(|m| m.status.clone())
        .unwrap_or(ModelDownloadStatus::NotDownloaded)
}
fn demucs_model_status(app: &App) -> ModelDownloadStatus {
    app.onnx_models.iter()
        .find(|m| m.kind == OnnxModelKind::Demucs)
        .map(|m| m.status.clone())
        .unwrap_or(ModelDownloadStatus::NotDownloaded)
}
fn musicgen_download_progress(app: &App) -> f32 {
    app.onnx_models.iter().find(|m| m.kind == OnnxModelKind::MusicGen).map(|m| m.download_progress).unwrap_or(0.0)
}
fn demucs_download_progress(app: &App) -> f32 {
    app.onnx_models.iter().find(|m| m.kind == OnnxModelKind::Demucs).map(|m| m.download_progress).unwrap_or(0.0)
}

pub fn render_ai_panel(
    app: &App,
    ai_prompt_field: Entity<TextField>,
    cx: &mut Context<Tone>,
) -> impl IntoElement {
    let has_musicgen_running = app.musicgen_jobs.iter().any(|j| j.status == MusicGenStatus::Running);
    let has_demucs_running = app.demucs_jobs.iter().any(|j| j.status == DemucsStatus::Splitting);
    let last_musicgen = app.musicgen_jobs.last();
    let last_demucs = app.demucs_jobs.last();
    let musicgen_status = musicgen_model_status(app);
    let demucs_status = demucs_model_status(app);
    let musicgen_progress = musicgen_download_progress(app);
    let demucs_progress = demucs_download_progress(app);

    div()
        .id("tone-ai-panel")
        .w(px(240.0))
        .h_full()
        .flex_shrink_0()
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
        // ── MusicGen section ──────────────────────────────────────────────
        .child(
            div()
                .px_3()
                .py_2()
                .flex()
                .flex_col()
                .gap_2()
                .border_b_1()
                .border_color(colors::surface_border())
                .child(
                    div()
                        .text_size(px(font_size::XS))
                        .text_color(colors::text_secondary())
                        .child("MUSICGEN"),
                )
                // Prompt area — a real editable TextField. The user types the
                // prompt that drives MusicGen; it is synced into `app.ai_prompt`.
                .child(
                    div()
                        .id("tone-ai-prompt")
                        .w_full()
                        .on_click(cx.listener(move |this, _ev, win, cx| {
                            // Clicking anywhere in the prompt area focuses the field.
                            let fh = this.ai_prompt_field.focus_handle(cx);
                            win.focus(&fh);
                            cx.notify();
                        }))
                        .child(ai_prompt_field),
                )
                // Style tags
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_1()
                        .children(
                            [("Jazz", "jazz"), ("Electronic", "electronic"), ("Ambient", "ambient"), ("Rock", "rock"), ("Lo-Fi", "lo-fi")]
                                .iter()
                                .enumerate()
                                .map(|(idx, (label, tag))| {
                                    let tag_s = tag.to_string();
                                    div()
                                        .id(("tone-style-tag", idx))
                                        .px_2()
                                        .py(px(2.0))
                                        .bg(colors::surface_overlay())
                                        .rounded(px(10.0))
                                        .text_size(px(7.0))
                                        .text_color(colors::text_secondary())
                                        .cursor_pointer()
                                        .on_click(cx.listener(move |this, _ev, win, cx| {
                                            // Append the style tag to the prompt — keep the
                                            // editable field and `app.ai_prompt` in lockstep.
                                            let mut p = this.app.ai_prompt.clone();
                                            if !p.is_empty() { p.push(' '); }
                                            p.push_str(&tag_s);
                                            this.app.apply(Action::SetAiPrompt(p.clone()));
                                            let field = this.ai_prompt_field.clone();
                                            field.update(cx, |f, cx| f.set_text(p, win, cx));
                                            cx.notify();
                                        }))
                                        .child(*label)
                                }),
                        ),
                )
                // Download / Generate button — switches based on model state
                .child(model_action_button_musicgen(cx, &musicgen_status, musicgen_progress, has_musicgen_running))
                // Last job status
                .when(last_musicgen.is_some(), |d| {
                    let job = last_musicgen.unwrap();
                    let status = match job.status {
                        MusicGenStatus::Idle => "Queued",
                        MusicGenStatus::Running => "Running...",
                        MusicGenStatus::Done => "Done",
                        MusicGenStatus::Error => "Failed",
                    };
                    d.child(
                        div()
                            .text_size(px(7.0))
                            .text_color(colors::text_disabled())
                            .child(format!("Last: {} — {}", job.prompt, status)),
                    )
                }),
        )
        // ── Stem Separation (Demucs) ──────────────────────────────────────
        .child(
            div()
                .px_3()
                .py_2()
                .flex()
                .flex_col()
                .gap_2()
                .border_b_1()
                .border_color(colors::surface_border())
                .child(
                    div()
                        .text_size(px(font_size::XS))
                        .text_color(colors::text_secondary())
                        .child("STEM SEPARATION"),
                )
                .child(
                    div()
                        .text_size(px(7.0))
                        .text_color(colors::text_disabled())
                        .child("Select an audio track in the timeline, then separate into vocals, drums, bass, other."),
                )
                .child(model_action_button_demucs(cx, &demucs_status, demucs_progress, has_demucs_running))
                .when(last_demucs.is_some(), |d| {
                    let job = last_demucs.unwrap();
                    let status = match job.status {
                        DemucsStatus::Queued => "Queued",
                        DemucsStatus::Splitting => "Splitting...",
                        DemucsStatus::Done => "Done",
                        DemucsStatus::Error => "Failed",
                    };
                    d.child(
                        div()
                            .text_size(px(7.0))
                            .text_color(colors::text_disabled())
                            .child(format!("Stems: {}", status)),
                    )
                }),
        )
        // ── AI Mastering ──────────────────────────────────────────────────
        .child(
            div()
                .px_3()
                .py_2()
                .flex()
                .flex_col()
                .gap_2()
                .border_b_1()
                .border_color(colors::surface_border())
                .child(
                    div()
                        .text_size(px(font_size::XS))
                        .text_color(colors::text_secondary())
                        .child("AI MASTERING"),
                )
                .child(
                    div()
                        .id("master-btn")
                        .w_full()
                        .h(px(28.0))
                        .bg(colors::surface_overlay())
                        .rounded(px(3.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(font_size::XS))
                        .text_color(colors::text_primary())
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _ev, _win, cx| {
                            this.app.apply(Action::QueueAiMaster {
                                target: crate::app_state::LufsTarget::SpotifyStream,
                            });
                            cx.notify();
                        }))
                        .child("Master to -14 LUFS"),
                ),
        )
        // ── Chord suggestion ──────────────────────────────────────────────
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
                        .child("CHORD TOOLS"),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_1()
                        .children(
                            ["I–IV–V", "I–V–vi–IV", "ii–V–I", "Blues"]
                                .iter()
                                .enumerate()
                                .map(|(idx, prog)| {
                                    div()
                                        .id(("chord-prog", idx))
                                        .px_2()
                                        .py(px(3.0))
                                        .bg(colors::surface_overlay())
                                        .rounded(px(3.0))
                                        .text_size(px(7.0))
                                        .text_color(colors::text_secondary())
                                        .cursor_pointer()
                                        .on_click(cx.listener(|this, _ev, _win, cx| {
                                            this.app.apply(Action::QueueMusicGen {
                                                prompt: "chord progression accompaniment".to_string(),
                                                style_tag: "chords".to_string(),
                                                bars: 4,
                                                temperature: 0.8,
                                            });
                                            cx.notify();
                                        }))
                                        .child(*prog)
                                }),
                        ),
                ),
        )
}

// ── Download-aware action buttons ─────────────────────────────────────────────

fn model_action_button_musicgen(
    cx: &mut gpui::Context<Tone>,
    status: &ModelDownloadStatus,
    progress: f32,
    is_running: bool,
) -> impl IntoElement {
    match status {
        ModelDownloadStatus::NotDownloaded => div()
            .id("musicgen-download-btn")
            .w_full().h(px(28.0))
            .bg(colors::surface_overlay()).rounded(px(3.0))
            .flex().items_center().justify_center()
            .text_size(px(font_size::XS)).text_color(colors::text_primary())
            .cursor_pointer()
            .on_click(cx.listener(|this, _ev, _win, cx| {
                let handle = model_manager::start_download(ToneModelId::MusicGenSmall);
                this.app.apply(Action::StartModelDownload { kind: OnnxModelKind::MusicGen });
                this.model_downloads.push(handle);
                cx.notify();
            }))
            .child(format!("Download MusicGen ({}MB)", ToneModelId::MusicGenSmall.size_mb())),

        ModelDownloadStatus::Downloading => div()
            .id("musicgen-progress")
            .w_full().h(px(28.0))
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

        ModelDownloadStatus::Downloaded => div()
            .id("musicgen-generate-btn")
            .w_full().h(px(28.0))
            .bg(colors::accent()).rounded(px(3.0))
            .flex().items_center().justify_center()
            .text_size(px(font_size::XS)).text_color(gpui::white())
            .cursor_pointer()
            .on_click(cx.listener(|this, _ev, _win, cx| {
                let prompt = this.app.ai_prompt.clone();
                this.app.apply(Action::QueueMusicGen {
                    prompt: if prompt.is_empty() { "upbeat background music".to_string() } else { prompt },
                    style_tag: String::new(),
                    bars: 8,
                    temperature: 1.0,
                });
                cx.notify();
            }))
            .child(if is_running { "Generating..." } else { "Generate Music" }),

        ModelDownloadStatus::Error => div()
            .id("musicgen-error-btn")
            .w_full().h(px(28.0))
            .bg(gpui::rgb(0x7f1d1d)).rounded(px(3.0))
            .flex().items_center().justify_center()
            .text_size(px(font_size::XS)).text_color(gpui::rgb(0xfca5a5))
            .cursor_pointer()
            .on_click(cx.listener(|this, _ev, _win, cx| {
                // Retry download
                let handle = model_manager::start_download(ToneModelId::MusicGenSmall);
                this.app.apply(Action::StartModelDownload { kind: OnnxModelKind::MusicGen });
                this.model_downloads.push(handle);
                cx.notify();
            }))
            .child("Download failed — retry"),
    }
}

fn model_action_button_demucs(
    cx: &mut gpui::Context<Tone>,
    status: &ModelDownloadStatus,
    progress: f32,
    is_running: bool,
) -> impl IntoElement {
    match status {
        ModelDownloadStatus::NotDownloaded => div()
            .id("demucs-download-btn")
            .w_full().h(px(28.0))
            .bg(colors::surface_overlay()).rounded(px(3.0))
            .flex().items_center().justify_center()
            .text_size(px(font_size::XS)).text_color(colors::text_primary())
            .cursor_pointer()
            .on_click(cx.listener(|this, _ev, _win, cx| {
                let handle = model_manager::start_download(ToneModelId::DemucsHybrid);
                this.app.apply(Action::StartModelDownload { kind: OnnxModelKind::Demucs });
                this.model_downloads.push(handle);
                cx.notify();
            }))
            .child(format!("Download Demucs ({}MB)", ToneModelId::DemucsHybrid.size_mb())),

        ModelDownloadStatus::Downloading => div()
            .id("demucs-progress")
            .w_full().h(px(28.0))
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

        ModelDownloadStatus::Downloaded => div()
            .id("demucs-separate-btn")
            .w_full().h(px(28.0))
            .bg(colors::surface_overlay()).rounded(px(3.0))
            .flex().items_center().justify_center()
            .text_size(px(font_size::XS)).text_color(colors::text_primary())
            .cursor_pointer()
            .on_click(cx.listener(|this, _ev, _win, cx| {
                let clip_id = this.app.clips.iter()
                    .find(|c| matches!(c.kind, crate::app_state::ClipKind::Audio))
                    .map(|c| c.id).unwrap_or(0);
                this.app.apply(Action::QueueStemSplit { clip_id });
                cx.notify();
            }))
            .child(if is_running { "Separating..." } else { "Separate Stems" }),

        ModelDownloadStatus::Error => div()
            .id("demucs-error-btn")
            .w_full().h(px(28.0))
            .bg(gpui::rgb(0x7f1d1d)).rounded(px(3.0))
            .flex().items_center().justify_center()
            .text_size(px(font_size::XS)).text_color(gpui::rgb(0xfca5a5))
            .cursor_pointer()
            .on_click(cx.listener(|this, _ev, _win, cx| {
                let handle = model_manager::start_download(ToneModelId::DemucsHybrid);
                this.app.apply(Action::StartModelDownload { kind: OnnxModelKind::Demucs });
                this.model_downloads.push(handle);
                cx.notify();
            }))
            .child("Download failed — retry"),
    }
}
