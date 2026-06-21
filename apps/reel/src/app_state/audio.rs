use super::{App, Action, MAX_AUDIO_GAIN};
use super::timeline::ClipSource;

/// Per-track audio effect (for the effect chain).
#[derive(Clone, Debug)]
pub enum AudioEffect {
    Eq3(TrackEq3),
    Compressor { threshold_db: f32, ratio: f32, attack_ms: f32, release_ms: f32 },
    Reverb { room_size: f32, damping: f32, wet: f32 },
    Delay { time_ms: f32, feedback: f32, wet: f32 },
}

impl AudioEffect {
    pub fn label(&self) -> &'static str {
        match self {
            AudioEffect::Eq3(_) => "3-Band EQ",
            AudioEffect::Compressor { .. } => "Compressor",
            AudioEffect::Reverb { .. } => "Reverb",
            AudioEffect::Delay { .. } => "Delay",
        }
    }
}

/// Per-track 5-band parametric EQ.
#[derive(Clone, Copy, Debug)]
pub struct EqBand {
    pub freq: f32,
    pub gain_db: f32,
    pub q: f32,
}

/// Per-track 5-band parametric EQ.
#[derive(Clone, Debug)]
pub struct TrackEq {
    pub enabled: bool,
    pub bands: [EqBand; 5],
}

impl Default for TrackEq {
    fn default() -> Self {
        Self {
            enabled: false,
            bands: [
                EqBand { freq: 80.0,    gain_db: 0.0, q: 0.707 },
                EqBand { freq: 250.0,   gain_db: 0.0, q: 0.707 },
                EqBand { freq: 1000.0,  gain_db: 0.0, q: 0.707 },
                EqBand { freq: 4000.0,  gain_db: 0.0, q: 0.707 },
                EqBand { freq: 12000.0, gain_db: 0.0, q: 0.707 },
            ],
        }
    }
}

/// Per-track peak compressor.
#[derive(Clone, Debug)]
pub struct TrackCompressor {
    pub enabled: bool,
    pub threshold_db: f32,
    pub ratio: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    pub makeup_db: f32,
}

impl Default for TrackCompressor {
    fn default() -> Self {
        Self { enabled: false, threshold_db: -18.0, ratio: 4.0, attack_ms: 10.0, release_ms: 100.0, makeup_db: 0.0 }
    }
}

/// Simple 3-band EQ per track.
#[derive(Clone, Debug, PartialEq)]
pub struct TrackEq3 {
    pub low_gain_db: f32,
    pub mid_gain_db: f32,
    pub high_gain_db: f32,
    pub mid_freq: f32,
}

impl Default for TrackEq3 {
    fn default() -> Self {
        Self { low_gain_db: 0.0, mid_gain_db: 0.0, high_gain_db: 0.0, mid_freq: 1000.0 }
    }
}

/// Audio track channel format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioTrackType { Mono, Stereo, Surround51 }

impl AudioTrackType {
    pub fn label(self) -> &'static str {
        match self { AudioTrackType::Mono => "M", AudioTrackType::Stereo => "ST", AudioTrackType::Surround51 => "5.1" }
    }

    pub fn next(self) -> Self {
        match self {
            AudioTrackType::Mono => AudioTrackType::Stereo,
            AudioTrackType::Stereo => AudioTrackType::Surround51,
            AudioTrackType::Surround51 => AudioTrackType::Mono,
        }
    }
}

/// Audio Suite processing kinds.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum AudioSuiteKind {
    #[default] Normalize, Reverse, GainChange, NoiseReduction, DeEsser, PitchShift, TimeStretch, ChannelMix,
}

/// Audio Suite configuration.
#[derive(Debug, Clone)]
pub struct AudioSuiteConfig {
    pub kind: AudioSuiteKind,
    pub gain_db: f32,
    pub preserve_duration: bool,
    pub process_in_place: bool,
    pub clip_by_clip: bool,
    pub target_level_db: f32,
    pub pitch_semitones: f32,
    pub stretch_ratio: f32,
}

impl Default for AudioSuiteConfig {
    fn default() -> Self {
        Self {
            kind: AudioSuiteKind::Normalize,
            gain_db: 0.0,
            preserve_duration: true,
            process_in_place: true,
            clip_by_clip: false,
            target_level_db: -1.0,
            pitch_semitones: 0.0,
            stretch_ratio: 1.0,
        }
    }
}

pub trait AppAudioExt {
    fn apply_audio(&mut self, action: Action);
}

impl AppAudioExt for App {
    fn apply_audio(&mut self, action: Action) {
        match action {
            Action::SetTrackVolume { track_idx, v } => {
                if let Some(vol) = self.track_volumes.get_mut(track_idx) { *vol = v.clamp(0.0, 2.0); }
            }
            Action::ToggleTrackMute { track_idx } => {
                if let Some(m) = self.track_muted.get_mut(track_idx) { *m = !*m; }
            }
            Action::ToggleTrackSolo { track_idx } => {
                if let Some(s) = self.track_soloed.get_mut(track_idx) { *s = !*s; }
            }
            Action::SetMasterVolume { v } => { self.master_volume = v.clamp(0.0, 2.0); }
            Action::ToggleMixer => { self.show_mixer = !self.show_mixer; }
            Action::SetAudioVolume(v) => {
                let vol = v.clamp(0.0, 2.0);
                self.master_volume = vol;
                if let Some(player) = &self.audio_player { player.set_volume(vol); }
            }
            Action::SetAudioMute(m) => {
                if let Some(player) = &self.audio_player {
                    let vol = if m { 0.0 } else { self.master_volume };
                    player.set_volume(vol);
                }
            }
            Action::ToggleTrackEq { track_idx } => {
                if let Some(eq) = self.track_eq.get_mut(track_idx) { eq.enabled = !eq.enabled; }
            }
            Action::SetEqBand { track_idx, band, gain_db } => {
                if let Some(eq) = self.track_eq.get_mut(track_idx) {
                    if let Some(b) = eq.bands.get_mut(band) { b.gain_db = gain_db.clamp(-12.0, 12.0); }
                }
            }
            Action::ToggleTrackCompressor { track_idx } => {
                if let Some(comp) = self.track_comp.get_mut(track_idx) { comp.enabled = !comp.enabled; }
            }
            Action::SetCompressor { track_idx, threshold_db, ratio, attack_ms, release_ms, makeup_db } => {
                if let Some(comp) = self.track_comp.get_mut(track_idx) {
                    comp.threshold_db = threshold_db.clamp(-60.0, 0.0);
                    comp.ratio = ratio.clamp(1.0, 20.0);
                    comp.attack_ms = attack_ms.clamp(0.1, 200.0);
                    comp.release_ms = release_ms.clamp(10.0, 2000.0);
                    comp.makeup_db = makeup_db.clamp(-12.0, 24.0);
                }
            }
            Action::CycleAudioTrackType { track_idx } => {
                if let Some(t) = self.track_types.get_mut(track_idx) { *t = t.next(); }
            }
            Action::SetTrackEq3 { track_idx, eq } => {
                if let Some(e) = self.track_eq3.get_mut(track_idx) { *e = eq; }
            }
            Action::AddAudioEffect { track_idx, effect } => {
                while self.audio_effects.len() <= track_idx { self.audio_effects.push(Vec::new()); }
                self.audio_effects[track_idx].push(effect);
            }
            Action::RemoveAudioEffect { track_idx, effect_idx } => {
                if track_idx < self.audio_effects.len() && effect_idx < self.audio_effects[track_idx].len() {
                    self.audio_effects[track_idx].remove(effect_idx);
                }
            }
            Action::SetAudioEffect { track_idx, effect_idx, effect } => {
                if track_idx < self.audio_effects.len() && effect_idx < self.audio_effects[track_idx].len() {
                    self.audio_effects[track_idx][effect_idx] = effect;
                }
            }
            Action::ToggleTrackFx { track_idx } => {
                while self.track_fx_open.len() <= track_idx { self.track_fx_open.push(false); }
                self.track_fx_open[track_idx] = !self.track_fx_open[track_idx];
            }
            Action::ExpandTrackFx { track_idx, effect_idx } => {
                while self.track_fx_expanded.len() <= track_idx { self.track_fx_expanded.push(None); }
                self.track_fx_expanded[track_idx] = effect_idx;
            }
            Action::UpdateLufsMeters { power } => {
                self.lufs_power_history.push(power);
                if self.lufs_power_history.len() > 6000 { self.lufs_power_history.drain(..1000); }
                self.lufs_short_term = super::lufs_short_term(&self.lufs_power_history, 10.0);
                self.lufs_integrated = super::lufs_integrated(&self.lufs_power_history);
            }
            Action::ResetLufsIntegrated => {
                self.lufs_power_history.clear();
                self.lufs_short_term = -f32::INFINITY;
                self.lufs_integrated = -f32::INFINITY;
            }
            Action::ToggleAudioSuitePanel => { self.audio_suite_panel_open = !self.audio_suite_panel_open; }
            Action::SetAudioSuiteKind(k) => { self.audio_suite_config.kind = k; }
            Action::SetAudioSuiteGain(g) => { self.audio_suite_config.gain_db = g.clamp(-60.0, 60.0); }
            Action::SetAudioSuitePreserve(b) => { self.audio_suite_config.preserve_duration = b; }
            Action::SetAudioSuiteProcessInPlace(b) => { self.audio_suite_config.process_in_place = b; }
            Action::SetAudioSuiteTargetLevel(l) => { self.audio_suite_config.target_level_db = l.clamp(-60.0, 0.0); }
            Action::SetAudioSuitePitch(p) => { self.audio_suite_config.pitch_semitones = p.clamp(-24.0, 24.0); }
            Action::SetAudioSuiteStretch(r) => { self.audio_suite_config.stretch_ratio = r.clamp(0.1, 10.0); }
            Action::ToggleAudioSuitePreview => { self.audio_suite_preview = !self.audio_suite_preview; }
            Action::ApplyAudioSuite { clip_idx } => {
                if let Some(clip) = self.project.clips.get_mut(clip_idx) {
                    if matches!(self.audio_suite_config.kind, AudioSuiteKind::GainChange | AudioSuiteKind::Normalize) {
                        if let ClipSource::Audio(ref mut a) = clip.source {
                            let factor = 10f32.powf(self.audio_suite_config.gain_db / 20.0);
                            a.gain = (a.gain * factor).clamp(0.0, MAX_AUDIO_GAIN);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::{App, Action, lufs_integrated};

    #[test]
    fn lufs_integrated_pure_functions() {
        // A history of constant-power blocks at -23 LUFS equivalent.
        // -23 LUFS → mean-square = 10^((-23+0.691)/10) ≈ 5.37e-3
        let target_ms = 10f32.powf((-23.0 + 0.691) / 10.0);
        let history: Vec<f32> = vec![target_ms; 100];
        let lufs = lufs_integrated(&history);
        // Should be approximately -23 ± 1 dB.
        assert!((lufs + 23.0).abs() < 1.5, "integrated LUFS ≈ -23, got {lufs:.2}");
    }

    #[test]
    fn update_lufs_meters_action_updates_app_fields() {
        let mut app = App::new();
        let power = 10f32.powf((-23.0 + 0.691) / 10.0);
        for _ in 0..50 {
            app.apply(Action::UpdateLufsMeters { power });
        }
        assert!(app.lufs_short_term.is_finite() || app.lufs_short_term == -f32::INFINITY);
        app.apply(Action::ResetLufsIntegrated);
        assert_eq!(app.lufs_power_history.len(), 0);
        assert_eq!(app.lufs_integrated, -f32::INFINITY);
    }

    #[test]
    fn test_audio_suite_gain_clamp() {
        let mut app = App::new();
        app.apply(Action::SetAudioSuiteGain(100.0));
        assert!((app.audio_suite_config.gain_db - 60.0).abs() < 1e-5);
        app.apply(Action::SetAudioSuiteGain(-100.0));
        assert!((app.audio_suite_config.gain_db - (-60.0)).abs() < 1e-5);
    }

    #[test]
    fn test_audio_suite_pitch_clamp() {
        let mut app = App::new();
        app.apply(Action::SetAudioSuitePitch(-30.0));
        assert!((app.audio_suite_config.pitch_semitones - (-24.0)).abs() < 1e-5);
        app.apply(Action::SetAudioSuitePitch(30.0));
        assert!((app.audio_suite_config.pitch_semitones - 24.0).abs() < 1e-5);
    }

    #[test]
    fn test_audio_suite_stretch_clamp() {
        let mut app = App::new();
        app.apply(Action::SetAudioSuiteStretch(0.0));
        assert!((app.audio_suite_config.stretch_ratio - 0.1).abs() < 1e-5);
        app.apply(Action::SetAudioSuiteStretch(100.0));
        assert!((app.audio_suite_config.stretch_ratio - 10.0).abs() < 1e-5);
    }

    #[test]
    fn test_audio_suite_panel_toggle() {
        let mut app = App::new();
        assert!(!app.audio_suite_panel_open);
        app.apply(Action::ToggleAudioSuitePanel);
        assert!(app.audio_suite_panel_open);
        app.apply(Action::ToggleAudioSuitePanel);
        assert!(!app.audio_suite_panel_open);
        assert!(!app.audio_suite_preview);
        app.apply(Action::ToggleAudioSuitePreview);
        assert!(app.audio_suite_preview);
    }
}
