use super::*;

#[derive(Clone, Debug)]
pub struct AudioBus {
    pub id: usize,
    pub name: String,
    pub volume: f32,
    pub pan: f32,
    pub muted: bool,
    pub solo: bool,
    pub sends: Vec<(usize, f32)>,
    pub eq_enabled: bool,
    pub eq_low: f32,
    pub eq_mid: f32,
    pub eq_high: f32,
    pub compressor_threshold: f32,
    pub compressor_ratio: f32,
}

impl AudioBus {
    pub fn new(id: usize, name: String) -> Self {
        Self {
            id,
            name,
            volume: 1.0,
            pan: 0.0,
            muted: false,
            solo: false,
            sends: Vec::new(),
            eq_enabled: false,
            eq_low: 0.0,
            eq_mid: 0.0,
            eq_high: 0.0,
            compressor_threshold: -18.0,
            compressor_ratio: 4.0,
        }
    }
}

// ── Audio mixer expansion: per-track strips, master bus, mixdown math ───────
//
// All deterministic data-model + DSP-free mixdown math (no realtime output). A
// [`MixerTrack`] is one channel strip (gain in dB, pan, solo, mute, and a bus
// routing). The [`MasterBus`] sums everything. The mixdown produces a stereo
// `(left, right)` gain pair per track and a summed master, applying the AE/Logic
// solo rule (any solo mutes every non-soloed strip) and an equal-power pan law.

/// dB → linear amplitude gain. `-inf`-ish (≤ -120 dB) maps to silence.
pub fn db_to_linear(db: f32) -> f32 {
    if db <= -120.0 {
        0.0
    } else {
        10.0_f32.powf(db / 20.0)
    }
}

/// Linear amplitude gain → dB. A non-positive gain maps to `-120` dB (floor).
pub fn linear_to_db(linear: f32) -> f32 {
    if linear <= 0.0 {
        -120.0
    } else {
        20.0 * linear.log10()
    }
}

/// Equal-power stereo pan: `pan` ∈ [-1, 1] (left..right). Returns `(l, r)` gains,
/// each in `[0, 1]`, with `l² + r² == 1` (constant perceived loudness).
pub fn pan_law(pan: f32) -> (f32, f32) {
    let p = pan.clamp(-1.0, 1.0);
    // Map [-1,1] → [0, π/2]; left = cos, right = sin.
    let angle = (p + 1.0) * 0.5 * std::f32::consts::FRAC_PI_2;
    (angle.cos(), angle.sin())
}

/// One mixer channel strip for an audio-bearing layer/track.
#[derive(Clone, Debug, PartialEq)]
pub struct MixerTrack {
    /// Source layer index this strip controls.
    pub layer_id: usize,
    /// Display name.
    pub name: String,
    /// Channel gain in **decibels** (0 dB = unity).
    pub gain_db: f32,
    /// Pan position, -1 (hard left) .. +1 (hard right).
    pub pan: f32,
    pub muted: bool,
    pub solo: bool,
    /// Destination bus id, or `None` to route straight to master.
    pub output_bus: Option<usize>,
}

impl MixerTrack {
    pub fn new(layer_id: usize, name: impl Into<String>) -> Self {
        Self {
            layer_id,
            name: name.into(),
            gain_db: 0.0,
            pan: 0.0,
            muted: false,
            solo: false,
            output_bus: None,
        }
    }

    /// Linear amplitude gain from the strip's dB fader.
    pub fn linear_gain(&self) -> f32 {
        db_to_linear(self.gain_db)
    }
}

/// The master output bus that sums the whole mix.
#[derive(Clone, Debug, PartialEq)]
pub struct MasterBus {
    pub gain_db: f32,
    pub pan: f32,
    pub muted: bool,
}

impl Default for MasterBus {
    fn default() -> Self {
        Self { gain_db: 0.0, pan: 0.0, muted: false }
    }
}

impl MasterBus {
    pub fn linear_gain(&self) -> f32 {
        db_to_linear(self.gain_db)
    }
}

/// The result of a static mixdown: per-track `(left, right)` contribution gains
/// plus the summed master `(left, right)`. Pure data — feeds a renderer/meter.
#[derive(Clone, Debug, PartialEq)]
pub struct Mixdown {
    /// `(layer_id, left_gain, right_gain)` per audible track after solo/mute.
    pub tracks: Vec<(usize, f32, f32)>,
    /// Summed master `(left, right)`, post master fader/pan.
    pub master: (f32, f32),
}

impl App {
    /// Compute a static stereo [`Mixdown`] over the current mixer tracks.
    ///
    /// Solo rule: if **any** track is soloed, only soloed (and non-muted) tracks
    /// pass. Each audible track contributes `unity × dB-gain × pan-law`, summed
    /// per channel, then scaled by the master fader/pan. Fully deterministic.
    pub fn compute_mixdown(&self) -> Mixdown {
        let any_solo = self.mixer_tracks.iter().any(|t| t.solo);
        let mut master_l = 0.0_f32;
        let mut master_r = 0.0_f32;
        let mut tracks = Vec::new();
        for t in &self.mixer_tracks {
            let audible = if any_solo { t.solo && !t.muted } else { !t.muted };
            if !audible {
                tracks.push((t.layer_id, 0.0, 0.0));
                continue;
            }
            let (pl, pr) = pan_law(t.pan);
            // Apply any output-bus volume (linear) when routed through a bus.
            let bus_gain = t
                .output_bus
                .and_then(|bid| self.audio_buses.iter().find(|b| b.id == bid))
                .map(|b| if b.muted { 0.0 } else { b.volume })
                .unwrap_or(1.0);
            let g = t.linear_gain() * bus_gain;
            let l = g * pl;
            let r = g * pr;
            tracks.push((t.layer_id, l, r));
            master_l += l;
            master_r += r;
        }
        let (mpl, mpr) = pan_law(self.master_bus.pan);
        let mg = if self.master_bus.muted { 0.0 } else { self.master_bus.linear_gain() };
        Mixdown {
            tracks,
            master: (master_l * mg * mpl * std::f32::consts::SQRT_2,
                     master_r * mg * mpr * std::f32::consts::SQRT_2),
        }
    }

    pub(super) fn apply_audio_mixer(&mut self, action: Action) {
        match action {
            Action::AddMixerTrack { layer_id, name } => {
                self.mixer_tracks.push(MixerTrack::new(layer_id, name));
            }
            Action::RemoveMixerTrack { layer_id } => {
                self.mixer_tracks.retain(|t| t.layer_id != layer_id);
            }
            Action::SetTrackGainDb { layer_id, gain_db } => {
                if let Some(t) = self.mixer_tracks.iter_mut().find(|t| t.layer_id == layer_id) {
                    t.gain_db = gain_db.clamp(-120.0, 24.0);
                }
            }
            Action::SetTrackPan { layer_id, pan } => {
                if let Some(t) = self.mixer_tracks.iter_mut().find(|t| t.layer_id == layer_id) {
                    t.pan = pan.clamp(-1.0, 1.0);
                }
            }
            Action::SetTrackMute { layer_id, muted } => {
                if let Some(t) = self.mixer_tracks.iter_mut().find(|t| t.layer_id == layer_id) {
                    t.muted = muted;
                }
            }
            Action::SetTrackSolo { layer_id, solo } => {
                if let Some(t) = self.mixer_tracks.iter_mut().find(|t| t.layer_id == layer_id) {
                    t.solo = solo;
                }
            }
            Action::SetTrackOutputBus { layer_id, bus_id } => {
                if let Some(t) = self.mixer_tracks.iter_mut().find(|t| t.layer_id == layer_id) {
                    t.output_bus = bus_id;
                }
            }
            Action::SetMasterGainDb(db) => {
                self.master_bus.gain_db = db.clamp(-120.0, 24.0);
            }
            Action::SetMasterBusPan(pan) => {
                self.master_bus.pan = pan.clamp(-1.0, 1.0);
            }
            Action::SetMasterBusMute(muted) => {
                self.master_bus.muted = muted;
            }
            _ => unreachable!("apply_audio_mixer called with wrong action"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_db_linear_round_trip() {
        for &db in &[-60.0_f32, -12.0, -6.0, 0.0, 6.0, 12.0] {
            let lin = db_to_linear(db);
            let back = linear_to_db(lin);
            assert!((back - db).abs() < 1e-3, "db={db} → {lin} → {back}");
        }
        // Known anchors.
        assert!((db_to_linear(0.0) - 1.0).abs() < 1e-5);
        assert!((db_to_linear(-6.0206) - 0.5).abs() < 1e-3);
        assert!((db_to_linear(6.0206) - 2.0).abs() < 1e-3);
        // Floor.
        assert_eq!(db_to_linear(-130.0), 0.0);
        assert_eq!(linear_to_db(0.0), -120.0);
    }

    #[test]
    fn test_pan_law_equal_power() {
        let (l, r) = pan_law(0.0);
        // Center: equal, and l²+r²==1.
        assert!((l - r).abs() < 1e-4);
        assert!((l * l + r * r - 1.0).abs() < 1e-4);
        // Hard left / right.
        let (ll, lr) = pan_law(-1.0);
        assert!((ll - 1.0).abs() < 1e-4 && lr.abs() < 1e-4);
        let (rl, rr) = pan_law(1.0);
        assert!(rl.abs() < 1e-4 && (rr - 1.0).abs() < 1e-4);
    }

    #[test]
    fn test_mixer_track_linear_gain() {
        let mut t = MixerTrack::new(0, "Music");
        assert!((t.linear_gain() - 1.0).abs() < 1e-5);
        t.gain_db = -6.0206;
        assert!((t.linear_gain() - 0.5).abs() < 1e-3);
    }

    #[test]
    fn test_add_remove_mixer_track() {
        let mut app = App::new();
        assert!(app.mixer_tracks.is_empty());
        app.apply(Action::AddMixerTrack { layer_id: 0, name: "Dialog".to_string() });
        app.apply(Action::AddMixerTrack { layer_id: 1, name: "Music".to_string() });
        assert_eq!(app.mixer_tracks.len(), 2);
        app.apply(Action::RemoveMixerTrack { layer_id: 0 });
        assert_eq!(app.mixer_tracks.len(), 1);
        assert_eq!(app.mixer_tracks[0].layer_id, 1);
    }

    #[test]
    fn test_track_gain_pan_clamp() {
        let mut app = App::new();
        app.apply(Action::AddMixerTrack { layer_id: 0, name: "T".to_string() });
        app.apply(Action::SetTrackGainDb { layer_id: 0, gain_db: 100.0 });
        assert!((app.mixer_tracks[0].gain_db - 24.0).abs() < 1e-4);
        app.apply(Action::SetTrackPan { layer_id: 0, pan: 5.0 });
        assert!((app.mixer_tracks[0].pan - 1.0).abs() < 1e-4);
    }

    #[test]
    fn test_solo_rule_mutes_others() {
        let mut app = App::new();
        app.apply(Action::AddMixerTrack { layer_id: 0, name: "A".to_string() });
        app.apply(Action::AddMixerTrack { layer_id: 1, name: "B".to_string() });
        // No solo: both audible.
        let m = app.compute_mixdown();
        assert!(m.tracks[0].1 > 0.0 && m.tracks[1].1 > 0.0);
        // Solo track 0: track 1 silenced.
        app.apply(Action::SetTrackSolo { layer_id: 0, solo: true });
        let m = app.compute_mixdown();
        assert!(m.tracks[0].1 > 0.0, "soloed track audible");
        assert_eq!((m.tracks[1].1, m.tracks[1].2), (0.0, 0.0), "non-solo muted");
    }

    #[test]
    fn test_mute_silences_track() {
        let mut app = App::new();
        app.apply(Action::AddMixerTrack { layer_id: 0, name: "A".to_string() });
        app.apply(Action::SetTrackMute { layer_id: 0, muted: true });
        let m = app.compute_mixdown();
        assert_eq!((m.tracks[0].1, m.tracks[0].2), (0.0, 0.0));
    }

    #[test]
    fn test_master_mute_silences_mix() {
        let mut app = App::new();
        app.apply(Action::AddMixerTrack { layer_id: 0, name: "A".to_string() });
        app.apply(Action::SetMasterBusMute(true));
        let m = app.compute_mixdown();
        assert_eq!(m.master, (0.0, 0.0));
    }

    #[test]
    fn test_master_gain_pan_set() {
        let mut app = App::new();
        app.apply(Action::SetMasterGainDb(-6.0));
        assert!((app.master_bus.gain_db - (-6.0)).abs() < 1e-4);
        app.apply(Action::SetMasterBusPan(0.5));
        assert!((app.master_bus.pan - 0.5).abs() < 1e-4);
    }

    #[test]
    fn test_track_routed_through_muted_bus_is_silent() {
        let mut app = App::new();
        app.apply(Action::AddAudioBus { name: "Reverb".to_string() });
        let bus_id = app.audio_buses[0].id;
        app.apply(Action::AddMixerTrack { layer_id: 0, name: "A".to_string() });
        app.apply(Action::SetTrackOutputBus { layer_id: 0, bus_id: Some(bus_id) });
        app.apply(Action::MuteBus { bus_id, muted: true });
        let m = app.compute_mixdown();
        assert_eq!((m.tracks[0].1, m.tracks[0].2), (0.0, 0.0));
    }
}
