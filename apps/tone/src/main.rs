//! Tone — GPUI host for the AI-first music creation app.
//!
//! Architecture: GPUI drives the chrome (toolbar, piano roll canvas, mixer
//! dock, timeline strip). All application state lives in [`app_state::App`]
//! and is mutated exclusively through [`app_state::Action`]s emitted by
//! panels and routed through `App::apply`. No wgpu custom passes — Tone's
//! piano roll and waveform displays are CPU-rasterised into GPUI surfaces,
//! matching the Reel pattern.

mod app_state;
mod model_manager;
mod panels;
mod welcome;

use app_state::{Action, App, OnnxModelKind};
use gpui::{
    div, px, size, AppContext, Bounds, Context, Entity, FocusHandle, Focusable,
    InteractiveElement, IntoElement, KeyDownEvent, ParentElement, Render, Styled, Window,
    WindowBounds, WindowKind, WindowOptions,
};
use prism_ui::{colors, TextField};
use welcome::WelcomeView;

fn tone_model_to_onnx_kind(m: &model_manager::ToneModelId) -> OnnxModelKind {
    match m {
        model_manager::ToneModelId::MusicGenSmall => OnnxModelKind::MusicGen,
        model_manager::ToneModelId::DemucsHybrid  => OnnxModelKind::Demucs,
        model_manager::ToneModelId::AiMasterNet   => OnnxModelKind::AiMasterNet,
        model_manager::ToneModelId::MelodyRnn     => OnnxModelKind::MelodyRnn,
    }
}

/// The GPUI root view. Owns the shared [`App`]; panels read it and route
/// mutations back through `app.apply` inside `cx.listener` callbacks.
struct Tone {
    app: App,
    focus: FocusHandle,
    last_tick: Option<std::time::Instant>,
    model_downloads: Vec<model_manager::ModelDownloadHandle>,

    // ── Real editable text inputs (prism_ui::TextField) ─────────────────────
    /// MusicGen / AI-clip generation prompt — the user types the prompt that
    /// drives generation. Kept in sync with `app.ai_prompt` via `on_change`.
    ai_prompt_field: Entity<TextField>,
    /// Project name field (toolbar). Commits to `SetProjectName` on submit.
    project_name_field: Entity<TextField>,
    /// Typeable BPM field (toolbar). Parses + commits to `SetBpm` on submit.
    bpm_field: Entity<TextField>,
    /// Inline track-rename field. Active only while `editing_track_id` is set.
    track_name_field: Entity<TextField>,
    editing_track_id: Option<usize>,
    /// Inline clip-rename field. Active only while `editing_clip_id` is set.
    clip_name_field: Entity<TextField>,
    editing_clip_id: Option<usize>,
}

impl Tone {
    /// Build the root view, constructing every editable [`TextField`] and wiring
    /// its commit callbacks back through `App::apply` via a weak self-handle.
    fn new(app: App, focus: FocusHandle, cx: &mut Context<Self>) -> Self {
        let weak = cx.weak_entity();

        // AI prompt — synced live so presets / status text stay coherent.
        let w = weak.clone();
        let ai_prompt_field = cx.new(|cx| {
            TextField::new(cx)
                .placeholder("Describe the music to generate…")
                .initial_value(app.ai_prompt.clone())
                .on_change(move |text, _win, cx| {
                    let text = text.to_string();
                    let _ = w.update(cx, |this, cx| {
                        this.app.apply(Action::SetAiPrompt(text));
                        cx.notify();
                    });
                })
        });

        // Project name — commit on Enter.
        let w = weak.clone();
        let project_name_field = cx.new(|cx| {
            TextField::new(cx)
                .placeholder("Project name")
                .initial_value(app.project.name.clone())
                .width(px(160.0))
                .on_submit(move |text, _win, cx| {
                    let name = text.to_string();
                    let _ = w.update(cx, |this, cx| {
                        if !name.trim().is_empty() {
                            this.app.apply(Action::SetProjectName(name));
                        }
                        cx.notify();
                    });
                })
        });

        // BPM — parse + commit on Enter, then reflect the clamped value back.
        let w = weak.clone();
        let bpm_field = cx.new(|cx| {
            TextField::new(cx)
                .placeholder("BPM")
                .initial_value(format!("{:.0}", app.project.bpm))
                .width(px(52.0))
                .on_submit(move |text, win, cx| {
                    let raw = text.to_string();
                    let _ = w.update(cx, |this, cx| {
                        if let Some(bpm) = app_state::parse_bpm(&raw) {
                            this.app.apply(Action::SetBpm(bpm));
                        }
                        // Reflect the canonical (clamped) value back into the field.
                        let canonical = format!("{:.0}", this.app.project.bpm);
                        let field = this.bpm_field.clone();
                        field.update(cx, |f, cx| f.set_text(canonical, win, cx));
                        cx.notify();
                    });
                })
        });

        // Track rename — commit on Enter, then leave edit mode.
        let w = weak.clone();
        let track_name_field = cx.new(|cx| {
            TextField::new(cx)
                .placeholder("Track name")
                .width(px(120.0))
                .on_submit(move |text, _win, cx| {
                    let name = text.to_string();
                    let _ = w.update(cx, |this, cx| {
                        if let Some(id) = this.editing_track_id.take() {
                            if !name.trim().is_empty() {
                                this.app.apply(Action::RenameTrack { id, name });
                            }
                        }
                        cx.notify();
                    });
                })
        });

        // Clip rename — commit on Enter, then leave edit mode.
        let w = weak.clone();
        let clip_name_field = cx.new(|cx| {
            TextField::new(cx)
                .placeholder("Clip name")
                .width(px(120.0))
                .on_submit(move |text, _win, cx| {
                    let name = text.to_string();
                    let _ = w.update(cx, |this, cx| {
                        if let Some(id) = this.editing_clip_id.take() {
                            if !name.trim().is_empty() {
                                this.app.apply(Action::RenameClip { id, name });
                            }
                        }
                        cx.notify();
                    });
                })
        });

        Self {
            app,
            focus,
            last_tick: None,
            model_downloads: vec![],
            ai_prompt_field,
            project_name_field,
            bpm_field,
            track_name_field,
            editing_track_id: None,
            clip_name_field,
            editing_clip_id: None,
        }
    }

    /// True when any of the editable text fields currently holds focus, so the
    /// global key handler should stand down.
    fn any_text_field_focused(&self, window: &Window, cx: &gpui::App) -> bool {
        self.ai_prompt_field.focus_handle(cx).is_focused(window)
            || self.project_name_field.focus_handle(cx).is_focused(window)
            || self.bpm_field.focus_handle(cx).is_focused(window)
            || self.track_name_field.focus_handle(cx).is_focused(window)
            || self.clip_name_field.focus_handle(cx).is_focused(window)
    }

    /// Begin inline rename of a track: seed the field and focus it.
    fn begin_track_rename(&mut self, id: usize, window: &mut Window, cx: &mut Context<Self>) {
        let name = self
            .app
            .tracks
            .iter()
            .find(|t| t.id == id)
            .map(|t| t.name.clone())
            .unwrap_or_default();
        self.editing_track_id = Some(id);
        self.editing_clip_id = None;
        let field = self.track_name_field.clone();
        field.update(cx, |f, cx| f.set_text(name, window, cx));
        window.focus(&field.focus_handle(cx));
        cx.notify();
    }

    /// Begin inline rename of a clip: seed the field and focus it.
    fn begin_clip_rename(&mut self, id: usize, window: &mut Window, cx: &mut Context<Self>) {
        let name = self
            .app
            .clips
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.name.clone())
            .unwrap_or_default();
        self.editing_clip_id = Some(id);
        self.editing_track_id = None;
        let field = self.clip_name_field.clone();
        field.update(cx, |f, cx| f.set_text(name, window, cx));
        window.focus(&field.focus_handle(cx));
        cx.notify();
    }
}

impl Focusable for Tone {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Tone {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use gpui::prelude::FluentBuilder;
        let _ = window;
        let has_active_clip = self.app.piano_roll_clip.is_some();
        // Field entity clones handed to the (stateless) panel render fns.
        let ai_prompt_field = self.ai_prompt_field.clone();
        let project_name_field = self.project_name_field.clone();
        let bpm_field = self.bpm_field.clone();
        let track_name_field = self.track_name_field.clone();
        let clip_name_field = self.clip_name_field.clone();
        let editing_track_id = self.editing_track_id;
        let editing_clip_id = self.editing_clip_id;
        // ── Model download polling ──────────────────────────────────
        {
            use model_manager::DownloadEvent;
            use crate::app_state::{Action, OnnxModelKind};
            let mut done_indices = vec![];
            for (i, handle) in self.model_downloads.iter().enumerate() {
                while let Ok(ev) = handle.rx.try_recv() {
                    match ev {
                        DownloadEvent::Progress { bytes_done, bytes_total } => {
                            let progress = if bytes_total > 0 {
                                bytes_done as f32 / bytes_total as f32
                            } else {
                                0.0
                            };
                            let kind = tone_model_to_onnx_kind(&handle.model_id);
                            self.app.apply(Action::UpdateModelDownload { kind, progress });
                            cx.notify();
                        }
                        DownloadEvent::Done => {
                            let kind = tone_model_to_onnx_kind(&handle.model_id);
                            let path = handle.model_id.local_path().to_string_lossy().to_string();
                            self.app.apply(Action::CompleteModelDownload { kind, local_path: path });
                            done_indices.push(i);
                            cx.notify();
                        }
                        DownloadEvent::Error(e) => {
                            log::error!("model download error: {e}");
                            done_indices.push(i);
                            cx.notify();
                        }
                    }
                }
            }
            for i in done_indices.into_iter().rev() {
                self.model_downloads.swap_remove(i);
            }
        }

        // ── Transport tick ──────────────────────────────────────────
        if self.app.playing {
            let now = std::time::Instant::now();
            if let Some(last) = self.last_tick {
                let elapsed = now.duration_since(last).as_secs_f32();
                let beats_per_sec = self.app.project.bpm / 60.0;
                let new_beat = self.app.playhead_beat + elapsed * beats_per_sec;
                self.app.apply(crate::app_state::Action::SetPlayheadBeat(new_beat));
            }
            self.last_tick = Some(now);
            cx.notify();
        } else {
            self.last_tick = None;
        }

        div()
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .bg(colors::surface_bg())
            .text_color(colors::text_primary())
            .font_family(".SystemUIFont")
            // Transport toolbar
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, win, cx| {
                let ks = &ev.keystroke;
                let m = &ks.modifiers;
                // Don't steal keys (spacebar transport, etc.) while the user is
                // typing into one of the editable TextFields — those handle and
                // own their own key events.
                if this.any_text_field_focused(win, cx) {
                    return;
                }
                if m.platform && !m.alt && !m.control {
                    match ks.key.as_str() {
                        "z" if m.shift => { this.app.apply(crate::app_state::Action::Redo); cx.notify(); }
                        "z" => { this.app.apply(crate::app_state::Action::Undo); cx.notify(); }
                        _ => {}
                    }
                }
                if !m.platform && !m.control && !m.alt && !m.shift {
                    match ks.key.as_str() {
                        " " => {
                            if this.app.playing { this.app.apply(crate::app_state::Action::Pause); }
                            else { this.app.apply(crate::app_state::Action::Play); }
                            cx.notify();
                        }
                        _ => {}
                    }
                }
            }))
            .child(panels::render_toolbar(&self.app, project_name_field, bpm_field, cx))
            // Main workspace row: left (arrangement + piano roll) | right (AI panel)
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .flex()
                    .flex_row()
                    // Left: arrangement fills space, piano roll splits below when clip active
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .flex_col()
                            // Arrangement — primary flex-1
                            .child(
                                div()
                                    .flex_1()
                                    .min_h(px(0.0))
                                    .child(panels::render_timeline(
                                        &self.app,
                                        track_name_field,
                                        editing_track_id,
                                        clip_name_field,
                                        editing_clip_id,
                                        cx,
                                    )),
                            )
                            // Piano roll — conditional 260px bottom split
                            .when(has_active_clip, |d| {
                                d.child(
                                    div()
                                        .w_full()
                                        .h(px(260.0))
                                        .flex()
                                        .flex_row()
                                        .border_t_1()
                                        .border_color(colors::surface_border())
                                        .child(
                                            div()
                                                .w(px(panels::TRACKS_W))
                                                .h_full()
                                                .flex_shrink_0()
                                                .bg(colors::surface_raised())
                                                .border_r_1()
                                                .border_color(colors::surface_border())
                                                .flex()
                                                .flex_col()
                                                .justify_center()
                                                .items_center()
                                                .child(
                                                    div()
                                                        .text_size(px(9.0))
                                                        .text_color(colors::text_disabled())
                                                        .child("PIANO ROLL"),
                                                ),
                                        )
                                        .child(panels::render_piano_roll(&self.app, cx)),
                                )
                            }),
                    )
                    // Right: AI panel
                    .child(panels::render_ai_panel(&self.app, ai_prompt_field, cx)),
            )
            // Mixer strip — always visible at bottom
            .child(panels::render_mixer(&self.app, cx))
    }
}

fn main() {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    gpui::Application::new()
        .with_assets(prism_ui::PrismAssets)
        .run(|cx: &mut gpui::App| {
            prism_ui::init(cx);

            // Open the main editor window first to capture a WeakEntity for
            // the welcome screen buttons.
            let bounds = cx
                .primary_display()
                .map(|d| d.bounds())
                .unwrap_or_else(|| Bounds::centered(None, size(px(1600.0), px(1000.0)), cx));
            let main_handle = cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |window, cx| {
                    cx.new(|cx| {
                        let focus = cx.focus_handle();
                        window.focus(&focus);
                        {
                            let mut app = App::new();
                            for (model_id, kind) in [
                                (model_manager::ToneModelId::MusicGenSmall, OnnxModelKind::MusicGen),
                                (model_manager::ToneModelId::DemucsHybrid,  OnnxModelKind::Demucs),
                                (model_manager::ToneModelId::AiMasterNet,   OnnxModelKind::AiMasterNet),
                                (model_manager::ToneModelId::MelodyRnn,     OnnxModelKind::MelodyRnn),
                            ] {
                                if model_id.is_downloaded() {
                                    app.apply(Action::CompleteModelDownload {
                                        kind,
                                        local_path: model_id.local_path().to_string_lossy().to_string(),
                                    });
                                }
                            }
                            Tone::new(app, focus, cx)
                        }
                    })
                },
            )
            .expect("failed to open main window");

            // Welcome window — opened second so it appears on top of the editor.
            // Pass a WeakEntity so buttons can dispatch Actions before closing.
            let weak_main = main_handle
                .entity(cx)
                .expect("failed to get main entity")
                .downgrade();
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(900.0), px(560.0)),
                        cx,
                    ))),
                    kind: WindowKind::Floating,
                    ..Default::default()
                },
                |window, cx| {
                    let focus = cx.focus_handle();
                    window.focus(&focus);
                    cx.new(|_cx| WelcomeView::new(focus, weak_main))
                },
            )
            .expect("failed to open welcome window");

            cx.activate(true);
        });
}
