//! **Render output formats** — an extended output-format model (ProRes, H.264,
//! H.265, PNG sequence, EXR sequence) plus a pure **ffmpeg-arg generator**.
//!
//! [`RenderCodec`] enumerates the deliverable codecs; [`RenderSpec`] bundles a
//! codec with the comp dimensions / frame rate / quality knobs. [`ffmpeg_args`]
//! turns a spec + input/output paths into the exact `ffmpeg` argument vector —
//! a **pure function** that never executes anything (so tests stay hermetic).
//!
//! This is app-side, parallel to (and distinct from) `render.rs`'s
//! `RenderFormat` / `RenderOutputFormat`; it does not touch the engine. The
//! `App` impl holds a current [`RenderSpec`] and the panel actions.

use std::path::Path;

use super::{App, Action};

/// A deliverable codec / container.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderCodec {
    /// Apple ProRes (variant chosen by [`ProResProfile`]) in a `.mov`.
    ProRes(ProResProfile),
    /// H.264 (libx264) in an `.mp4`.
    H264,
    /// H.265 / HEVC (libx265) in an `.mp4`.
    H265,
    /// A numbered PNG image sequence.
    PngSequence,
    /// A numbered OpenEXR image sequence (linear float).
    ExrSequence,
}

/// ProRes profile (maps to ffmpeg's `-profile:v` index for `prores_ks`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProResProfile {
    /// Proxy (≈45 Mbps) — `-profile:v 0`.
    Proxy,
    /// LT — `-profile:v 1`.
    Lt,
    /// 422 (Standard) — `-profile:v 2`.
    Standard,
    /// 422 HQ — `-profile:v 3`.
    Hq,
    /// 4444 (with alpha) — `-profile:v 4`.
    P4444,
}

impl ProResProfile {
    /// The ffmpeg `prores_ks` profile index.
    pub fn ffmpeg_index(self) -> u8 {
        match self {
            ProResProfile::Proxy => 0,
            ProResProfile::Lt => 1,
            ProResProfile::Standard => 2,
            ProResProfile::Hq => 3,
            ProResProfile::P4444 => 4,
        }
    }
    /// Whether this profile carries an alpha channel.
    pub fn has_alpha(self) -> bool {
        matches!(self, ProResProfile::P4444)
    }
}

impl RenderCodec {
    /// The output file extension (no dot). Sequences use the per-frame ext.
    pub fn extension(self) -> &'static str {
        match self {
            RenderCodec::ProRes(_) => "mov",
            RenderCodec::H264 | RenderCodec::H265 => "mp4",
            RenderCodec::PngSequence => "png",
            RenderCodec::ExrSequence => "exr",
        }
    }

    /// Whether this codec writes a per-frame image sequence (vs. a single movie).
    pub fn is_sequence(self) -> bool {
        matches!(self, RenderCodec::PngSequence | RenderCodec::ExrSequence)
    }

    /// Human-readable name for the panel.
    pub fn label(self) -> &'static str {
        match self {
            RenderCodec::ProRes(ProResProfile::Proxy) => "ProRes 422 Proxy",
            RenderCodec::ProRes(ProResProfile::Lt) => "ProRes 422 LT",
            RenderCodec::ProRes(ProResProfile::Standard) => "ProRes 422",
            RenderCodec::ProRes(ProResProfile::Hq) => "ProRes 422 HQ",
            RenderCodec::ProRes(ProResProfile::P4444) => "ProRes 4444",
            RenderCodec::H264 => "H.264 (MP4)",
            RenderCodec::H265 => "H.265 / HEVC (MP4)",
            RenderCodec::PngSequence => "PNG Sequence",
            RenderCodec::ExrSequence => "EXR Sequence",
        }
    }
}

/// A full render specification: codec + output geometry + quality knobs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderSpec {
    pub codec: RenderCodec,
    /// Output frame rate (fps).
    pub fps: f32,
    /// x264/x265 CRF quality (lower = better; ignored for ProRes / sequences).
    pub crf: u8,
    /// Whether to mux audio (movie codecs only).
    pub audio: bool,
}

impl Default for RenderSpec {
    fn default() -> Self {
        Self {
            codec: RenderCodec::H264,
            fps: 30.0,
            crf: 18,
            audio: true,
        }
    }
}

/// Build the **ffmpeg argument vector** for a render: read frames from `input`
/// (a single file, or a printf-style sequence pattern like `frame_%05d.png`),
/// and write to `output`. The args are the exact tokens ffmpeg expects after the
/// `ffmpeg` program name. **Pure** — this never runs ffmpeg.
///
/// For a frame-sequence *output* (`PngSequence` / `ExrSequence`) the encoder is
/// the per-frame image codec and audio is dropped. For movie codecs the encoder
/// is the corresponding video codec, with CRF (x264/x265) or ProRes profile.
pub fn ffmpeg_args(spec: RenderSpec, input: &Path, output: &Path) -> Vec<String> {
    let mut a: Vec<String> = Vec::new();
    a.push("-y".into()); // overwrite

    // Input: rate + (sequence reads use the same -framerate before -i).
    a.push("-framerate".into());
    a.push(fmt_fps(spec.fps));
    a.push("-i".into());
    a.push(input.to_string_lossy().into_owned());

    match spec.codec {
        RenderCodec::ProRes(profile) => {
            a.push("-c:v".into());
            a.push("prores_ks".into());
            a.push("-profile:v".into());
            a.push(profile.ffmpeg_index().to_string());
            // 4444 carries alpha (yuva444p10le); others use yuv422p10le.
            a.push("-pix_fmt".into());
            a.push(if profile.has_alpha() { "yuva444p10le" } else { "yuv422p10le" }.into());
            push_audio(&mut a, spec.audio);
        }
        RenderCodec::H264 => {
            a.push("-c:v".into());
            a.push("libx264".into());
            a.push("-crf".into());
            a.push(spec.crf.to_string());
            a.push("-pix_fmt".into());
            a.push("yuv420p".into());
            push_audio(&mut a, spec.audio);
        }
        RenderCodec::H265 => {
            a.push("-c:v".into());
            a.push("libx265".into());
            a.push("-crf".into());
            a.push(spec.crf.to_string());
            a.push("-pix_fmt".into());
            a.push("yuv420p".into());
            push_audio(&mut a, spec.audio);
        }
        RenderCodec::PngSequence => {
            a.push("-c:v".into());
            a.push("png".into());
            // Sequences never carry audio.
            a.push("-an".into());
        }
        RenderCodec::ExrSequence => {
            a.push("-c:v".into());
            a.push("exr".into());
            a.push("-pix_fmt".into());
            a.push("rgba16f".into());
            a.push("-an".into());
        }
    }

    a.push(output.to_string_lossy().into_owned());
    a
}

/// Append audio args: AAC for movies when `audio`, else drop audio (`-an`).
fn push_audio(a: &mut Vec<String>, audio: bool) {
    if audio {
        a.push("-c:a".into());
        a.push("aac".into());
        a.push("-b:a".into());
        a.push("192k".into());
    } else {
        a.push("-an".into());
    }
}

/// Format an fps without a trailing `.0` (ffmpeg accepts both, but integers read
/// cleaner): `30.0` → `"30"`, `29.97` → `"29.97"`.
fn fmt_fps(fps: f32) -> String {
    if (fps.fract()).abs() < 1e-4 {
        format!("{}", fps.round() as i64)
    } else {
        // Trim to 3 decimals.
        let s = format!("{fps:.3}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

impl App {
    /// Apply a render-format [`Action`]. Dispatched from [`App::apply`].
    pub(super) fn apply_render_formats(&mut self, action: Action) {
        match action {
            Action::SetRenderCodec(codec) => {
                self.render_spec.codec = codec;
            }
            Action::SetRenderFps(fps) => {
                self.render_spec.fps = fps.clamp(1.0, 240.0);
            }
            Action::SetRenderCrf(crf) => {
                self.render_spec.crf = crf.min(51);
            }
            Action::SetRenderAudio(on) => {
                self.render_spec.audio = on;
            }
            _ => unreachable!("apply_render_formats called with wrong action"),
        }
    }

    /// The ffmpeg args for the current [`RenderSpec`] given input/output paths.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn render_ffmpeg_args(&self, input: &Path, output: &Path) -> Vec<String> {
        ffmpeg_args(self.render_spec, input, output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn args_for(spec: RenderSpec) -> Vec<String> {
        ffmpeg_args(spec, &PathBuf::from("in.mov"), &PathBuf::from("out.mov"))
    }

    /// Assert that `flag` is immediately followed by `value` in the arg vector.
    fn flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
        args.iter().position(|s| s == flag).and_then(|i| args.get(i + 1)).map(|s| s.as_str())
    }

    #[test]
    fn prores_args_are_correct() {
        let spec = RenderSpec {
            codec: RenderCodec::ProRes(ProResProfile::Hq),
            fps: 24.0,
            crf: 18,
            audio: true,
        };
        let args = args_for(spec);
        assert_eq!(flag_value(&args, "-c:v"), Some("prores_ks"), "ProRes uses prores_ks");
        assert_eq!(flag_value(&args, "-profile:v"), Some("3"), "HQ is profile 3");
        assert_eq!(flag_value(&args, "-pix_fmt"), Some("yuv422p10le"));
        assert_eq!(flag_value(&args, "-framerate"), Some("24"));
        assert_eq!(flag_value(&args, "-c:a"), Some("aac"), "audio muxed");
        assert_eq!(args.last().map(|s| s.as_str()), Some("out.mov"));
        assert_eq!(args[0], "-y");
    }

    #[test]
    fn prores_4444_carries_alpha() {
        let spec = RenderSpec {
            codec: RenderCodec::ProRes(ProResProfile::P4444),
            ..Default::default()
        };
        let args = args_for(spec);
        assert_eq!(flag_value(&args, "-profile:v"), Some("4"));
        assert_eq!(flag_value(&args, "-pix_fmt"), Some("yuva444p10le"), "4444 has alpha");
    }

    #[test]
    fn h264_uses_libx264_and_crf() {
        let spec = RenderSpec { codec: RenderCodec::H264, crf: 20, fps: 30.0, audio: false };
        let args = args_for(spec);
        assert_eq!(flag_value(&args, "-c:v"), Some("libx264"));
        assert_eq!(flag_value(&args, "-crf"), Some("20"));
        assert_eq!(flag_value(&args, "-pix_fmt"), Some("yuv420p"));
        assert!(args.iter().any(|s| s == "-an"), "no-audio passes -an");
        assert!(!args.iter().any(|s| s == "-c:a"), "no audio codec when disabled");
    }

    #[test]
    fn h265_uses_libx265() {
        let spec = RenderSpec { codec: RenderCodec::H265, ..Default::default() };
        let args = args_for(spec);
        assert_eq!(flag_value(&args, "-c:v"), Some("libx265"));
    }

    #[test]
    fn png_sequence_drops_audio() {
        let spec = RenderSpec { codec: RenderCodec::PngSequence, ..Default::default() };
        let args = ffmpeg_args(spec, &PathBuf::from("in.mov"), &PathBuf::from("frame_%05d.png"));
        assert_eq!(flag_value(&args, "-c:v"), Some("png"));
        assert!(args.iter().any(|s| s == "-an"), "sequences carry no audio");
        assert_eq!(args.last().map(|s| s.as_str()), Some("frame_%05d.png"));
    }

    #[test]
    fn exr_sequence_is_float() {
        let spec = RenderSpec { codec: RenderCodec::ExrSequence, ..Default::default() };
        let args = ffmpeg_args(spec, &PathBuf::from("in.mov"), &PathBuf::from("frame_%05d.exr"));
        assert_eq!(flag_value(&args, "-c:v"), Some("exr"));
        assert_eq!(flag_value(&args, "-pix_fmt"), Some("rgba16f"), "EXR is linear float");
    }

    #[test]
    fn codec_metadata() {
        assert_eq!(RenderCodec::H264.extension(), "mp4");
        assert_eq!(RenderCodec::ProRes(ProResProfile::Standard).extension(), "mov");
        assert_eq!(RenderCodec::PngSequence.extension(), "png");
        assert!(RenderCodec::ExrSequence.is_sequence());
        assert!(!RenderCodec::H265.is_sequence());
        assert_eq!(ProResProfile::Proxy.ffmpeg_index(), 0);
        assert!(ProResProfile::P4444.has_alpha());
        assert!(!ProResProfile::Hq.has_alpha());
    }

    #[test]
    fn fractional_fps_formats() {
        let spec = RenderSpec { fps: 29.97, ..Default::default() };
        let args = args_for(spec);
        assert_eq!(flag_value(&args, "-framerate"), Some("29.97"));
    }

    #[test]
    fn render_format_actions() {
        let mut app = App::new();
        app.apply(Action::SetRenderCodec(RenderCodec::ProRes(ProResProfile::P4444)));
        assert_eq!(app.render_spec.codec, RenderCodec::ProRes(ProResProfile::P4444));
        app.apply(Action::SetRenderFps(0.0));
        assert!(app.render_spec.fps >= 1.0, "fps clamps to >= 1");
        app.apply(Action::SetRenderCrf(99));
        assert_eq!(app.render_spec.crf, 51, "crf clamps to <= 51");
        app.apply(Action::SetRenderAudio(false));
        assert!(!app.render_spec.audio);
        let args = app.render_ffmpeg_args(&PathBuf::from("i.mov"), &PathBuf::from("o.mov"));
        assert_eq!(args.first().map(|s| s.as_str()), Some("-y"));
    }
}
