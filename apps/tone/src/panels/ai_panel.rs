//! AI panel — MusicGen, Demucs stem split, chord suggestions, and AI mastering.

use gpui::{div, px, Context, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled};
use gpui::prelude::FluentBuilder;
use prism_ui::{colors, font_size};

use crate::app_state::{Action, App, MusicGenStatus, DemucsStatus};
use crate::Tone;

pub fn render_ai_panel(app: &App, editing_prompt: bool, cx: &mut Context<Tone>) -> impl IntoElement {
    let has_musicgen_running = app.musicgen_jobs.iter().any(|j| j.status == MusicGenStatus::Running);
    let has_demucs_running = app.demucs_jobs.iter().any(|j| j.status == DemucsStatus::Splitting);
    let last_musicgen = app.musicgen_jobs.last();
    let last_demucs = app.demucs_jobs.last();

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
                // Prompt area — click to type
                .child(
                    div()
                        .id("tone-ai-prompt")
                        .w_full()
                        .h(px(56.0))
                        .bg(colors::surface_bg())
                        .rounded(px(3.0))
                        .border_1()
                        .border_color(if editing_prompt { colors::accent() } else { colors::surface_border() })
                        .p_2()
                        .text_size(px(font_size::XS))
                        .text_color(if app.ai_prompt.is_empty() && !editing_prompt {
                            colors::text_disabled()
                        } else {
                            colors::text_primary()
                        })
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _ev, _win, cx| {
                            this.editing_ai_prompt = true;
                            cx.notify();
                        }))
                        .child(if app.ai_prompt.is_empty() && !editing_prompt {
                            "Click to describe music...".to_string()
                        } else if editing_prompt {
                            format!("{}|", app.ai_prompt)
                        } else {
                            app.ai_prompt.clone()
                        }),
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
                                        .on_click(cx.listener(move |this, _ev, _win, cx| {
                                            let mut p = this.app.ai_prompt.clone();
                                            if !p.is_empty() { p.push(' '); }
                                            p.push_str(&tag_s);
                                            this.app.apply(Action::SetAiPrompt(p));
                                            cx.notify();
                                        }))
                                        .child(*label)
                                }),
                        ),
                )
                // Generate button
                .child(
                    div()
                        .id("musicgen-btn")
                        .w_full()
                        .h(px(28.0))
                        .bg(colors::accent())
                        .rounded(px(3.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(font_size::XS))
                        .text_color(gpui::white())
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _ev, _win, cx| {
                            let prompt = this.app.ai_prompt.clone();
                            this.app.apply(Action::QueueMusicGen {
                                prompt: if prompt.is_empty() { "upbeat background music".to_string() } else { prompt },
                                style_tag: String::new(),
                                bars: 8,
                                temperature: 1.0,
                            });
                            this.editing_ai_prompt = false;
                            cx.notify();
                        }))
                        .child(if has_musicgen_running { "Generating..." } else { "Generate Music" }),
                )
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
                .child(
                    div()
                        .id("demucs-btn")
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
                            // Use first audio clip id, or 0 if none
                            let clip_id = this.app.clips.iter()
                                .find(|c| matches!(c.kind, crate::app_state::ClipKind::Audio))
                                .map(|c| c.id)
                                .unwrap_or(0);
                            this.app.apply(Action::QueueStemSplit { clip_id });
                            cx.notify();
                        }))
                        .child(if has_demucs_running { "Separating..." } else { "Separate Stems" }),
                )
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
