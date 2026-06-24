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
use prism_ui::{colors, TextArea, TextField};
use welcome::WelcomeView;

/// Format a pan value (`-1.0..=1.0`) into a producer-friendly label that round-
/// trips through [`app_state::parse_pan`]: `C` at centre, `L<n>` / `R<n>` (percent
/// of full deflection) otherwise.
fn pan_label(pan: f32) -> String {
    if pan.abs() < 0.005 {
        "C".to_string()
    } else if pan < 0.0 {
        format!("L{:.0}", -pan * 100.0)
    } else {
        format!("R{:.0}", pan * 100.0)
    }
}

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

    // ── Real editable text inputs (prism_ui::TextField / TextArea) ──────────
    /// MusicGen / AI-clip generation prompt — the user types the prompt that
    /// drives generation. Multi-line ([`TextArea`]) because music prompts read
    /// naturally across several lines. Kept in sync with `app.ai_prompt` via
    /// `on_change`.
    ai_prompt_field: Entity<TextArea>,
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

    // ── Typeable numeric fields (bound to the currently-inspected object) ────
    /// Per-clip gain in dB. Bound to `numeric_clip_id`; re-seeded when the
    /// inspected clip changes (and the field isn't focused).
    clip_gain_field: Entity<TextField>,
    /// Per-clip pitch shift in semitones. Bound to `numeric_clip_id`.
    clip_pitch_field: Entity<TextField>,
    /// Per-clip length in beats. Bound to `numeric_clip_id`.
    clip_length_field: Entity<TextField>,
    /// The clip the three clip-numeric fields above currently reflect.
    numeric_clip_id: Option<usize>,

    /// Active-track volume as a percentage of unity. Bound to `numeric_track_id`.
    track_vol_field: Entity<TextField>,
    /// Active-track stereo pan. Bound to `numeric_track_id`.
    track_pan_field: Entity<TextField>,
    /// The track the two track-numeric fields above currently reflect.
    numeric_track_id: Option<usize>,
}

impl Tone {
    /// Build the root view, constructing every editable [`TextField`] and wiring
    /// its commit callbacks back through `App::apply` via a weak self-handle.
    fn new(app: App, focus: FocusHandle, cx: &mut Context<Self>) -> Self {
        let weak = cx.weak_entity();

        // AI prompt — multi-line, synced live so presets / status text stay
        // coherent. Plain Enter inserts a newline; Cmd/Ctrl+Enter is reserved
        // for submit by the TextArea (no submit handler wired here — generation
        // is driven by the Generate button).
        let w = weak.clone();
        let ai_prompt_field = cx.new(|cx| {
            TextArea::new(cx)
                .placeholder("Describe the music to generate…")
                .initial_value(app.ai_prompt.clone())
                .rows(4)
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

        // Clip gain (dB) — parse + commit on Enter, reflect clamped value back.
        let w = weak.clone();
        let clip_gain_field = cx.new(|cx| {
            TextField::new(cx)
                .placeholder("dB")
                .width(px(56.0))
                .on_submit(move |text, win, cx| {
                    let raw = text.to_string();
                    let _ = w.update(cx, |this, cx| {
                        if let Some(clip_id) = this.numeric_clip_id {
                            if let Some(db) = app_state::parse_db(&raw) {
                                this.app.apply(Action::SetClipGainDb { clip_id, gain_db: db });
                            }
                            this.reseed_clip_numeric_fields(win, cx, true);
                        }
                        cx.notify();
                    });
                })
        });

        // Clip pitch (semitones) — parse + commit on Enter.
        let w = weak.clone();
        let clip_pitch_field = cx.new(|cx| {
            TextField::new(cx)
                .placeholder("st")
                .width(px(56.0))
                .on_submit(move |text, win, cx| {
                    let raw = text.to_string();
                    let _ = w.update(cx, |this, cx| {
                        if let Some(clip_id) = this.numeric_clip_id {
                            if let Some(st) = app_state::parse_semitones(&raw) {
                                this.app.apply(Action::SetClipPitchF32 { clip_id, semitones: st });
                            }
                            this.reseed_clip_numeric_fields(win, cx, true);
                        }
                        cx.notify();
                    });
                })
        });

        // Clip length (beats) — parse + commit on Enter via ResizeClip.
        let w = weak.clone();
        let clip_length_field = cx.new(|cx| {
            TextField::new(cx)
                .placeholder("beats")
                .width(px(56.0))
                .on_submit(move |text, win, cx| {
                    let raw = text.to_string();
                    let _ = w.update(cx, |this, cx| {
                        if let Some(id) = this.numeric_clip_id {
                            if let Some(beats) = app_state::parse_beats(&raw) {
                                this.app.apply(Action::ResizeClip { id, duration_beats: beats });
                            }
                            this.reseed_clip_numeric_fields(win, cx, true);
                        }
                        cx.notify();
                    });
                })
        });

        // Track volume (%) — parse + commit on Enter via SetTrackVolume.
        let w = weak.clone();
        let track_vol_field = cx.new(|cx| {
            TextField::new(cx)
                .placeholder("%")
                .width(px(52.0))
                .on_submit(move |text, win, cx| {
                    let raw = text.to_string();
                    let _ = w.update(cx, |this, cx| {
                        if let Some(id) = this.numeric_track_id {
                            if let Some(vol) = app_state::parse_volume_percent(&raw) {
                                this.app.apply(Action::SetTrackVolume { id, volume: vol });
                            }
                            this.reseed_track_numeric_fields(win, cx, true);
                        }
                        cx.notify();
                    });
                })
        });

        // Track pan — parse + commit on Enter via SetTrackPan.
        let w = weak.clone();
        let track_pan_field = cx.new(|cx| {
            TextField::new(cx)
                .placeholder("L/C/R")
                .width(px(52.0))
                .on_submit(move |text, win, cx| {
                    let raw = text.to_string();
                    let _ = w.update(cx, |this, cx| {
                        if let Some(id) = this.numeric_track_id {
                            if let Some(pan) = app_state::parse_pan(&raw) {
                                this.app.apply(Action::SetTrackPan { id, pan });
                            }
                            this.reseed_track_numeric_fields(win, cx, true);
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
            clip_gain_field,
            clip_pitch_field,
            clip_length_field,
            numeric_clip_id: None,
            track_vol_field,
            track_pan_field,
            numeric_track_id: None,
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
            || self.clip_gain_field.focus_handle(cx).is_focused(window)
            || self.clip_pitch_field.focus_handle(cx).is_focused(window)
            || self.clip_length_field.focus_handle(cx).is_focused(window)
            || self.track_vol_field.focus_handle(cx).is_focused(window)
            || self.track_pan_field.focus_handle(cx).is_focused(window)
    }

    /// Write the inspected clip's current numeric values into the clip-numeric
    /// fields. `force` re-seeds even a focused field (used right after a commit
    /// to reflect the clamped/canonical value); otherwise focused fields are
    /// left alone so they don't clobber in-progress typing.
    fn reseed_clip_numeric_fields(&mut self, window: &mut Window, cx: &mut Context<Self>, force: bool) {
        let Some(clip) = self
            .numeric_clip_id
            .and_then(|id| self.app.clips.iter().find(|c| c.id == id))
        else {
            return;
        };
        let gain_db = if clip.gain > 0.0 {
            20.0 * clip.gain.log10()
        } else {
            -60.0
        };
        let gain_s = format!("{:.1}", gain_db);
        let pitch_s = format!("{:.1}", clip.pitch_shift_f32);
        let len_s = format!("{:.2}", clip.duration_beats);

        for (field, value) in [
            (self.clip_gain_field.clone(), gain_s),
            (self.clip_pitch_field.clone(), pitch_s),
            (self.clip_length_field.clone(), len_s),
        ] {
            let focused = field.focus_handle(cx).is_focused(window);
            if force || !focused {
                field.update(cx, |f, cx| f.set_text(value, window, cx));
            }
        }
    }

    /// Write the inspected track's current volume / pan into the track-numeric
    /// fields. See [`reseed_clip_numeric_fields`](Self::reseed_clip_numeric_fields)
    /// for `force` semantics.
    fn reseed_track_numeric_fields(&mut self, window: &mut Window, cx: &mut Context<Self>, force: bool) {
        let Some(track) = self
            .numeric_track_id
            .and_then(|id| self.app.tracks.iter().find(|t| t.id == id))
        else {
            return;
        };
        let vol_s = format!("{:.0}", track.volume * 100.0);
        let pan_s = pan_label(track.pan);

        for (field, value) in [
            (self.track_vol_field.clone(), vol_s),
            (self.track_pan_field.clone(), pan_s),
        ] {
            let focused = field.focus_handle(cx).is_focused(window);
            if force || !focused {
                field.update(cx, |f, cx| f.set_text(value, window, cx));
            }
        }
    }

    /// Keep the clip / track numeric fields bound to the right object: when the
    /// piano-roll clip or active track changes, rebind and re-seed the fields.
    /// Called once per render before the panels read the field clones.
    fn sync_numeric_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let clip = self.app.piano_roll_clip;
        if clip != self.numeric_clip_id {
            self.numeric_clip_id = clip;
            self.reseed_clip_numeric_fields(window, cx, true);
        } else {
            // Same clip: refresh unfocused fields so stepper / external edits show.
            self.reseed_clip_numeric_fields(window, cx, false);
        }

        let track = self.app.active_track;
        if track != self.numeric_track_id {
            self.numeric_track_id = track;
            self.reseed_track_numeric_fields(window, cx, true);
        } else {
            self.reseed_track_numeric_fields(window, cx, false);
        }
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
        // Rebind / refresh the typeable numeric fields to the inspected clip and
        // active track before the panels read their clones.
        self.sync_numeric_fields(window, cx);
        let has_active_clip = self.app.piano_roll_clip.is_some();
        // Field entity clones handed to the (stateless) panel render fns.
        let ai_prompt_field = self.ai_prompt_field.clone();
        let project_name_field = self.project_name_field.clone();
        let bpm_field = self.bpm_field.clone();
        let track_name_field = self.track_name_field.clone();
        let clip_name_field = self.clip_name_field.clone();
        let clip_gain_field = self.clip_gain_field.clone();
        let clip_pitch_field = self.clip_pitch_field.clone();
        let clip_length_field = self.clip_length_field.clone();
        let track_vol_field = self.track_vol_field.clone();
        let track_pan_field = self.track_pan_field.clone();
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
                                                .child(
                                                    div()
                                                        .w_full()
                                                        .px_2()
                                                        .pt_1()
                                                        .text_size(px(9.0))
                                                        .text_color(colors::text_disabled())
                                                        .child("PIANO ROLL"),
                                                )
                                                // Typeable clip gain / pitch / length.
                                                .child(panels::render_clip_inspector(
                                                    &self.app,
                                                    clip_gain_field,
                                                    clip_pitch_field,
                                                    clip_length_field,
                                                    cx,
                                                )),
                                        )
                                        .child(panels::render_piano_roll(&self.app, cx)),
                                )
                            }),
                    )
                    // Right: AI panel
                    .child(panels::render_ai_panel(&self.app, ai_prompt_field, cx)),
            )
            // Mixer strip — always visible at bottom. The active-track inspector
            // (typeable volume % + pan) sits at the left, the channel strips fill
            // the rest.
            .child(
                div()
                    .w_full()
                    .h(px(120.0))
                    .flex()
                    .flex_row()
                    .bg(colors::surface_raised())
                    .border_t_1()
                    .border_color(colors::surface_border())
                    .child(panels::render_track_inspector(
                        &self.app,
                        track_vol_field,
                        track_pan_field,
                        cx,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .h_full()
                            .overflow_hidden()
                            .child(panels::render_mixer(&self.app, cx)),
                    ),
            )
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
