//! Reel — GPUI host (coexists with the eframe/egui `reel` binary).
//!
//! Architecture, honest from day one: GPUI paints the chrome (toolbar, right
//! inspector/tracks dock, bottom timeline strip) around a video preview whose
//! pixels come from [`canvas_host::CanvasHost`]. Unlike Pigment, Reel uses NO
//! wgpu — its preview is composited CPU-side: [`program_frame`] is a headless
//! sampler that reproduces the program content at the playhead into an RGBA8
//! buffer (mirroring the egui app's `program_frame.rs`), which the host converts
//! to BGRA8 and bridges in as a `RenderImage`, dirty-cached.
//!
//! The integration backbone:
//! - [`app_state::App`] owns everything panels read/mutate (project, host,
//!   playhead time, tool, selection, view) and exposes the single mutation choke
//!   point `App::apply`.
//! - Panels live in [`panels`] as `render(app: &App, cx: &mut Context<Reel>)`
//!   functions and emit [`app_state::Action`]s via `cx.listener` →
//!   `root.app.apply(...)`. See `panels/mod.rs` for the verbatim convention.
//! - This root view (`Reel`) holds the `App` and lays out the chrome around the
//!   live preview.
//!
//! This is the Phase 1+2 starter: a single playhead frame, a real read-only
//! tracks/clips panel + inspector, and a placeholder timeline strip. Real-time
//! playback, audio, and full timeline interaction are later waves.

mod app_state;
mod canvas_host;
mod export;
mod export_codecs;
mod export_window;
mod panels;
mod preferences_window;
mod program_frame;
mod waveform;
mod welcome;

use program_frame::{GlobalGrade, kelvin_to_rgb_gain};

use std::collections::HashMap;
use std::sync::Arc;

use app_state::{Action, App};
use gpui::{
    div, img, px, size, AppContext, Application, Bounds, Context, Entity, FocusHandle,
    InteractiveElement, IntoElement, ParentElement, Pixels, Render, RenderImage,
    StatefulInteractiveElement, Styled, Window, WindowBounds, WindowKind, WindowOptions,
};
use prism_ui::{colors, TextArea, TextField};

use panels::{DOCK_W, TIMELINE_H, TOOLBAR_H};

/// The GPUI root view. Owns the shared [`App`]; panels read it and route their
/// mutations back through `app.apply` inside `cx.listener` callbacks.
struct Reel {
    app: App,
    /// The in-flight background export (render→encode on a worker thread), polled
    /// each animation frame for progress. `None` when no export is running. The
    /// toolbar's Export button spawns the worker and parks it here; the render
    /// loop drains its progress channel and clears it on completion.
    export: Option<export::ExportJob>,
    /// The `RenderImage` painted last frame. gpui's sprite atlas only frees an
    /// image's GPU tile via an explicit `window.drop_image`, so we hold the
    /// previous frame's image and drop it when this frame paints a different one
    /// (a cached/paused frame returns the same `id` and keeps its single tile).
    /// Without this, every newly-bridged preview frame leaks one atlas tile.
    last_image: Option<Arc<RenderImage>>,
    /// Persistent, focusable [`TextField`] views keyed by a stable string
    /// (e.g. `"clip-name-3"`, `"title-text-7"`, `"export-path"`). Panels are
    /// stateless `render(&App, cx)` functions called fresh each frame, so the
    /// field *entities* must live here to survive across frames (keeping their
    /// focus / caret / selection). [`Reel::text_field`] lazily creates each one
    /// with an `on_submit` that dispatches an [`Action`] back through `app.apply`.
    text_fields: HashMap<String, Entity<TextField>>,
    /// Persistent, focusable multi-line [`TextArea`] views keyed by a stable
    /// string (e.g. `"cue-text-2"`). The multi-line companion to `text_fields`,
    /// used where the edited value spans lines (caption cue text). Same lifetime
    /// reasoning: the entities must survive across the stateless per-frame panel
    /// renders to keep their caret / selection / focus. Submitted via
    /// **Cmd/Ctrl+Enter** (plain Enter inserts a newline) through `app.apply`.
    text_areas: HashMap<String, Entity<TextArea>>,
}

impl Reel {
    /// A fresh root view with no export running.
    fn new() -> Self {
        Self {
            app: App::new(),
            export: None,
            last_image: None,
            text_fields: HashMap::new(),
            text_areas: HashMap::new(),
        }
    }

    /// Get-or-create a persistent [`TextField`] keyed by `key`. On first call the
    /// field is built with `placeholder`, seeded `initial`, fixed `width`, and an
    /// `on_submit` (Enter) that maps the typed text to an [`Action`] via
    /// `make_action` and dispatches it through `self.app.apply` (the single
    /// mutation choke point). Subsequent calls return the SAME entity so typing
    /// state (caret, selection, focus) persists across frames.
    ///
    /// `make_action` returning `None` means "ignore this submission" (e.g. the
    /// target row no longer exists). The dispatch runs inside a
    /// `WeakEntity<Reel>::update`, the standard way a child entity mutates the
    /// root view from a plain `&mut gpui::App` callback context.
    fn text_field(
        &mut self,
        key: impl Into<String>,
        placeholder: &str,
        initial: &str,
        width: Pixels,
        make_action: impl Fn(&str) -> Option<Action> + 'static,
        cx: &mut Context<Self>,
    ) -> Entity<TextField> {
        let key = key.into();
        if let Some(field) = self.text_fields.get(&key) {
            return field.clone();
        }
        let weak = cx.weak_entity();
        let placeholder = placeholder.to_string();
        let initial = initial.to_string();
        let field = cx.new(|cx| {
            TextField::new(cx)
                .placeholder(placeholder)
                .initial_value(initial)
                .width(width)
                .on_submit(move |text, _win, app| {
                    if let Some(action) = make_action(text) {
                        let _ = weak.update(app, |reel, cx| {
                            reel.app.apply(action);
                            cx.notify();
                        });
                    }
                })
        });
        self.text_fields.insert(key, field.clone());
        field
    }

    /// Get-or-create a persistent multi-line [`TextArea`] keyed by `key`. The
    /// multi-line analog of [`Reel::text_field`]: the area is built once with
    /// `placeholder`, seeded `initial`, fixed `width`, `rows` visible lines, and
    /// an `on_submit` (**Cmd/Ctrl+Enter** — plain Enter inserts a newline) that
    /// maps the typed text to an [`Action`] via `make_action` and dispatches it
    /// through `self.app.apply`. Subsequent calls return the SAME entity so
    /// typing state persists across frames.
    fn text_area(
        &mut self,
        key: impl Into<String>,
        placeholder: &str,
        initial: &str,
        width: Pixels,
        rows: usize,
        make_action: impl Fn(&str) -> Option<Action> + 'static,
        cx: &mut Context<Self>,
    ) -> Entity<TextArea> {
        let key = key.into();
        if let Some(area) = self.text_areas.get(&key) {
            return area.clone();
        }
        let weak = cx.weak_entity();
        let placeholder = placeholder.to_string();
        let initial = initial.to_string();
        let area = cx.new(|cx| {
            TextArea::new(cx)
                .placeholder(placeholder)
                .initial_value(initial)
                .width(width)
                .rows(rows)
                .on_submit(move |text, _win, app| {
                    if let Some(action) = make_action(text) {
                        let _ = weak.update(app, |reel, cx| {
                            reel.app.apply(action);
                            cx.notify();
                        });
                    }
                })
        });
        self.text_areas.insert(key, area.clone());
        area
    }

    /// Ensure a persistent [`TextField`] exists for every name/text the visible
    /// panels can edit this frame (clip / track names, the selected title clip's
    /// text, marker labels, Essential-Graphics text params, the active sequence
    /// name, and the export output path). Each field is keyed by identity so the
    /// caret follows the row, not a shared slot. Idempotent: existing fields are
    /// returned unchanged so typing state survives across frames. Panels then
    /// look these up by the same key in `self.text_fields`.
    ///
    /// Field width is fixed here (rather than filling) so the fields read as
    /// compact inline inputs inside the dock rows.
    fn prepare_text_fields(&mut self, cx: &mut Context<Self>) {
        const NAME_W: f32 = 150.0;

        // Clip names (one field per clip) — RenameClip by index.
        for i in 0..self.app.project.clips.len() {
            let name = self.app.project.clips[i].name.clone();
            self.text_field(
                format!("clip-name-{i}"),
                "Clip name",
                &name,
                px(NAME_W),
                move |t| Some(Action::RenameClip { index: i, name: t.to_string() }),
                cx,
            );
        }

        // Track names (one field per track) — RenameTrack by index.
        for ti in 0..self.app.project.tracks.len() {
            let name = self.app.project.tracks[ti].name.clone();
            self.text_field(
                format!("track-name-{ti}"),
                "Track name",
                &name,
                px(NAME_W),
                move |t| Some(Action::RenameTrack { index: ti, name: t.to_string() }),
                cx,
            );
        }

        // Selected clip's inline title text — SetTitleText (updates the program
        // preview live since the apply path marks the host dirty). Extract the
        // text first so the `self.app` borrow ends before `&mut self` below.
        if let Some(i) = self.app.selected {
            let title_text = match self.app.project.clips.get(i) {
                Some(clip) => match &clip.source {
                    app_state::ClipSource::Title { text, .. } => Some(text.clone()),
                    _ => None,
                },
                None => None,
            };
            if let Some(text) = title_text {
                self.text_field(
                    format!("title-text-{i}"),
                    "Title text",
                    &text,
                    px(200.0),
                    move |t| Some(Action::SetTitleText { index: i, text: t.to_string() }),
                    cx,
                );
            }
        }

        // Selected clip's typeable numeric inspector fields (parse → clamp →
        // existing Set* action; the +/− steppers stay). Snapshot the values
        // first so the immutable `self.app` borrow ends before field creation.
        if let Some(i) = self.app.selected {
            use crate::panels::numeric_parse as np;
            const NUM_W: f32 = 64.0;
            if let Some(clip) = self.app.project.clips.get(i) {
                let opacity_pct = clip.opacity * 100.0;
                let speed_pct = clip.speed * 100.0;
                let scale_x = clip.motion_scale_x * 100.0;
                let scale_y = clip.motion_scale_y * 100.0;
                let pos_x = clip.motion_x;
                let pos_y = clip.motion_y;
                let is_audio = matches!(&clip.source, app_state::ClipSource::Audio(_));
                let gain = match &clip.source {
                    app_state::ClipSource::Audio(a) => a.effective_gain(),
                    _ => 1.0,
                };

                // Opacity % (0–100 → 0.0–1.0).
                self.text_field(
                    format!("clip-opacity-{i}"),
                    "%",
                    &format!("{opacity_pct:.0}%"),
                    px(NUM_W),
                    move |t| np::parse_percent(t, 0.0, 100.0)
                        .map(|p| Action::SetClipOpacity { index: i, opacity: p / 100.0 }),
                    cx,
                );

                if is_audio {
                    // Audio gain in dB (−inf..+max), parsed to a linear gain.
                    self.text_field(
                        format!("clip-gain-{i}"),
                        "dB",
                        &np::gain_to_db_string(gain),
                        px(NUM_W),
                        move |t| np::parse_db_to_gain(t, app_state::MAX_AUDIO_GAIN)
                            .map(|gain| Action::SetClipGain { index: i, gain }),
                        cx,
                    );
                } else {
                    // Speed % (non-audio only — mirrors the stepper row).
                    self.text_field(
                        format!("clip-speed-{i}"),
                        "%",
                        &format!("{speed_pct:.0}%"),
                        px(NUM_W),
                        move |t| np::parse_percent(t, 1.0, 1000.0)
                            .map(|pct| Action::SetClipSpeed(i, pct)),
                        cx,
                    );
                    // Scale X / Y % and Position X / Y px (motion transform).
                    self.text_field(
                        format!("clip-scale-x-{i}"),
                        "%",
                        &format!("{scale_x:.0}%"),
                        px(NUM_W),
                        move |t| np::parse_percent(t, 1.0, 1000.0).map(|sx| {
                            Action::SetClipMotionScale { clip_idx: i, sx: sx / 100.0, sy: f32::NAN }
                        }),
                        cx,
                    );
                    self.text_field(
                        format!("clip-scale-y-{i}"),
                        "%",
                        &format!("{scale_y:.0}%"),
                        px(NUM_W),
                        move |t| np::parse_percent(t, 1.0, 1000.0).map(|sy| {
                            Action::SetClipMotionScale { clip_idx: i, sx: f32::NAN, sy: sy / 100.0 }
                        }),
                        cx,
                    );
                    self.text_field(
                        format!("clip-pos-x-{i}"),
                        "px",
                        &format!("{pos_x:.0}"),
                        px(NUM_W),
                        move |t| np::parse_clamped(t, -10000.0, 10000.0).map(|x| {
                            Action::SetClipMotion { clip_idx: i, x, y: f32::NAN }
                        }),
                        cx,
                    );
                    self.text_field(
                        format!("clip-pos-y-{i}"),
                        "px",
                        &format!("{pos_y:.0}"),
                        px(NUM_W),
                        move |t| np::parse_clamped(t, -10000.0, 10000.0).map(|y| {
                            Action::SetClipMotion { clip_idx: i, x: f32::NAN, y }
                        }),
                        cx,
                    );
                }
            }
        }

        // Marker labels — RenameMarker by index (only when the panel is open).
        if self.app.show_markers_panel {
            for i in 0..self.app.gpui_markers.len() {
                let name = self.app.gpui_markers[i].name.clone();
                self.text_field(
                    format!("marker-name-{i}"),
                    "Marker name",
                    &name,
                    px(150.0),
                    move |t| Some(Action::RenameMarker { index: i, name: t.to_string() }),
                    cx,
                );
            }
        }

        // Essential-Graphics text params for the active template — SetMogrParamText.
        // Snapshot the (param_idx, initial-text) pairs first so the immutable
        // `self.app` borrow ends before the `&mut self` field creation calls.
        if self.app.mogr_library_open {
            if let Some(ai) = self.app.active_mogr {
                let text_params: Vec<(usize, String)> = self
                    .app
                    .mogr_templates
                    .get(ai)
                    .map(|tmpl| {
                        tmpl.params
                            .iter()
                            .enumerate()
                            .filter_map(|(pi, p)| match &p.value {
                                app_state::MogrParamValue::Text(t) => Some((pi, t.clone())),
                                _ => None,
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                for (pi, initial) in text_params {
                    self.text_field(
                        format!("mogr-text-{ai}-{pi}"),
                        "Text",
                        &initial,
                        px(140.0),
                        move |s| Some(Action::SetMogrParamText {
                            template_idx: ai,
                            param_idx: pi,
                            text: s.to_string(),
                        }),
                        cx,
                    );
                }
            }
        }

        // Active sequence name — RenameSequence (sequence settings overlay).
        if self.app.show_sequence_settings {
            let active_id = self.app.active_sequence_id;
            let seq_name = self
                .app
                .sequences_b5
                .iter()
                .find(|s| s.id == active_id)
                .map(|s| s.name.clone());
            if let Some(name) = seq_name {
                self.text_field(
                    "seq-name",
                    "Sequence name",
                    &name,
                    px(190.0),
                    move |t| Some(Action::RenameSequence {
                        sequence_id: active_id,
                        name: t.to_string(),
                    }),
                    cx,
                );
            }
        }

        // Export output path (Export Presets panel) — SetExportPath.
        if self.app.show_export_presets {
            let initial = self.app.last_export_path
                .as_ref()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            self.text_field(
                "export-path",
                "Output path (e.g. /Users/me/out.mp4)",
                &initial,
                px(240.0),
                |t| {
                    let trimmed = t.trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(Action::SetExportPath(std::path::PathBuf::from(trimmed)))
                    }
                },
                cx,
            );
        }

        // Media-bin clip search box (Bins panel) — SetBinQuery, filtering the
        // active bin's clip list by name. `on_change` would be ideal but the
        // field is submit-driven; Enter applies the filter.
        if self.app.bins_open {
            let initial = self.app.bin_query.clone();
            self.text_field(
                "bin-search",
                "Search clips…",
                &initial,
                px(150.0),
                |t| Some(Action::SetBinQuery(t.trim().to_string())),
                cx,
            );
        }
    }

    /// Ensure a persistent multi-line [`TextArea`] exists for every editable
    /// multi-line value the visible panels render this frame (currently each
    /// caption's cue text in the Captions panel). Mirrors
    /// [`Reel::prepare_text_fields`] but for `TextArea`s. Idempotent: existing
    /// areas are returned unchanged so typing state survives across frames.
    fn prepare_text_areas(&mut self, cx: &mut Context<Self>) {
        if self.app.show_captions_panel {
            // Snapshot (index, text) so the immutable borrow ends before the
            // `&mut self` area-creation calls.
            let cues: Vec<(usize, String)> = self
                .app
                .captions
                .iter()
                .enumerate()
                .map(|(i, c)| (i, c.text.clone()))
                .collect();
            for (i, text) in cues {
                self.text_area(
                    format!("cue-text-{i}"),
                    "Caption text (Cmd/Ctrl+Enter to apply)",
                    &text,
                    px(220.0),
                    2,
                    move |t| Some(Action::SetCueText { index: i, text: t.to_string() }),
                    cx,
                );
            }
        }
    }

    /// Kick off a background MP4 export of the whole program to `path`: snapshot
    /// the project + render the program audio mix into a [`export::JobSpec`],
    /// spawn a worker thread to render→encode→mux (streaming progress over an
    /// mpsc channel), and park the receiver in `self.export` so the render loop
    /// polls it. A no-op (returns `false`) when an export is already running, the
    /// sequence has no frames, or ffmpeg is unavailable. ffmpeg is gated *here*
    /// so a missing binary surfaces as an immediate `Failed` status, not a panic.
    fn start_export(&mut self, path: std::path::PathBuf) -> bool {
        if self.export.is_some() {
            log::warn!("reel-gpui: an export is already running");
            return false;
        }
        log::info!("reel-gpui: starting export format={:?}", self.app.export_format);
        let Some(spec) = export::build_full_export(&self.app.project, &path, &self.app.captions) else {
            log::warn!("reel-gpui: nothing to export (empty sequence)");
            return false;
        };
        let total = spec.total_frames();
        let (tx, rx) = std::sync::mpsc::channel();

        if !prism_media::ffmpeg_available() {
            // Surface the missing-binary case immediately as a Failed status so
            // the toolbar shows it, mirroring the egui app's enqueue-time gate.
            self.export = Some(export::ExportJob {
                rx,
                status: export::JobStatus::Failed {
                    message: "ffmpeg not found".into(),
                },
            });
            return false;
        }

        std::thread::spawn(move || {
            let _ = export::run_job(&spec, &tx);
        });
        self.export = Some(export::ExportJob {
            rx,
            status: export::JobStatus::Rendering { done: 0, total },
        });
        true
    }

    /// Open the floating Preferences window. It receives a `WeakEntity<Reel>`
    /// so its controls dispatch Actions back into this view and read its state.
    /// Like every child window it uses `WindowKind::Floating` (CLAUDE.md).
    fn open_preferences_window(&mut self, cx: &mut Context<Self>) {
        let weak = cx.weak_entity();
        let bounds = Bounds::centered(None, size(px(440.0), px(560.0)), cx);
        let _ = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                kind: WindowKind::Floating,
                ..Default::default()
            },
            |win, cx| {
                let focus: FocusHandle = cx.focus_handle();
                win.focus(&focus);
                cx.new(|_cx| preferences_window::PreferencesView::new(focus, weak))
            },
        );
    }

    /// Open the floating Export Settings (codec matrix) window.
    fn open_export_window(&mut self, cx: &mut Context<Self>) {
        let weak = cx.weak_entity();
        let bounds = Bounds::centered(None, size(px(720.0), px(560.0)), cx);
        let _ = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                kind: WindowKind::Floating,
                ..Default::default()
            },
            |win, cx| {
                let focus: FocusHandle = cx.focus_handle();
                win.focus(&focus);
                cx.new(|_cx| export_window::ExportView::new(focus, weak))
            },
        );
    }
}

fn build_global_grade(app: &App) -> GlobalGrade {
    let wb_gain = if (app.white_balance_temp - 6500.0).abs() < 1.0 && app.white_balance_tint.abs() < 1.0 {
        [1.0, 1.0, 1.0]
    } else {
        let mut g = kelvin_to_rgb_gain(app.white_balance_temp);
        let tint_factor = 1.0 + app.white_balance_tint / 300.0;
        g[1] = (g[1] * tint_factor).clamp(0.0, 4.0);
        g
    };
    GlobalGrade {
        lut: app.lut_table.clone(),
        wb_gain,
        color_wheels: app.color_wheels,
        rgb_curves: app.rgb_curves.clone(),
        hsl_curves: app.hsl_curves.clone(),
    }
}

impl Reel {
    /// The bridged program image at the current playhead (re-samples only when
    /// the host is dirty).
    fn preview_image(&mut self) -> Arc<RenderImage> {
        let t = self.app.time;
        let global = build_global_grade(&self.app);
        // Split the borrow: read the project immutably while mutating the host.
        let Reel { app, .. } = self;
        let log_tracks = &app.track_log_transform;
        let captions = &app.captions;
        app.host.image(&app.project, t, &global, Some(log_tracks.as_slice()), captions.as_slice())
    }
}

impl Render for Reel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Drive the play loop: while playing, advance the playhead by the real
        // elapsed time and ask GPUI to redraw on the next animation frame. The
        // `request_animation_frame` re-notifies this view next frame, so the loop
        // self-sustains and stops once `tick` returns false (playback halted at
        // the sequence end, or paused). This is Reel's CPU-host analog of egui's
        // `ctx.request_repaint()` inside the playback block — the same pattern
        // `pulse-gpui` uses. Decoded frames stay cached so playback reuses them.
        if self.app.tick() {
            window.request_animation_frame();
        }

        // Poll a running export's progress channel each frame. While it is in
        // flight, keep re-arming the next animation frame so the toolbar's
        // progress label advances; clear the slot once it reaches a terminal
        // state (the final Done/Failed label paints one last frame first).
        if let Some(job) = self.export.as_mut() {
            let alive = job.poll();
            if alive {
                window.request_animation_frame();
            } else {
                log::info!("reel-gpui: export finished — {}", job.status.label());
            }
        }

        // Pre-compute multicam data before borrowing app/cx for panel renders.
        let multicam_mode = self.app.multicam_mode;
        let multicam_tracks_snap: Vec<(usize, String)> = if multicam_mode {
            self.app.multicam_tracks.iter().map(|&ti| {
                let name = self.app.project.tracks.get(ti)
                    .map(|t| t.name.clone())
                    .unwrap_or_else(|| format!("T{}", ti + 1));
                (ti, name)
            }).collect()
        } else {
            vec![]
        };

        // Sample the playhead frame first so the preview is fresh.
        let preview = self.preview_image();
        // Free last frame's atlas tile when this frame bridged a new image.
        // gpui's sprite atlas only releases a tile via an explicit
        // `window.drop_image`; the host builds a brand-new `RenderImage` (new
        // `id`) on every re-sample, so without this each playback frame leaks
        // one tile. A cached/paused frame returns the same `id` and is kept.
        if let Some(prev) = self.last_image.take() {
            if prev.id != preview.id {
                let _ = window.drop_image(prev);
            }
        }
        self.last_image = Some(preview.clone());
        let (cw, ch) = (self.app.host.comp_w as f32, self.app.host.comp_h as f32);
        let zoom = self.app.zoom;

        // The running export's status label (if any), shown in the toolbar.
        let export_label = self.export.as_ref().map(|j| j.status.label());

        // Ensure a persistent, focusable TextField exists for every editable
        // name/text the visible panels render this frame (clip/track names, the
        // selected title clip's text, marker labels, Essential-Graphics text
        // params, the active sequence name, export path). Must run BEFORE the
        // `&self.app` borrow below, since it needs `&mut self`.
        self.prepare_text_fields(cx);
        // Same for the multi-line TextAreas (caption cue text).
        self.prepare_text_areas(cx);

        // Build panel elements (read-only &App + cx for Action listeners).
        // `&self.text_fields` co-borrows alongside `&self.app` (both immutable);
        // panels look up their persistent input views by key.
        let app = &self.app;
        let text_fields = &self.text_fields;
        let text_areas = &self.text_areas;
        let toolbar = panels::toolbar::render(app, export_label, cx);
        let inspector = panels::inspector::render(app, text_fields, cx);
        let mixer_panel = if app.show_mixer {
            Some(panels::mixer::render(app, cx))
        } else {
            None
        };
        let scopes_panel = if app.scopes_open {
            Some(panels::scopes::render(app, cx))
        } else {
            None
        };
        let viewer_panel = if app.dual_viewer {
            Some(panels::viewer::render(app, cx))
        } else {
            None
        };
        let bins_panel = if app.bins_open {
            Some(panels::bins::render(app, text_fields, cx))
        } else {
            None
        };
        let sequence_settings_panel = if app.show_sequence_settings {
            Some(panels::sequence_settings::render(app, text_fields, cx))
        } else {
            None
        };
        let captions_panel = if app.show_captions_panel {
            Some(panels::captions::render(app, text_areas, cx))
        } else {
            None
        };
        let markers_panel = if app.show_markers_panel {
            Some(panels::markers::render(app, text_fields, cx))
        } else {
            None
        };
        let export_presets_panel = if app.show_export_presets {
            Some(panels::export_presets::render(app, text_fields, cx))
        } else {
            None
        };
        let graphics_panel = if app.mogr_library_open {
            Some(panels::graphics::render(app, text_fields, cx))
        } else {
            None
        };
        let render_bar = panels::render_bar::render(app, cx);
        let tracks = panels::tracks::render(app, text_fields, cx);
        let timeline = panels::timeline::render(app, cx);

        let dual_viewer = self.app.dual_viewer;
        let selected_clip_name = self.app.selected
            .and_then(|i| self.app.project.clips.get(i))
            .map(|c| c.name.clone());

        // Build the center preview area — either single program frame or multicam grid.
        let preview_center = if multicam_mode && !multicam_tracks_snap.is_empty() {
            let cam_panels: Vec<_> = multicam_tracks_snap.iter().enumerate().map(|(pi, (ti, name))| {
                let ti = *ti;
                let name = name.clone();
                div()
                    .id(("cam-panel", pi))
                    .flex_1()
                    .h_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(colors::surface_raised())
                    .border_1()
                    .border_color(colors::surface_border())
                    .cursor_pointer()
                    .on_click(cx.listener(move |root, _ev, _win, cx| {
                        root.app.apply(app_state::Action::SwitchCamera(ti));
                        cx.notify();
                    }))
                    .child(
                        div()
                            .text_color(colors::text_primary())
                            .text_size(px(11.0))
                            .child(name),
                    )
            }).collect();
            div()
                .flex_1()
                .h_full()
                .bg(colors::surface_bg())
                .flex()
                .flex_col()
                .overflow_hidden()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .w_full()
                        .h(px(64.0))
                        .gap_1()
                        .p_1()
                        .children(cam_panels),
                )
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(img(preview).w(px(cw * 0.4 * zoom)).h(px(ch * 0.4 * zoom))),
                )
                .into_any_element()
        } else if dual_viewer {
            // Dual viewer: Source (left) + Program (right) side by side.
            let src_label = selected_clip_name
                .map(|n| format!("Source: {}", n))
                .unwrap_or_else(|| "Source".to_string());
            div()
                .flex_1()
                .h_full()
                .bg(colors::surface_bg())
                .flex()
                .flex_row()
                .overflow_hidden()
                // Source pane (left)
                .child(
                    div()
                        .flex_1()
                        .h_full()
                        .bg(colors::surface_raised())
                        .flex()
                        .flex_col()
                        .relative()
                        .border_r_1()
                        .border_color(colors::surface_border())
                        .child(
                            div()
                                .absolute()
                                .top(px(4.0))
                                .left(px(4.0))
                                .px_1()
                                .rounded_sm()
                                .bg(gpui::Rgba { r: 0.0, g: 0.0, b: 0.0, a: 0.6 })
                                .text_color(colors::text_primary())
                                .text_size(px(9.0))
                                .child(src_label),
                        )
                        .child(
                            div()
                                .flex_1()
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    div()
                                        .w(px(cw * 0.4 * zoom))
                                        .h(px(ch * 0.4 * zoom))
                                        .bg(gpui::Rgba { r: 0.0, g: 0.0, b: 0.0, a: 1.0 })
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .child(
                                            div()
                                                .text_color(colors::text_disabled())
                                                .text_size(px(10.0))
                                                .child("Source"),
                                        ),
                                ),
                        ),
                )
                // Program pane (right)
                .child(
                    div()
                        .flex_1()
                        .h_full()
                        .bg(colors::surface_bg())
                        .flex()
                        .flex_col()
                        .relative()
                        .child(
                            div()
                                .absolute()
                                .top(px(4.0))
                                .left(px(4.0))
                                .px_1()
                                .rounded_sm()
                                .bg(gpui::Rgba { r: 0.0, g: 0.0, b: 0.0, a: 0.6 })
                                .text_color(colors::text_primary())
                                .text_size(px(9.0))
                                .child("Program"),
                        )
                        .child(
                            div()
                                .flex_1()
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(img(preview).w(px(cw * 0.4 * zoom)).h(px(ch * 0.4 * zoom))),
                        ),
                )
                .into_any_element()
        } else {
            div()
                .flex_1()
                .h_full()
                .bg(colors::surface_bg())
                .flex()
                .items_center()
                .justify_center()
                .overflow_hidden()
                .child(img(preview).w(px(cw * 0.5 * zoom)).h(px(ch * 0.5 * zoom)))
                .into_any_element()
        };

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(colors::surface_bg())
            .text_color(colors::text_primary())
            .font_family(".SystemUIFont")
            // Top toolbar (full width).
            .child(
                div()
                    .w_full()
                    .h(px(TOOLBAR_H))
                    .bg(colors::surface_raised())
                    .border_b_1()
                    .border_color(colors::surface_border())
                    .child(toolbar),
            )
            // Upper workspace: preview | right dock.
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_row()
                    .min_h(px(0.0))
                    // Center preview (program frame or multicam grid).
                    .child(preview_center)
                    // Right dock: inspector over tracks/clips.
                    .child(
                        div()
                            .id("right-dock")
                            .flex_shrink_0()
                            .w(px(DOCK_W))
                            .h_full()
                            .bg(colors::surface_raised())
                            .border_l_1()
                            .border_color(colors::surface_border())
                            .flex()
                            .flex_col()
                            .overflow_y_scroll()
                            .child(inspector)
                            .children(mixer_panel)
                            .children(scopes_panel)
                            .children(viewer_panel)
                            .children(bins_panel)
                            .children(captions_panel)
                            .children(markers_panel)
                            .children(export_presets_panel)
                            .children(graphics_panel)
                            .child(tracks),
                    ),
            )
            // Render-status + proxy bar (full width, above the timeline).
            .child(render_bar)
            // Bottom timeline placeholder strip (full width).
            .child(
                div()
                    .w_full()
                    .h(px(TIMELINE_H))
                    .bg(colors::surface_raised())
                    .border_t_1()
                    .border_color(colors::surface_border())
                    .child(timeline),
            )
            // Sequence settings overlay (floats over everything when open).
            .children(sequence_settings_panel)
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
            let bounds = cx.primary_display()
                .map(|d| d.bounds())
                .unwrap_or_else(|| Bounds::centered(None, size(px(1600.0), px(1000.0)), cx));

            // Open the main editor window first to capture a WeakEntity for the
            // welcome screen buttons.
            let main_handle = cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |_window, cx| cx.new(|_cx| Reel::new()),
            )
            .expect("failed to open window");

            // Open the welcome window second (Floating) so it sits on top.
            // Pass a WeakEntity so its buttons can dispatch Actions to the main
            // Reel view before closing themselves.
            let weak_main = main_handle
                .entity(cx)
                .expect("failed to get main entity")
                .downgrade();
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(
                        Bounds::centered(None, size(px(900.0), px(560.0)), cx),
                    )),
                    kind: WindowKind::Floating,
                    ..Default::default()
                },
                |win, cx| {
                    let focus: FocusHandle = cx.focus_handle();
                    win.focus(&focus);
                    cx.new(|_cx| welcome::WelcomeView::new(focus, weak_main))
                },
            )
            .expect("failed to open welcome window");

            cx.activate(true);
        });
}
