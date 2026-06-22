//! Mixer domain — `MixerChannel`, `EqBand`, `Compressor` types + mixer apply
//! methods + tests.

// ─── Types ────────────────────────────────────────────────────────────────────

/// The mixer state for one track channel.
#[derive(Clone, Debug)]
pub struct MixerChannel {
    pub track_id: usize,
    /// Signal level measured before the fader (read-only metering).
    pub pre_fader_level: f32,
    /// Signal level measured after the fader (read-only metering).
    pub post_fader_level: f32,
    /// Peak hold level for the level meter.
    pub peak_level: f32,
    /// Ordered list of effect plugin names in the insert chain.
    pub effects: Vec<String>,
    /// Low-shelf gain in dB. Clamped -12.0..=12.0.
    pub eq_low: f32,
    /// Mid-peak gain in dB. Clamped -12.0..=12.0.
    pub eq_mid: f32,
    /// High-shelf gain in dB. Clamped -12.0..=12.0.
    pub eq_high: f32,
    /// Compressor threshold in dB. Clamped -60.0..=0.0.
    pub comp_threshold: f32,
    /// Compressor ratio (e.g. 4.0 = 4:1). Clamped 1.0..=20.0.
    pub comp_ratio: f32,
    /// Whether the compressor insert is active.
    pub comp_enabled: bool,
    /// Parametric EQ bands for detailed EQ visualiser support.
    pub eq_bands: Vec<EqBand>,
    /// Sends to bus tracks: list of (bus_track_id, send_level).
    pub channel_sends: Vec<(usize, f32)>,
    /// Pre-fader listen (solo-in-place) mode.
    pub pre_fader_listen: bool,
    /// Invert the phase of the signal.
    pub phase_invert: bool,
    /// Stereo width. 0.0 = mono, 1.0 = normal, 2.0 = extra wide. Clamped 0.0..=2.0.
    pub stereo_width: f32,
    /// Trim gain in dB before the fader. Clamped -6.0..=6.0.
    pub trim_db: f32,
}

impl MixerChannel {
    pub fn new(track_id: usize) -> Self {
        Self {
            track_id,
            pre_fader_level: 0.0,
            post_fader_level: 0.0,
            peak_level: 0.0,
            effects: Vec::new(),
            eq_low: 0.0,
            eq_mid: 0.0,
            eq_high: 0.0,
            comp_threshold: -18.0,
            comp_ratio: 4.0,
            comp_enabled: false,
            eq_bands: Vec::new(),
            channel_sends: Vec::new(),
            pre_fader_listen: false,
            phase_invert: false,
            stereo_width: 1.0,
            trim_db: 0.0,
        }
    }
}

/// A parametric EQ band for frequency-specific gain adjustment.
#[derive(Clone, Debug)]
pub struct EqBand {
    /// Center frequency in Hz.
    pub freq: f32,
    /// Gain in dB. Clamped -12.0..=12.0.
    pub gain_db: f32,
    /// Q factor (0.1..=10.0).
    pub q: f32,
}

// ─── Apply methods ────────────────────────────────────────────────────────────

use super::{Action, App};

impl App {
    pub(super) fn apply_mixer(&mut self, action: Action) {
        match action {
            Action::SetChannelEqLow { track_id, gain_db } => {
                if let Some(m) = self.find_mixer_mut(track_id) {
                    m.eq_low = gain_db.clamp(-12.0, 12.0);
                }
            }
            Action::SetChannelEqMid { track_id, gain_db } => {
                if let Some(m) = self.find_mixer_mut(track_id) {
                    m.eq_mid = gain_db.clamp(-12.0, 12.0);
                }
            }
            Action::SetChannelEqHigh { track_id, gain_db } => {
                if let Some(m) = self.find_mixer_mut(track_id) {
                    m.eq_high = gain_db.clamp(-12.0, 12.0);
                }
            }
            Action::SetChannelCompThreshold { track_id, threshold_db } => {
                if let Some(m) = self.find_mixer_mut(track_id) {
                    m.comp_threshold = threshold_db.clamp(-60.0, 0.0);
                }
            }
            Action::SetChannelCompRatio { track_id, ratio } => {
                if let Some(m) = self.find_mixer_mut(track_id) {
                    m.comp_ratio = ratio.clamp(1.0, 20.0);
                }
            }
            Action::ToggleChannelComp(track_id) => {
                if let Some(m) = self.find_mixer_mut(track_id) {
                    m.comp_enabled = !m.comp_enabled;
                }
            }
            Action::AddChannelEffect { track_id, effect } => {
                if let Some(m) = self.find_mixer_mut(track_id) {
                    m.effects.push(effect);
                }
            }
            Action::RemoveChannelEffect { track_id, index } => {
                if let Some(m) = self.find_mixer_mut(track_id) {
                    if index < m.effects.len() {
                        m.effects.remove(index);
                    }
                }
            }
            Action::SetMasterVolume(vol) => {
                self.master_volume = vol.clamp(0.0, 2.0);
            }
            Action::ToggleMasterLimiter => {
                self.master_limiter = !self.master_limiter;
            }
            // ── Extended mixer params ─────────────────────────────────────────
            Action::AddChannelSend { channel_id, bus_id, level } => {
                if let Some(m) = self.find_mixer_mut(channel_id) {
                    if !m.channel_sends.iter().any(|(bid, _)| *bid == bus_id) {
                        m.channel_sends.push((bus_id, level.clamp(0.0, 1.0)));
                    }
                }
            }
            Action::RemoveChannelSend { channel_id, bus_id } => {
                if let Some(m) = self.find_mixer_mut(channel_id) {
                    m.channel_sends.retain(|(bid, _)| *bid != bus_id);
                }
            }
            Action::SetChannelSendLevel { channel_id, bus_id, level } => {
                if let Some(m) = self.find_mixer_mut(channel_id) {
                    if let Some(send) = m.channel_sends.iter_mut().find(|(bid, _)| *bid == bus_id) {
                        send.1 = level.clamp(0.0, 1.0);
                    }
                }
            }
            Action::SetChannelPfl { channel_id, pfl } => {
                if let Some(m) = self.find_mixer_mut(channel_id) {
                    m.pre_fader_listen = pfl;
                }
            }
            Action::SetChannelPhaseInvert { channel_id, invert } => {
                if let Some(m) = self.find_mixer_mut(channel_id) {
                    m.phase_invert = invert;
                }
            }
            Action::SetStereoWidth { channel_id, width } => {
                if let Some(m) = self.find_mixer_mut(channel_id) {
                    m.stereo_width = width.clamp(0.0, 2.0);
                }
            }
            Action::SetChannelTrim { channel_id, trim_db } => {
                if let Some(m) = self.find_mixer_mut(channel_id) {
                    m.trim_db = trim_db.clamp(-6.0, 6.0);
                }
            }
            Action::ResetChannel { channel_id } => {
                if let Some(m) = self.find_mixer_mut(channel_id) {
                    m.eq_low = 0.0;
                    m.eq_mid = 0.0;
                    m.eq_high = 0.0;
                    m.comp_threshold = -18.0;
                    m.comp_ratio = 4.0;
                    m.comp_enabled = false;
                    m.stereo_width = 1.0;
                    m.trim_db = 0.0;
                    m.pre_fader_listen = false;
                    m.phase_invert = false;
                    m.channel_sends = Vec::new();
                }
            }
            _ => {}
        }
    }

    /// Compute the approximate EQ frequency response at `freq_hz` for a given
    /// mixer channel, summing bell-curve contributions from each `EqBand`.
    ///
    /// Returns 0.0 if the channel doesn't exist or has no EQ bands.
    pub fn eq_response_db(&self, channel_id: usize, freq_hz: f32) -> f32 {
        let Some(m) = self.mixer_channels.iter().find(|m| m.track_id == channel_id) else {
            return 0.0;
        };
        if m.eq_bands.is_empty() {
            return 0.0;
        }

        let log_freq = freq_hz.log2();
        let mut sum = 0.0_f32;
        for band in &m.eq_bands {
            if band.freq <= 0.0 || band.q <= 0.0 {
                continue;
            }
            let log_center = band.freq.log2();
            let bw = 1.0 / band.q; // bandwidth in octaves
            let exponent = (log_freq - log_center).powi(2) / (2.0 * bw.powi(2));
            sum += band.gain_db * (-exponent).exp();
        }
        sum
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::{Action, App};
    use super::super::tracks::TrackKind;
    use super::EqBand;

    fn fresh() -> App {
        App::new()
    }

    fn add_audio_track(app: &mut App) -> usize {
        app.apply(Action::AddTrack(TrackKind::Audio));
        app.tracks.last().unwrap().id
    }

    #[test]
    fn set_channel_eq_low() {
        let mut app = fresh();
        let tid = add_audio_track(&mut app);
        app.apply(Action::SetChannelEqLow { track_id: tid, gain_db: 6.0 });
        let m = app.mixer_channels.iter().find(|m| m.track_id == tid).unwrap();
        assert_eq!(m.eq_low, 6.0);
    }

    #[test]
    fn set_channel_eq_low_clamp() {
        let mut app = fresh();
        let tid = add_audio_track(&mut app);
        app.apply(Action::SetChannelEqLow { track_id: tid, gain_db: -20.0 });
        let m = app.mixer_channels.iter().find(|m| m.track_id == tid).unwrap();
        assert_eq!(m.eq_low, -12.0);
    }

    #[test]
    fn set_channel_eq_mid() {
        let mut app = fresh();
        let tid = add_audio_track(&mut app);
        app.apply(Action::SetChannelEqMid { track_id: tid, gain_db: -3.0 });
        let m = app.mixer_channels.iter().find(|m| m.track_id == tid).unwrap();
        assert_eq!(m.eq_mid, -3.0);
    }

    #[test]
    fn set_channel_eq_high() {
        let mut app = fresh();
        let tid = add_audio_track(&mut app);
        app.apply(Action::SetChannelEqHigh { track_id: tid, gain_db: 4.0 });
        let m = app.mixer_channels.iter().find(|m| m.track_id == tid).unwrap();
        assert_eq!(m.eq_high, 4.0);
    }

    #[test]
    fn set_channel_comp_threshold() {
        let mut app = fresh();
        let tid = add_audio_track(&mut app);
        app.apply(Action::SetChannelCompThreshold { track_id: tid, threshold_db: -24.0 });
        let m = app.mixer_channels.iter().find(|m| m.track_id == tid).unwrap();
        assert_eq!(m.comp_threshold, -24.0);
    }

    #[test]
    fn set_channel_comp_threshold_clamp() {
        let mut app = fresh();
        let tid = add_audio_track(&mut app);
        app.apply(Action::SetChannelCompThreshold { track_id: tid, threshold_db: 10.0 });
        let m = app.mixer_channels.iter().find(|m| m.track_id == tid).unwrap();
        assert_eq!(m.comp_threshold, 0.0);
    }

    #[test]
    fn set_channel_comp_ratio() {
        let mut app = fresh();
        let tid = add_audio_track(&mut app);
        app.apply(Action::SetChannelCompRatio { track_id: tid, ratio: 8.0 });
        let m = app.mixer_channels.iter().find(|m| m.track_id == tid).unwrap();
        assert_eq!(m.comp_ratio, 8.0);
    }

    #[test]
    fn set_channel_comp_ratio_clamp() {
        let mut app = fresh();
        let tid = add_audio_track(&mut app);
        app.apply(Action::SetChannelCompRatio { track_id: tid, ratio: 50.0 });
        let m = app.mixer_channels.iter().find(|m| m.track_id == tid).unwrap();
        assert_eq!(m.comp_ratio, 20.0);
    }

    #[test]
    fn toggle_channel_comp() {
        let mut app = fresh();
        let tid = add_audio_track(&mut app);
        assert!(!app.mixer_channels.iter().find(|m| m.track_id == tid).unwrap().comp_enabled);
        app.apply(Action::ToggleChannelComp(tid));
        assert!(app.mixer_channels.iter().find(|m| m.track_id == tid).unwrap().comp_enabled);
        app.apply(Action::ToggleChannelComp(tid));
        assert!(!app.mixer_channels.iter().find(|m| m.track_id == tid).unwrap().comp_enabled);
    }

    #[test]
    fn add_remove_channel_effect() {
        let mut app = fresh();
        let tid = add_audio_track(&mut app);
        app.apply(Action::AddChannelEffect { track_id: tid, effect: "Reverb".to_string() });
        app.apply(Action::AddChannelEffect { track_id: tid, effect: "Delay".to_string() });
        let m = app.mixer_channels.iter().find(|m| m.track_id == tid).unwrap();
        assert_eq!(m.effects.len(), 2);
        app.apply(Action::RemoveChannelEffect { track_id: tid, index: 0 });
        let m = app.mixer_channels.iter().find(|m| m.track_id == tid).unwrap();
        assert_eq!(m.effects.len(), 1);
        assert_eq!(m.effects[0], "Delay");
    }

    #[test]
    fn set_master_volume() {
        let mut app = fresh();
        app.apply(Action::SetMasterVolume(0.8));
        assert_eq!(app.master_volume, 0.8);
    }

    #[test]
    fn set_master_volume_clamp() {
        let mut app = fresh();
        app.apply(Action::SetMasterVolume(3.0));
        assert_eq!(app.master_volume, 2.0);
    }

    #[test]
    fn toggle_master_limiter() {
        let mut app = fresh();
        assert!(!app.master_limiter);
        app.apply(Action::ToggleMasterLimiter);
        assert!(app.master_limiter);
        app.apply(Action::ToggleMasterLimiter);
        assert!(!app.master_limiter);
    }

    #[test]
    fn mixer_channel_effect_chain() {
        let mut app = fresh();
        let tid = add_audio_track(&mut app);
        app.apply(Action::AddChannelEffect { track_id: tid, effect: "3-Band EQ".to_string() });
        app.apply(Action::AddChannelEffect { track_id: tid, effect: "Compressor".to_string() });
        app.apply(Action::AddChannelEffect { track_id: tid, effect: "Reverb".to_string() });
        let m = app.mixer_channels.iter().find(|m| m.track_id == tid).unwrap();
        assert_eq!(m.effects.len(), 3);
        app.apply(Action::RemoveChannelEffect { track_id: tid, index: 1 });
        let m = app.mixer_channels.iter().find(|m| m.track_id == tid).unwrap();
        assert_eq!(m.effects[0], "3-Band EQ");
        assert_eq!(m.effects[1], "Reverb");
    }

    #[test]
    fn app_default_has_master_mixer_channel() {
        let app = fresh();
        assert!(app.mixer_channels.iter().any(|m| m.track_id == 0));
    }

    #[test]
    fn remove_effect_out_of_bounds_is_noop() {
        let mut app = fresh();
        let tid = add_audio_track(&mut app);
        app.apply(Action::AddChannelEffect { track_id: tid, effect: "Reverb".to_string() });
        app.apply(Action::RemoveChannelEffect { track_id: tid, index: 99 });
        let m = app.mixer_channels.iter().find(|m| m.track_id == tid).unwrap();
        assert_eq!(m.effects.len(), 1);
    }

    // ── EQ response helper tests ──────────────────────────────────────────────

    #[test]
    fn eq_response_no_bands_returns_zero() {
        let app = fresh();
        let resp = app.eq_response_db(0, 1000.0);
        assert!((resp).abs() < 0.001);
    }

    #[test]
    fn eq_response_nonexistent_channel_returns_zero() {
        let app = fresh();
        let resp = app.eq_response_db(9999, 1000.0);
        assert!((resp).abs() < 0.001);
    }

    #[test]
    fn eq_response_at_band_center_approximates_gain() {
        let mut app = fresh();
        let tid = add_audio_track(&mut app);
        // Add a band centered at 1000 Hz with +6 dB gain
        if let Some(m) = app.mixer_channels.iter_mut().find(|m| m.track_id == tid) {
            m.eq_bands.push(EqBand { freq: 1000.0, gain_db: 6.0, q: 1.0 });
        }
        // At center frequency, response should be close to gain_db
        let resp = app.eq_response_db(tid, 1000.0);
        assert!((resp - 6.0).abs() < 0.5, "Expected ~6 dB at center, got {}", resp);
    }

    #[test]
    fn eq_response_far_from_band_is_near_zero() {
        let mut app = fresh();
        let tid = add_audio_track(&mut app);
        if let Some(m) = app.mixer_channels.iter_mut().find(|m| m.track_id == tid) {
            m.eq_bands.push(EqBand { freq: 1000.0, gain_db: 6.0, q: 1.0 });
        }
        // Far from center (100x away = ~6.6 octaves), response should be near 0
        let resp = app.eq_response_db(tid, 100000.0);
        assert!(resp.abs() < 0.1, "Expected ~0 dB far from center, got {}", resp);
    }

    #[test]
    fn eq_response_negative_gain_band() {
        let mut app = fresh();
        let tid = add_audio_track(&mut app);
        if let Some(m) = app.mixer_channels.iter_mut().find(|m| m.track_id == tid) {
            m.eq_bands.push(EqBand { freq: 500.0, gain_db: -6.0, q: 2.0 });
        }
        let resp = app.eq_response_db(tid, 500.0);
        assert!((resp - (-6.0)).abs() < 0.5, "Expected ~-6 dB at center, got {}", resp);
    }

    #[test]
    fn eq_response_multiple_bands_sum() {
        let mut app = fresh();
        let tid = add_audio_track(&mut app);
        if let Some(m) = app.mixer_channels.iter_mut().find(|m| m.track_id == tid) {
            m.eq_bands.push(EqBand { freq: 1000.0, gain_db: 3.0, q: 1.0 });
            m.eq_bands.push(EqBand { freq: 1000.0, gain_db: 3.0, q: 1.0 });
        }
        // Both bands at same freq with same gain → sum should be ~6 dB
        let resp = app.eq_response_db(tid, 1000.0);
        assert!((resp - 6.0).abs() < 0.5, "Expected ~6 dB from two 3dB bands, got {}", resp);
    }

    // ── Extended mixer param tests ────────────────────────────────────────────

    #[test]
    fn add_channel_send() {
        let mut app = fresh();
        let tid = add_audio_track(&mut app);
        app.apply(Action::AddTrack(super::super::tracks::TrackKind::Bus));
        let bus = app.tracks.last().unwrap().id;
        app.apply(Action::AddChannelSend { channel_id: tid, bus_id: bus, level: 0.7 });
        let m = app.mixer_channels.iter().find(|m| m.track_id == tid).unwrap();
        assert_eq!(m.channel_sends.len(), 1);
        assert_eq!(m.channel_sends[0].0, bus);
        assert!((m.channel_sends[0].1 - 0.7).abs() < 0.001);
    }

    #[test]
    fn add_channel_send_no_duplicates() {
        let mut app = fresh();
        let tid = add_audio_track(&mut app);
        app.apply(Action::AddChannelSend { channel_id: tid, bus_id: 99, level: 0.5 });
        app.apply(Action::AddChannelSend { channel_id: tid, bus_id: 99, level: 0.8 });
        let m = app.mixer_channels.iter().find(|m| m.track_id == tid).unwrap();
        assert_eq!(m.channel_sends.len(), 1);
    }

    #[test]
    fn remove_channel_send() {
        let mut app = fresh();
        let tid = add_audio_track(&mut app);
        app.apply(Action::AddChannelSend { channel_id: tid, bus_id: 5, level: 0.5 });
        app.apply(Action::RemoveChannelSend { channel_id: tid, bus_id: 5 });
        let m = app.mixer_channels.iter().find(|m| m.track_id == tid).unwrap();
        assert!(m.channel_sends.is_empty());
    }

    #[test]
    fn set_channel_send_level() {
        let mut app = fresh();
        let tid = add_audio_track(&mut app);
        app.apply(Action::AddChannelSend { channel_id: tid, bus_id: 5, level: 0.5 });
        app.apply(Action::SetChannelSendLevel { channel_id: tid, bus_id: 5, level: 0.9 });
        let m = app.mixer_channels.iter().find(|m| m.track_id == tid).unwrap();
        assert!((m.channel_sends[0].1 - 0.9).abs() < 0.001);
    }

    #[test]
    fn set_channel_pfl() {
        let mut app = fresh();
        let tid = add_audio_track(&mut app);
        assert!(!app.mixer_channels.iter().find(|m| m.track_id == tid).unwrap().pre_fader_listen);
        app.apply(Action::SetChannelPfl { channel_id: tid, pfl: true });
        assert!(app.mixer_channels.iter().find(|m| m.track_id == tid).unwrap().pre_fader_listen);
    }

    #[test]
    fn set_channel_phase_invert() {
        let mut app = fresh();
        let tid = add_audio_track(&mut app);
        app.apply(Action::SetChannelPhaseInvert { channel_id: tid, invert: true });
        assert!(app.mixer_channels.iter().find(|m| m.track_id == tid).unwrap().phase_invert);
    }

    #[test]
    fn set_stereo_width_clamped() {
        let mut app = fresh();
        let tid = add_audio_track(&mut app);
        app.apply(Action::SetStereoWidth { channel_id: tid, width: 5.0 });
        assert_eq!(app.mixer_channels.iter().find(|m| m.track_id == tid).unwrap().stereo_width, 2.0);
        app.apply(Action::SetStereoWidth { channel_id: tid, width: -1.0 });
        assert_eq!(app.mixer_channels.iter().find(|m| m.track_id == tid).unwrap().stereo_width, 0.0);
    }

    #[test]
    fn set_channel_trim_clamped() {
        let mut app = fresh();
        let tid = add_audio_track(&mut app);
        app.apply(Action::SetChannelTrim { channel_id: tid, trim_db: 10.0 });
        assert_eq!(app.mixer_channels.iter().find(|m| m.track_id == tid).unwrap().trim_db, 6.0);
        app.apply(Action::SetChannelTrim { channel_id: tid, trim_db: -10.0 });
        assert_eq!(app.mixer_channels.iter().find(|m| m.track_id == tid).unwrap().trim_db, -6.0);
    }

    #[test]
    fn reset_channel_restores_all_defaults() {
        let mut app = fresh();
        let tid = add_audio_track(&mut app);
        // Modify everything
        app.apply(Action::SetChannelEqLow { track_id: tid, gain_db: 6.0 });
        app.apply(Action::SetChannelEqMid { track_id: tid, gain_db: -3.0 });
        app.apply(Action::SetChannelEqHigh { track_id: tid, gain_db: 4.0 });
        app.apply(Action::SetChannelCompThreshold { track_id: tid, threshold_db: -30.0 });
        app.apply(Action::SetChannelCompRatio { track_id: tid, ratio: 8.0 });
        app.apply(Action::ToggleChannelComp(tid));
        app.apply(Action::SetStereoWidth { channel_id: tid, width: 1.5 });
        app.apply(Action::SetChannelTrim { channel_id: tid, trim_db: 3.0 });
        app.apply(Action::SetChannelPfl { channel_id: tid, pfl: true });
        app.apply(Action::SetChannelPhaseInvert { channel_id: tid, invert: true });
        app.apply(Action::AddChannelSend { channel_id: tid, bus_id: 5, level: 0.5 });
        // Reset
        app.apply(Action::ResetChannel { channel_id: tid });
        let m = app.mixer_channels.iter().find(|m| m.track_id == tid).unwrap();
        assert_eq!(m.eq_low, 0.0);
        assert_eq!(m.eq_mid, 0.0);
        assert_eq!(m.eq_high, 0.0);
        assert_eq!(m.comp_threshold, -18.0);
        assert_eq!(m.comp_ratio, 4.0);
        assert!(!m.comp_enabled);
        assert_eq!(m.stereo_width, 1.0);
        assert_eq!(m.trim_db, 0.0);
        assert!(!m.pre_fader_listen);
        assert!(!m.phase_invert);
        assert!(m.channel_sends.is_empty());
    }

    #[test]
    fn mixer_channel_defaults_have_correct_new_fields() {
        let app = fresh();
        let m = app.mixer_channels.iter().find(|m| m.track_id == 0).unwrap();
        assert!(m.channel_sends.is_empty());
        assert!(!m.pre_fader_listen);
        assert!(!m.phase_invert);
        assert_eq!(m.stereo_width, 1.0);
        assert_eq!(m.trim_db, 0.0);
    }
}
