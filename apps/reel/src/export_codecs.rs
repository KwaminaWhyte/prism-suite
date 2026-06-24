//! **Export codec matrix** — model the full set of container × video-codec ×
//! audio-codec × bit-depth × pixel-format × rate-control combinations Reel can
//! export, and generate the exact **ffmpeg argument list** for each combo.
//!
//! This is a *pure, deterministic* model: [`ExportSpec::ffmpeg_args`] builds the
//! `Vec<String>` you would pass to `ffmpeg` (after the input args) — it never
//! spawns a process. That keeps the whole matrix unit-testable without ffmpeg on
//! the box. Hardware-encode and bit-depth are *settings only*; this module does
//! not execute any encode.
//!
//! Argument order matches ffmpeg's expectation: video codec + its codec-specific
//! options, pixel format, rate control (CBR/VBR via `-b:v` / `-crf`/`-qscale`),
//! then the audio codec + bitrate/sample-fmt, then container-level flags.

/// The output container / muxer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Container {
    Mp4,
    Mov,
    Mkv,
    WebM,
    /// PNG image sequence (one file per frame).
    PngSeq,
    /// OpenEXR image sequence (one file per frame).
    ExrSeq,
}

impl Container {
    /// The ffmpeg `-f` muxer name, or `None` for image sequences (ffmpeg infers
    /// the muxer from the `%0Nd` output pattern's extension).
    pub fn muxer(self) -> Option<&'static str> {
        match self {
            Container::Mp4 => Some("mp4"),
            Container::Mov => Some("mov"),
            Container::Mkv => Some("matroska"),
            Container::WebM => Some("webm"),
            Container::PngSeq => Some("image2"),
            Container::ExrSeq => Some("image2"),
        }
    }

    /// The file extension (no dot) for this container's output.
    pub fn extension(self) -> &'static str {
        match self {
            Container::Mp4 => "mp4",
            Container::Mov => "mov",
            Container::Mkv => "mkv",
            Container::WebM => "webm",
            Container::PngSeq => "png",
            Container::ExrSeq => "exr",
        }
    }

    /// True for the per-frame image-sequence containers (no audio track).
    pub fn is_image_sequence(self) -> bool {
        matches!(self, Container::PngSeq | Container::ExrSeq)
    }
}

/// The video codec.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VideoCodec {
    H264,
    H265,
    /// ProRes 422 family (Proxy / LT / standard / HQ chosen by [`ProResProfile`]).
    ProRes422(ProResProfile),
    /// ProRes 4444 (with optional alpha).
    ProRes4444,
    Vp9,
    Av1,
    /// Avid DNxHR (profile chosen by [`DnxhrProfile`]).
    Dnxhr(DnxhrProfile),
    /// PNG frames (image sequence).
    Png,
    /// OpenEXR frames (image sequence).
    Exr,
}

/// ProRes 422 sub-profile (maps to ffmpeg `-profile:v 0..3`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProResProfile { Proxy, Lt, Standard, Hq }

impl ProResProfile {
    fn profile_index(self) -> &'static str {
        match self {
            ProResProfile::Proxy => "0",
            ProResProfile::Lt => "1",
            ProResProfile::Standard => "2",
            ProResProfile::Hq => "3",
        }
    }
}

/// DNxHR sub-profile (maps to ffmpeg `-profile:v`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DnxhrProfile { Lb, Sq, Hq, Hqx, Rgb444 }

impl DnxhrProfile {
    fn profile_name(self) -> &'static str {
        match self {
            DnxhrProfile::Lb => "dnxhr_lb",
            DnxhrProfile::Sq => "dnxhr_sq",
            DnxhrProfile::Hq => "dnxhr_hq",
            DnxhrProfile::Hqx => "dnxhr_hqx",
            DnxhrProfile::Rgb444 => "dnxhr_444",
        }
    }
}

impl VideoCodec {
    /// The ffmpeg `-c:v` encoder name.
    pub fn encoder(self) -> &'static str {
        match self {
            VideoCodec::H264 => "libx264",
            VideoCodec::H265 => "libx265",
            VideoCodec::ProRes422(_) | VideoCodec::ProRes4444 => "prores_ks",
            VideoCodec::Vp9 => "libvpx-vp9",
            VideoCodec::Av1 => "libaom-av1",
            VideoCodec::Dnxhr(_) => "dnxhd",
            VideoCodec::Png => "png",
            VideoCodec::Exr => "exr",
        }
    }
}

/// The audio codec.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioCodec {
    Aac,
    /// Uncompressed PCM (16-bit signed little-endian).
    Pcm,
    Flac,
    /// No audio track.
    None,
}

impl AudioCodec {
    /// The ffmpeg `-c:a` encoder name, or `None` for [`AudioCodec::None`].
    pub fn encoder(self) -> Option<&'static str> {
        match self {
            AudioCodec::Aac => Some("aac"),
            AudioCodec::Pcm => Some("pcm_s16le"),
            AudioCodec::Flac => Some("flac"),
            AudioCodec::None => None,
        }
    }
}

/// Working bit depth of the encoded video. Models the full matrix; not every
/// depth is exercised by the default presets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum BitDepth { Eight, Ten, Twelve, Sixteen }

impl BitDepth {
    pub fn bits(self) -> u32 {
        match self {
            BitDepth::Eight => 8,
            BitDepth::Ten => 10,
            BitDepth::Twelve => 12,
            BitDepth::Sixteen => 16,
        }
    }
}

/// The encoded chroma + bit-depth pixel format (`-pix_fmt`). Models the full
/// matrix; not every format is used by the default presets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum PixelFormat {
    Yuv420p,
    Yuv422p,
    Yuv444p,
    Yuv420p10le,
    Yuv422p10le,
    Yuv444p10le,
    /// ProRes 4444 with alpha.
    Yuva444p10le,
    /// 16-bit float RGBA (EXR).
    Rgba16f,
    /// 8-bit RGBA (PNG).
    Rgba,
}

impl PixelFormat {
    pub fn ffmpeg_name(self) -> &'static str {
        match self {
            PixelFormat::Yuv420p => "yuv420p",
            PixelFormat::Yuv422p => "yuv422p",
            PixelFormat::Yuv444p => "yuv444p",
            PixelFormat::Yuv420p10le => "yuv420p10le",
            PixelFormat::Yuv422p10le => "yuv422p10le",
            PixelFormat::Yuv444p10le => "yuv444p10le",
            PixelFormat::Yuva444p10le => "yuva444p10le",
            PixelFormat::Rgba16f => "gbrapf32le",
            PixelFormat::Rgba => "rgba",
        }
    }
}

/// Rate-control strategy for lossy codecs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RateControl {
    /// Constant bitrate at `kbps` (`-b:v K -minrate -maxrate -bufsize`).
    Cbr { kbps: u32 },
    /// Variable bitrate at a target `kbps` (`-b:v`).
    Vbr { kbps: u32 },
    /// Constant-quality (`-crf` for x264/x265/vp9/av1; `-qscale:v` for prores).
    Quality { crf: u32 },
}

/// One fully-specified export configuration.
#[derive(Clone, Debug)]
pub struct ExportSpec {
    pub container: Container,
    pub video: VideoCodec,
    pub audio: AudioCodec,
    pub bit_depth: BitDepth,
    pub pixel_format: PixelFormat,
    pub rate_control: RateControl,
    /// Audio bitrate in kbps (used for lossy audio codecs; ignored for PCM/FLAC).
    pub audio_kbps: u32,
    /// Audio sample rate (Hz).
    pub audio_sample_rate: u32,
    /// Audio channel count.
    pub audio_channels: u32,
    /// Two-pass video encode (adds `-pass`).
    pub two_pass: bool,
}

impl ExportSpec {
    /// A sensible H.264 / AAC 1080p MP4 default.
    pub fn h264_mp4() -> Self {
        Self {
            container: Container::Mp4,
            video: VideoCodec::H264,
            audio: AudioCodec::Aac,
            bit_depth: BitDepth::Eight,
            pixel_format: PixelFormat::Yuv420p,
            rate_control: RateControl::Vbr { kbps: 12_000 },
            audio_kbps: 256,
            audio_sample_rate: 48_000,
            audio_channels: 2,
            two_pass: false,
        }
    }

    /// A ProRes 422 HQ / PCM MOV default (mastering).
    pub fn prores_mov() -> Self {
        Self {
            container: Container::Mov,
            video: VideoCodec::ProRes422(ProResProfile::Hq),
            audio: AudioCodec::Pcm,
            bit_depth: BitDepth::Ten,
            pixel_format: PixelFormat::Yuv422p10le,
            rate_control: RateControl::Quality { crf: 11 },
            audio_kbps: 0,
            audio_sample_rate: 48_000,
            audio_channels: 2,
            two_pass: false,
        }
    }

    /// True when this spec encodes a per-frame image sequence (no audio).
    pub fn is_image_sequence(&self) -> bool {
        self.container.is_image_sequence()
    }

    /// Build the ordered ffmpeg argument list for this spec (the part **after**
    /// the input flags, ending with the implicit output path the caller appends).
    /// Pure — no I/O.
    pub fn ffmpeg_args(&self) -> Vec<String> {
        let mut a: Vec<String> = Vec::new();

        // --- Video codec --------------------------------------------------
        a.push("-c:v".into());
        a.push(self.video.encoder().into());

        // Codec-specific profile flags.
        match self.video {
            VideoCodec::ProRes422(p) => {
                a.push("-profile:v".into());
                a.push(p.profile_index().into());
            }
            VideoCodec::ProRes4444 => {
                a.push("-profile:v".into());
                // 4 = 4444, 5 = 4444 XQ; use 4444 here.
                a.push("4".into());
            }
            VideoCodec::Dnxhr(p) => {
                a.push("-profile:v".into());
                a.push(p.profile_name().into());
            }
            VideoCodec::Vp9 | VideoCodec::Av1 => {
                // Enable row-multithreading for the SVT/aom/vpx encoders.
                a.push("-row-mt".into());
                a.push("1".into());
            }
            _ => {}
        }

        // --- Pixel format -------------------------------------------------
        a.push("-pix_fmt".into());
        a.push(self.pixel_format.ffmpeg_name().into());

        // --- Rate control (skipped for lossless image sequences) ----------
        if !self.is_image_sequence() {
            self.push_rate_control(&mut a);
        }

        // --- Two-pass marker ----------------------------------------------
        if self.two_pass && !self.is_image_sequence() {
            a.push("-pass".into());
            a.push("1".into());
        }

        // --- Audio --------------------------------------------------------
        if self.container.is_image_sequence() || self.audio == AudioCodec::None {
            // Image sequences and explicit "no audio" drop the audio track.
            a.push("-an".into());
        } else if let Some(enc) = self.audio.encoder() {
            a.push("-c:a".into());
            a.push(enc.into());
            a.push("-ar".into());
            a.push(self.audio_sample_rate.to_string());
            a.push("-ac".into());
            a.push(self.audio_channels.to_string());
            // Lossy audio gets an explicit bitrate; PCM/FLAC are sample-driven.
            if matches!(self.audio, AudioCodec::Aac) && self.audio_kbps > 0 {
                a.push("-b:a".into());
                a.push(format!("{}k", self.audio_kbps));
            }
        }

        // --- Container muxer ----------------------------------------------
        if let Some(mux) = self.container.muxer() {
            a.push("-f".into());
            a.push(mux.into());
        }
        // MP4 fast-start (moov atom at the front) for streaming.
        if self.container == Container::Mp4 {
            a.push("-movflags".into());
            a.push("+faststart".into());
        }

        a
    }

    fn push_rate_control(&self, a: &mut Vec<String>) {
        match self.rate_control {
            RateControl::Cbr { kbps } => {
                let b = format!("{}k", kbps);
                a.push("-b:v".into());
                a.push(b.clone());
                a.push("-minrate".into());
                a.push(b.clone());
                a.push("-maxrate".into());
                a.push(b.clone());
                a.push("-bufsize".into());
                a.push(format!("{}k", kbps * 2));
            }
            RateControl::Vbr { kbps } => {
                a.push("-b:v".into());
                a.push(format!("{}k", kbps));
            }
            RateControl::Quality { crf } => {
                // ProRes / DNxHR use -qscale:v; the modern lossy codecs use -crf.
                match self.video {
                    VideoCodec::ProRes422(_) | VideoCodec::ProRes4444 => {
                        a.push("-qscale:v".into());
                        a.push(crf.to_string());
                    }
                    VideoCodec::Dnxhr(_) => {
                        // DNxHR is profile-driven; qscale is a no-op, skip it.
                    }
                    _ => {
                        a.push("-crf".into());
                        a.push(crf.to_string());
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args_str(spec: &ExportSpec) -> String {
        spec.ffmpeg_args().join(" ")
    }

    #[test]
    fn h264_mp4_vbr_aac_args() {
        let spec = ExportSpec::h264_mp4();
        let s = args_str(&spec);
        assert!(s.contains("-c:v libx264"), "{s}");
        assert!(s.contains("-pix_fmt yuv420p"), "{s}");
        assert!(s.contains("-b:v 12000k"), "{s}");
        assert!(s.contains("-c:a aac"), "{s}");
        assert!(s.contains("-b:a 256k"), "{s}");
        assert!(s.contains("-f mp4"), "{s}");
        assert!(s.contains("-movflags +faststart"), "{s}");
    }

    #[test]
    fn prores_422_hq_mov_args_are_correct() {
        let spec = ExportSpec::prores_mov();
        let args = spec.ffmpeg_args();
        let s = args.join(" ");
        // Encoder + profile index 3 (HQ).
        assert!(s.contains("-c:v prores_ks"), "{s}");
        let pi = args.iter().position(|x| x == "-profile:v").expect("profile flag");
        assert_eq!(args[pi + 1], "3", "ProRes 422 HQ → profile 3");
        // 10-bit 4:2:2 pixel format and qscale rate control.
        assert!(s.contains("-pix_fmt yuv422p10le"), "{s}");
        assert!(s.contains("-qscale:v 11"), "{s}");
        // PCM audio, MOV container.
        assert!(s.contains("-c:a pcm_s16le"), "{s}");
        assert!(s.contains("-f mov"), "{s}");
        // ProRes is not MP4 → no faststart.
        assert!(!s.contains("faststart"), "{s}");
        assert_eq!(spec.container.extension(), "mov");
    }

    #[test]
    fn prores_4444_uses_alpha_profile() {
        let mut spec = ExportSpec::prores_mov();
        spec.video = VideoCodec::ProRes4444;
        spec.pixel_format = PixelFormat::Yuva444p10le;
        let args = spec.ffmpeg_args();
        let pi = args.iter().position(|x| x == "-profile:v").unwrap();
        assert_eq!(args[pi + 1], "4", "ProRes 4444 → profile 4");
        assert!(args.join(" ").contains("yuva444p10le"));
    }

    #[test]
    fn dnxhr_profile_and_no_qscale() {
        let spec = ExportSpec {
            container: Container::Mov,
            video: VideoCodec::Dnxhr(DnxhrProfile::Hqx),
            audio: AudioCodec::Pcm,
            bit_depth: BitDepth::Ten,
            pixel_format: PixelFormat::Yuv422p10le,
            rate_control: RateControl::Quality { crf: 1 },
            audio_kbps: 0,
            audio_sample_rate: 48_000,
            audio_channels: 2,
            two_pass: false,
        };
        let args = spec.ffmpeg_args();
        let s = args.join(" ");
        assert!(s.contains("-c:v dnxhd"), "{s}");
        let pi = args.iter().position(|x| x == "-profile:v").unwrap();
        assert_eq!(args[pi + 1], "dnxhr_hqx");
        // DNxHR ignores qscale (profile-driven).
        assert!(!s.contains("-qscale:v"), "{s}");
    }

    #[test]
    fn vp9_webm_with_crf_and_rowmt() {
        let spec = ExportSpec {
            container: Container::WebM,
            video: VideoCodec::Vp9,
            audio: AudioCodec::None,
            bit_depth: BitDepth::Eight,
            pixel_format: PixelFormat::Yuv420p,
            rate_control: RateControl::Quality { crf: 31 },
            audio_kbps: 0,
            audio_sample_rate: 48_000,
            audio_channels: 2,
            two_pass: false,
        };
        let s = args_str(&spec);
        assert!(s.contains("-c:v libvpx-vp9"), "{s}");
        assert!(s.contains("-row-mt 1"), "{s}");
        assert!(s.contains("-crf 31"), "{s}");
        assert!(s.contains("-f webm"), "{s}");
        // Explicit no-audio.
        assert!(s.contains("-an"), "{s}");
    }

    #[test]
    fn av1_mkv_cbr_includes_bufsize() {
        let spec = ExportSpec {
            container: Container::Mkv,
            video: VideoCodec::Av1,
            audio: AudioCodec::Flac,
            bit_depth: BitDepth::Ten,
            pixel_format: PixelFormat::Yuv420p10le,
            rate_control: RateControl::Cbr { kbps: 8_000 },
            audio_kbps: 0,
            audio_sample_rate: 48_000,
            audio_channels: 2,
            two_pass: false,
        };
        let s = args_str(&spec);
        assert!(s.contains("-c:v libaom-av1"), "{s}");
        assert!(s.contains("-b:v 8000k"), "{s}");
        assert!(s.contains("-minrate 8000k") && s.contains("-maxrate 8000k"), "{s}");
        assert!(s.contains("-bufsize 16000k"), "CBR bufsize is 2× bitrate: {s}");
        assert!(s.contains("-c:a flac"), "{s}");
        assert!(s.contains("-f matroska"), "{s}");
    }

    #[test]
    fn png_sequence_drops_audio_and_rate_control() {
        let spec = ExportSpec {
            container: Container::PngSeq,
            video: VideoCodec::Png,
            audio: AudioCodec::None,
            bit_depth: BitDepth::Eight,
            pixel_format: PixelFormat::Rgba,
            rate_control: RateControl::Vbr { kbps: 99_999 },
            audio_kbps: 0,
            audio_sample_rate: 48_000,
            audio_channels: 2,
            two_pass: true,
        };
        let s = args_str(&spec);
        assert!(s.contains("-c:v png"), "{s}");
        assert!(s.contains("-pix_fmt rgba"), "{s}");
        // No rate control / pass for an image sequence.
        assert!(!s.contains("-b:v"), "{s}");
        assert!(!s.contains("-pass"), "{s}");
        assert!(s.contains("-an"), "{s}");
        assert!(spec.is_image_sequence());
        assert_eq!(spec.container.extension(), "png");
    }

    #[test]
    fn exr_sequence_uses_float_rgba() {
        let spec = ExportSpec {
            container: Container::ExrSeq,
            video: VideoCodec::Exr,
            audio: AudioCodec::None,
            bit_depth: BitDepth::Sixteen,
            pixel_format: PixelFormat::Rgba16f,
            rate_control: RateControl::Quality { crf: 0 },
            audio_kbps: 0,
            audio_sample_rate: 48_000,
            audio_channels: 2,
            two_pass: false,
        };
        let s = args_str(&spec);
        assert!(s.contains("-c:v exr"), "{s}");
        assert!(s.contains("-pix_fmt gbrapf32le"), "{s}");
        assert!(s.contains("-an"), "{s}");
        assert_eq!(spec.bit_depth.bits(), 16);
    }

    #[test]
    fn two_pass_adds_pass_flag() {
        let mut spec = ExportSpec::h264_mp4();
        spec.two_pass = true;
        let s = args_str(&spec);
        assert!(s.contains("-pass 1"), "{s}");
    }

    #[test]
    fn h265_aac_mp4_args() {
        let mut spec = ExportSpec::h264_mp4();
        spec.video = VideoCodec::H265;
        spec.rate_control = RateControl::Quality { crf: 23 };
        let s = args_str(&spec);
        assert!(s.contains("-c:v libx265"), "{s}");
        assert!(s.contains("-crf 23"), "{s}");
        assert!(!s.contains("-b:v"), "quality mode has no -b:v: {s}");
    }
}
