//! Timeline **transport / playback / import / scopes** helpers split out of
//! `timeline.rs` (file-size rule): the wall-clock `tick`, rodio audio playback +
//! scrub burst, media import → clip building, the program scope-data sampler, and
//! the transport dispatcher arms (seek / step / play / pause / import).
//!
//! `impl App` blocks are additive across the crate, so these methods are callable
//! from `timeline.rs` and the other domain files.

use std::time::Instant;
use rodio::{DeviceSinkBuilder, Player as RodioPlayer};

use super::{
    App, Action, DEFAULT_VIDEO_LEN, DEFAULT_IMAGE_LEN, DEFAULT_AUDIO_LEN,
    is_video_path, is_audio_path,
};
use super::timeline::{AudioSource, Clip, ClipSource, ScopeData, VideoSource};

impl App {
    /// Sub-router for **transport / import** actions. Returns `Some(action)` when
    /// the action is not one of ours, `None` once handled.
    pub(crate) fn apply_timeline_playback(&mut self, action: Action) -> Option<Action> {
        match action {
            Action::ImportMedia(path) => {
                if let Some(clip) = self.build_imported_clip(&path) {
                    self.project.clips.push(clip);
                    self.selected = Some(self.project.clips.len() - 1);
                    self.host.mark_dirty();
                }
            }
            Action::Seek(t) => {
                let was_playing = self.playing;
                self.stop_audio();
                self.time = t.clamp(0.0, self.project.duration);
                self.host.mark_dirty();
                self.update_scope_data();
                if was_playing {
                    let nt = self.time;
                    self.start_audio(nt);
                } else {
                    self.scrub_audio_burst(t);
                }
            }
            Action::StepBy(delta) => {
                self.playing = false;
                self.last_tick = None;
                self.time = (self.time + delta).clamp(0.0, self.project.duration);
                self.host.mark_dirty();
            }
            Action::TogglePlay => {
                self.playing = !self.playing;
                if self.playing {
                    if self.time >= self.project.duration.max(1e-3) {
                        self.time = 0.0;
                        self.host.mark_dirty();
                    }
                    self.last_tick = Some(std::time::Instant::now());
                    let t = self.time;
                    self.start_audio(t);
                } else {
                    self.last_tick = None;
                    self.stop_audio();
                }
            }
            Action::Pause => {
                self.playing = false;
                self.last_tick = None;
                self.stop_audio();
            }
            other => return Some(other),
        }
        None
    }

    /// Advance the playhead by wall-clock elapsed time. Returns `false` when
    /// playback stops (end of sequence or already paused).
    pub fn tick(&mut self) -> bool {
        if !self.playing {
            self.audio_playing = false;
            return false;
        }
        let now = Instant::now();
        let dt = match self.last_tick {
            Some(prev) => now.duration_since(prev).as_secs_f32().min(0.1),
            None => 0.0,
        };
        self.last_tick = Some(now);

        let dur = self.project.duration.max(1e-3);
        let next = self.time + dt;
        if next >= dur {
            self.time = dur;
            self.playing = false;
            self.last_tick = None;
            self.stop_audio();
            self.host.mark_dirty();
            return false;
        }
        self.time = next;
        self.host.mark_dirty();
        true
    }

    /// Start audio playback from `start_t` to the end of the sequence.
    pub(crate) fn start_audio(&mut self, start_t: f32) {
        self.stop_audio();
        let mix_clips = crate::export::build_mix_clips(
            &self.project, &self.track_eq, &self.track_comp, &self.track_types, &self.audio_effects,
        );
        if mix_clips.is_empty() { return; }
        let fps = self.project.fps.max(1.0);
        let dur = self.project.duration.max(1e-3);
        let first_frame = (start_t * fps).round() as u64;
        let total_frames = ((dur - start_t).max(0.0) * fps).ceil() as u64;
        if total_frames == 0 { return; }
        let plan = crate::export::FramePlan {
            first: first_frame,
            last: first_frame + total_frames - 1,
            count: total_frames,
        };
        let audio_mix = crate::export::render_program_audio(&mix_clips, plan, fps);
        if audio_mix.samples.is_empty() { return; }
        let device_sink = match DeviceSinkBuilder::open_default_sink() {
            Ok(s) => s,
            Err(e) => { log::warn!("reel-gpui: rodio open_default_sink: {e}"); return; }
        };
        let channels = std::num::NonZero::new(audio_mix.channels.max(1)).unwrap();
        let rate = std::num::NonZero::new(audio_mix.sample_rate.max(1)).unwrap();
        let source = rodio::buffer::SamplesBuffer::new(channels, rate, audio_mix.samples.clone());
        let player = RodioPlayer::connect_new(device_sink.mixer());
        player.append(source);
        player.set_volume(self.master_volume.clamp(0.0, 2.0));
        player.play();
        self.audio_device_sink = Some(device_sink);
        self.audio_player = Some(player);
        self.audio_playing = true;
    }

    /// Stop and drop the active audio player + device sink.
    pub(crate) fn stop_audio(&mut self) {
        if let Some(player) = self.audio_player.take() {
            player.stop();
        }
        self.audio_device_sink = None;
        self.audio_playing = false;
    }

    /// Play a short 100ms audio burst at `t` when scrubbing while paused.
    pub(crate) fn scrub_audio_burst(&mut self, t: f32) {
        let mix_clips = crate::export::build_mix_clips(
            &self.project, &self.track_eq, &self.track_comp, &self.track_types, &self.audio_effects,
        );
        if mix_clips.is_empty() { return; }
        let fps = self.project.fps.max(1.0);
        let burst_frames = ((0.1 * fps).ceil() as u64).max(1);
        let first_frame = (t * fps).floor() as u64;
        let plan = crate::export::FramePlan {
            first: first_frame,
            last: first_frame + burst_frames - 1,
            count: burst_frames,
        };
        let audio_mix = crate::export::render_program_audio(&mix_clips, plan, fps);
        if audio_mix.samples.is_empty() { return; }
        let Ok(device_sink) = DeviceSinkBuilder::open_default_sink() else { return; };
        let channels = std::num::NonZero::new(audio_mix.channels.max(1)).unwrap();
        let rate = std::num::NonZero::new(audio_mix.sample_rate.max(1)).unwrap();
        let source = rodio::buffer::SamplesBuffer::new(channels, rate, audio_mix.samples.clone());
        let player = RodioPlayer::connect_new(device_sink.mixer());
        player.append(source);
        player.set_volume(self.master_volume.clamp(0.0, 2.0));
        player.play();
        drop(player);
        drop(device_sink);
    }

    /// Build a clip from `path` placed at the playhead on the first visible track.
    pub(crate) fn build_imported_clip(&self, path: &std::path::Path) -> Option<Clip> {
        if path.as_os_str().is_empty() { return None; }
        let name = path.file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "media".into());
        let start = self.snap_to_frame(self.time);
        let track = self.first_visible_track();
        let (source, duration) = if is_video_path(path) {
            let (video, probed_len) = VideoSource::probed(path);
            (ClipSource::Video(video), probed_len.unwrap_or(DEFAULT_VIDEO_LEN))
        } else if is_audio_path(path) {
            let (audio, probed_len) = AudioSource::probed(path);
            (ClipSource::Audio(audio), probed_len.unwrap_or(DEFAULT_AUDIO_LEN))
        } else {
            (ClipSource::Image(path.to_path_buf()), DEFAULT_IMAGE_LEN)
        };
        Some(Clip {
            name,
            source,
            track,
            start,
            duration: duration.max(0.001),
            ..Clip::default()
        })
    }

    /// Compute scope data from the last rendered program frame stored in the host.
    pub fn update_scope_data(&mut self) {
        let rgba = &self.host.last_rgba;
        if rgba.is_empty() { self.scope_data = None; return; }
        let (w, h) = self.host.last_dims;
        if w == 0 || h == 0 { self.scope_data = None; return; }
        let cols = 256usize;
        let mut waveform_cols: Vec<Vec<f32>> = vec![Vec::new(); cols];
        let mut vectorscope_dots: Vec<(f32, f32)> = Vec::new();
        let mut hist_r = vec![0u32; 256];
        let mut hist_g = vec![0u32; 256];
        let mut hist_b = vec![0u32; 256];
        let mut total_pixels = 0u32;
        for (i, px) in rgba.chunks_exact(4).enumerate() {
            let r = px[0] as f32 / 255.0;
            let g = px[1] as f32 / 255.0;
            let b = px[2] as f32 / 255.0;
            let x = (i as u32) % w;
            let col = (x as usize * cols / w as usize).min(cols - 1);
            let luma = 0.299 * r + 0.587 * g + 0.114 * b;
            waveform_cols[col].push(luma);
            if i % 8 == 0 {
                let u = -0.147 * r - 0.289 * g + 0.436 * b;
                let v = 0.615 * r - 0.515 * g - 0.100 * b;
                vectorscope_dots.push((u, v));
            }
            hist_r[px[0] as usize] += 1;
            hist_g[px[1] as usize] += 1;
            hist_b[px[2] as usize] += 1;
            total_pixels += 1;
        }
        let norm = if total_pixels > 0 { total_pixels as f32 } else { 1.0 };
        let hist_r: Vec<f32> = hist_r.iter().map(|&v| v as f32 / norm).collect();
        let hist_g: Vec<f32> = hist_g.iter().map(|&v| v as f32 / norm).collect();
        let hist_b: Vec<f32> = hist_b.iter().map(|&v| v as f32 / norm).collect();
        self.scope_data = Some(ScopeData {
            waveform_cols,
            vectorscope_dots,
            hist_r,
            hist_g,
            hist_b,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::super::{App, Action, DEFAULT_IMAGE_LEN};
    use super::super::timeline::ClipSource;
    use std::path::PathBuf;

    #[test]
    fn import_image_places_a_clip_at_the_playhead() {
        let mut app = App::new();
        app.time = 7.0;
        let before = app.project.clips.len();
        app.apply(Action::ImportMedia("/tmp/shot.png".into()));
        assert_eq!(app.project.clips.len(), before + 1);
        let clip = app.project.clips.last().unwrap();
        assert!(matches!(clip.source, ClipSource::Image(_)));
        assert_eq!(clip.name, "shot");
        assert!((clip.start - app.snap_to_frame(7.0)).abs() < 1e-5);
        assert_eq!(clip.track, 0);
        assert!((clip.duration - DEFAULT_IMAGE_LEN).abs() < 1e-5);
        assert_eq!(app.selected, Some(app.project.clips.len() - 1));
    }

    #[test]
    fn import_skips_an_empty_path() {
        let mut app = App::new();
        let before = app.project.clips.len();
        app.apply(Action::ImportMedia(PathBuf::new()));
        assert_eq!(app.project.clips.len(), before);
    }

    #[test]
    fn seek_action_clamps_and_marks_dirty() {
        let mut app = App::new();
        app.apply(Action::Seek(12.5));
        assert!((app.time - 12.5).abs() < 1e-6);
        app.apply(Action::Seek(9999.0));
        assert!((app.time - app.project.duration).abs() < 1e-6);
        app.apply(Action::Seek(-5.0));
        assert!((app.time - 0.0).abs() < 1e-6);
    }

    #[test]
    fn toggle_play_anchors_and_clears_clock() {
        let mut app = App::new();
        assert!(!app.playing);
        app.apply(Action::TogglePlay);
        assert!(app.playing);
        let before = app.time;
        let alive = app.tick();
        assert!(alive);
        assert!(app.time >= before);
        app.apply(Action::TogglePlay);
        assert!(!app.playing);
        assert!(!app.tick());
    }

    #[test]
    fn tick_stops_at_sequence_end() {
        let mut app = App::new();
        app.apply(Action::TogglePlay);
        app.time = app.project.duration;
        let alive = app.tick();
        assert!(!alive, "tick at the end should stop the loop");
        assert!(!app.playing, "playback halts at the sequence end");
        assert!((app.time - app.project.duration).abs() < 1e-3);
    }

    #[test]
    fn toggle_play_at_end_rewinds_to_head() {
        let mut app = App::new();
        app.time = app.project.duration;
        app.apply(Action::TogglePlay);
        assert!(app.playing);
        assert!((app.time - 0.0).abs() < 1e-6, "play from the end rewinds");
    }

    #[test]
    fn frame_step_halts_playback() {
        let mut app = App::new();
        app.apply(Action::TogglePlay);
        assert!(app.playing);
        app.apply(Action::StepBy(1.0 / 30.0));
        assert!(!app.playing, "a manual frame step stops the play loop");
    }
}
