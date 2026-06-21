//! Export / Bounce domain — `BounceFormat`, `BounceConfig` types + export
//! apply methods + tests.

// ─── Types ────────────────────────────────────────────────────────────────────

/// Audio file format for the bounce output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BounceFormat {
    Wav,
    Mp3,
    Flac,
    Ogg,
    /// Export each track as a separate file.
    Stems,
}

impl BounceFormat {
    pub fn label(self) -> &'static str {
        match self {
            BounceFormat::Wav => "WAV",
            BounceFormat::Mp3 => "MP3",
            BounceFormat::Flac => "FLAC",
            BounceFormat::Ogg => "OGG",
            BounceFormat::Stems => "Stems",
        }
    }
}

/// Configuration for the current bounce / export run.
#[derive(Clone, Debug)]
pub struct BounceConfig {
    pub format: BounceFormat,
    pub sample_rate: u32,
    pub bit_depth: u8,
    /// Normalise the output to 0 dBFS.
    pub normalize: bool,
    /// Apply noise-shaped dither before truncating to the target bit depth.
    pub dither: bool,
    /// Absolute path where the output file(s) will be written.
    pub export_path: String,
    /// Apply the master FX chain to the bounce.
    pub include_master_fx: bool,
    /// When true (and format = Stems), one file per track is written.
    pub stems_per_track: bool,
}

impl BounceConfig {
    /// Sensible defaults: WAV, 44100 Hz, 24-bit, normalised.
    pub fn new() -> Self {
        Self {
            format: BounceFormat::Wav,
            sample_rate: 44100,
            bit_depth: 24,
            normalize: true,
            dither: false,
            export_path: String::new(),
            include_master_fx: true,
            stems_per_track: false,
        }
    }
}

impl Default for BounceConfig {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Apply methods ────────────────────────────────────────────────────────────

use super::{Action, App};

impl App {
    pub(super) fn apply_export(&mut self, action: Action) {
        match action {
            Action::SetBounceFormat(fmt) => {
                self.bounce_config.format = fmt;
            }
            Action::SetBounceNormalize(v) => {
                self.bounce_config.normalize = v;
            }
            Action::SetBounceDither(v) => {
                self.bounce_config.dither = v;
            }
            Action::SetBounceExportPath(path) => {
                self.bounce_config.export_path = path;
            }
            Action::SetBounceStemsPerTrack(v) => {
                self.bounce_config.stems_per_track = v;
            }
            Action::StartBounce => {
                self.bounce_in_progress = true;
            }
            Action::CancelBounce => {
                self.bounce_in_progress = false;
            }
            _ => {}
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::{Action, App};
    use super::{BounceConfig, BounceFormat};

    fn fresh() -> App {
        App::new()
    }

    #[test]
    fn bounce_config_defaults() {
        let cfg = BounceConfig::new();
        assert_eq!(cfg.format, BounceFormat::Wav);
        assert_eq!(cfg.sample_rate, 44100);
        assert_eq!(cfg.bit_depth, 24);
        assert!(cfg.normalize);
    }

    #[test]
    fn set_bounce_format() {
        let mut app = fresh();
        app.apply(Action::SetBounceFormat(BounceFormat::Flac));
        assert_eq!(app.bounce_config.format, BounceFormat::Flac);
    }

    #[test]
    fn set_bounce_format_ogg() {
        let mut app = fresh();
        app.apply(Action::SetBounceFormat(BounceFormat::Ogg));
        assert_eq!(app.bounce_config.format, BounceFormat::Ogg);
    }

    #[test]
    fn set_bounce_format_mp3() {
        let mut app = fresh();
        app.apply(Action::SetBounceFormat(BounceFormat::Mp3));
        assert_eq!(app.bounce_config.format, BounceFormat::Mp3);
    }

    #[test]
    fn set_bounce_normalize() {
        let mut app = fresh();
        app.apply(Action::SetBounceNormalize(false));
        assert!(!app.bounce_config.normalize);
    }

    #[test]
    fn set_bounce_dither() {
        let mut app = fresh();
        app.apply(Action::SetBounceDither(true));
        assert!(app.bounce_config.dither);
    }

    #[test]
    fn set_bounce_export_path() {
        let mut app = fresh();
        app.apply(Action::SetBounceExportPath("/tmp/out.wav".to_string()));
        assert_eq!(app.bounce_config.export_path, "/tmp/out.wav");
    }

    #[test]
    fn set_bounce_stems_per_track() {
        let mut app = fresh();
        app.apply(Action::SetBounceStemsPerTrack(true));
        assert!(app.bounce_config.stems_per_track);
    }

    #[test]
    fn start_bounce() {
        let mut app = fresh();
        assert!(!app.bounce_in_progress);
        app.apply(Action::StartBounce);
        assert!(app.bounce_in_progress);
    }

    #[test]
    fn cancel_bounce() {
        let mut app = fresh();
        app.apply(Action::StartBounce);
        app.apply(Action::CancelBounce);
        assert!(!app.bounce_in_progress);
    }

    #[test]
    fn bounce_format_label() {
        assert_eq!(BounceFormat::Wav.label(), "WAV");
        assert_eq!(BounceFormat::Mp3.label(), "MP3");
        assert_eq!(BounceFormat::Flac.label(), "FLAC");
        assert_eq!(BounceFormat::Ogg.label(), "OGG");
        assert_eq!(BounceFormat::Stems.label(), "Stems");
    }

    #[test]
    fn bounce_stems_per_track_with_format() {
        let mut app = fresh();
        app.apply(Action::SetBounceFormat(BounceFormat::Stems));
        app.apply(Action::SetBounceStemsPerTrack(true));
        assert_eq!(app.bounce_config.format, BounceFormat::Stems);
        assert!(app.bounce_config.stems_per_track);
    }

    #[test]
    fn bounce_config_default_normalize_true() {
        let app = fresh();
        assert!(app.bounce_config.normalize);
    }

    #[test]
    fn bounce_config_default_dither_false() {
        let app = fresh();
        assert!(!app.bounce_config.dither);
    }

    #[test]
    fn bounce_config_default_include_master_fx() {
        let app = fresh();
        assert!(app.bounce_config.include_master_fx);
    }

    #[test]
    fn bounce_config_default_stems_per_track_false() {
        let app = fresh();
        assert!(!app.bounce_config.stems_per_track);
    }

    #[test]
    fn bounce_config_default_export_path_empty() {
        let app = fresh();
        assert!(app.bounce_config.export_path.is_empty());
    }

    #[test]
    fn multiple_start_cancels_idempotent() {
        let mut app = fresh();
        app.apply(Action::StartBounce);
        app.apply(Action::StartBounce);
        assert!(app.bounce_in_progress);
    }

    #[test]
    fn cancel_without_start_is_noop() {
        let mut app = fresh();
        assert!(!app.bounce_in_progress);
        app.apply(Action::CancelBounce);
        assert!(!app.bounce_in_progress);
    }

    #[test]
    fn set_bounce_normalize_toggle() {
        let mut app = fresh();
        assert!(app.bounce_config.normalize); // default true
        app.apply(Action::SetBounceNormalize(false));
        assert!(!app.bounce_config.normalize);
        app.apply(Action::SetBounceNormalize(true));
        assert!(app.bounce_config.normalize);
    }

    #[test]
    fn set_bounce_dither_toggle() {
        let mut app = fresh();
        assert!(!app.bounce_config.dither);
        app.apply(Action::SetBounceDither(true));
        assert!(app.bounce_config.dither);
        app.apply(Action::SetBounceDither(false));
        assert!(!app.bounce_config.dither);
    }

    #[test]
    fn bounce_format_wav_is_default() {
        let app = fresh();
        assert_eq!(app.bounce_config.format, BounceFormat::Wav);
    }

    #[test]
    fn set_bounce_format_then_change() {
        let mut app = fresh();
        app.apply(Action::SetBounceFormat(BounceFormat::Flac));
        assert_eq!(app.bounce_config.format, BounceFormat::Flac);
        app.apply(Action::SetBounceFormat(BounceFormat::Wav));
        assert_eq!(app.bounce_config.format, BounceFormat::Wav);
    }

    #[test]
    fn export_path_overwritten() {
        let mut app = fresh();
        app.apply(Action::SetBounceExportPath("/tmp/v1.wav".to_string()));
        app.apply(Action::SetBounceExportPath("/tmp/v2.wav".to_string()));
        assert_eq!(app.bounce_config.export_path, "/tmp/v2.wav");
    }
}
