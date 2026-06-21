//! Project / session domain — `ToneProject` type + project apply methods + tests.

// ─── Types ────────────────────────────────────────────────────────────────────

/// The top-level project settings for a Tone session.
#[derive(Clone, Debug)]
pub struct ToneProject {
    /// Human-readable project name.
    pub name: String,
    /// Beats per minute. Clamped to 20.0..=999.0 by `SetBpm`.
    pub bpm: f32,
    /// Numerator of the time signature (1..=16).
    pub time_signature_num: u8,
    /// Denominator of the time signature. Valid values: 1, 2, 4, 8, 16.
    pub time_signature_den: u8,
    /// Audio sample rate in Hz (44100, 48000, 88200, 96000).
    pub sample_rate: u32,
    /// Bit depth for audio. Valid values: 16, 24, 32.
    pub bit_depth: u8,
    /// Root key of the project ("C", "C#", "D", "Db", …).
    pub key: String,
    /// Scale name ("Major", "Minor", "Dorian", "Mixolydian", …).
    pub scale: String,
}

impl ToneProject {
    /// Sensible defaults: 120 BPM, 4/4, 44100 Hz, 24-bit, C Major.
    pub fn new() -> Self {
        Self {
            name: "Untitled Project".to_string(),
            bpm: 120.0,
            time_signature_num: 4,
            time_signature_den: 4,
            sample_rate: 44100,
            bit_depth: 24,
            key: "C".to_string(),
            scale: "Major".to_string(),
        }
    }
}

impl Default for ToneProject {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Apply methods ────────────────────────────────────────────────────────────

use super::{Action, App};

impl App {
    pub(super) fn apply_project(&mut self, action: Action) {
        match action {
            Action::SetBpm(bpm) => {
                self.project.bpm = bpm.clamp(20.0, 999.0);
            }
            Action::SetTimeSignature { num, den } => {
                if num >= 1 && num <= 16 && [1u8, 2, 4, 8, 16].contains(&den) {
                    self.project.time_signature_num = num;
                    self.project.time_signature_den = den;
                }
            }
            Action::SetSampleRate(sr) => {
                if [44100u32, 48000, 88200, 96000].contains(&sr) {
                    self.project.sample_rate = sr;
                }
            }
            Action::SetBitDepth(bd) => {
                if [16u8, 24, 32].contains(&bd) {
                    self.project.bit_depth = bd;
                }
            }
            Action::SetProjectKey(key) => {
                self.project.key = key;
            }
            Action::SetProjectScale(scale) => {
                self.project.scale = scale;
            }
            Action::SetProjectName(name) => {
                self.project.name = name;
            }
            _ => {}
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::{Action, App};

    fn fresh() -> App {
        App::new()
    }

    #[test]
    fn project_defaults() {
        let app = fresh();
        assert_eq!(app.project.bpm, 120.0);
        assert_eq!(app.project.time_signature_num, 4);
        assert_eq!(app.project.time_signature_den, 4);
        assert_eq!(app.project.sample_rate, 44100);
        assert_eq!(app.project.bit_depth, 24);
        assert_eq!(app.project.key, "C");
        assert_eq!(app.project.scale, "Major");
    }

    #[test]
    fn set_bpm_normal() {
        let mut app = fresh();
        app.apply(Action::SetBpm(140.0));
        assert_eq!(app.project.bpm, 140.0);
    }

    #[test]
    fn set_bpm_clamp_low() {
        let mut app = fresh();
        app.apply(Action::SetBpm(0.0));
        assert_eq!(app.project.bpm, 20.0);
    }

    #[test]
    fn set_bpm_clamp_high() {
        let mut app = fresh();
        app.apply(Action::SetBpm(9999.0));
        assert_eq!(app.project.bpm, 999.0);
    }

    #[test]
    fn set_time_signature_valid() {
        let mut app = fresh();
        app.apply(Action::SetTimeSignature { num: 3, den: 4 });
        assert_eq!(app.project.time_signature_num, 3);
        assert_eq!(app.project.time_signature_den, 4);
    }

    #[test]
    fn set_time_signature_invalid_den_ignored() {
        let mut app = fresh();
        app.apply(Action::SetTimeSignature { num: 4, den: 3 });
        assert_eq!(app.project.time_signature_den, 4);
    }

    #[test]
    fn set_time_signature_invalid_num_ignored() {
        let mut app = fresh();
        app.apply(Action::SetTimeSignature { num: 0, den: 4 });
        assert_eq!(app.project.time_signature_num, 4);
    }

    #[test]
    fn set_time_signature_max_num() {
        let mut app = fresh();
        app.apply(Action::SetTimeSignature { num: 16, den: 8 });
        assert_eq!(app.project.time_signature_num, 16);
        assert_eq!(app.project.time_signature_den, 8);
    }

    #[test]
    fn set_sample_rate_valid() {
        let mut app = fresh();
        app.apply(Action::SetSampleRate(48000));
        assert_eq!(app.project.sample_rate, 48000);
    }

    #[test]
    fn set_sample_rate_88200() {
        let mut app = fresh();
        app.apply(Action::SetSampleRate(88200));
        assert_eq!(app.project.sample_rate, 88200);
    }

    #[test]
    fn set_sample_rate_invalid_ignored() {
        let mut app = fresh();
        app.apply(Action::SetSampleRate(22050));
        assert_eq!(app.project.sample_rate, 44100);
    }

    #[test]
    fn set_bit_depth_valid() {
        let mut app = fresh();
        app.apply(Action::SetBitDepth(16));
        assert_eq!(app.project.bit_depth, 16);
    }

    #[test]
    fn set_bit_depth_32() {
        let mut app = fresh();
        app.apply(Action::SetBitDepth(32));
        assert_eq!(app.project.bit_depth, 32);
    }

    #[test]
    fn set_bit_depth_invalid_ignored() {
        let mut app = fresh();
        app.apply(Action::SetBitDepth(20));
        assert_eq!(app.project.bit_depth, 24);
    }

    #[test]
    fn set_project_key() {
        let mut app = fresh();
        app.apply(Action::SetProjectKey("F#".to_string()));
        assert_eq!(app.project.key, "F#");
    }

    #[test]
    fn set_project_key_flat() {
        let mut app = fresh();
        app.apply(Action::SetProjectKey("Bb".to_string()));
        assert_eq!(app.project.key, "Bb");
    }

    #[test]
    fn set_project_scale() {
        let mut app = fresh();
        app.apply(Action::SetProjectScale("Dorian".to_string()));
        assert_eq!(app.project.scale, "Dorian");
    }

    #[test]
    fn set_project_name() {
        let mut app = fresh();
        app.apply(Action::SetProjectName("My Track".to_string()));
        assert_eq!(app.project.name, "My Track");
    }

    #[test]
    fn project_name_default() {
        let app = fresh();
        assert_eq!(app.project.name, "Untitled Project");
    }

    #[test]
    fn set_bpm_boundary_exactly_20() {
        let mut app = fresh();
        app.apply(Action::SetBpm(20.0));
        assert_eq!(app.project.bpm, 20.0);
    }

    #[test]
    fn set_bpm_boundary_exactly_999() {
        let mut app = fresh();
        app.apply(Action::SetBpm(999.0));
        assert_eq!(app.project.bpm, 999.0);
    }

    #[test]
    fn set_sample_rate_96000() {
        let mut app = fresh();
        app.apply(Action::SetSampleRate(96000));
        assert_eq!(app.project.sample_rate, 96000);
    }

    #[test]
    fn set_project_key_empty_allowed() {
        let mut app = fresh();
        app.apply(Action::SetProjectKey("".to_string()));
        assert_eq!(app.project.key, "");
    }
}
