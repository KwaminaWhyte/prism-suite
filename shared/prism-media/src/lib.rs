//! prism-media — the suite's app-agnostic A/V decode bridge.
//!
//! This crate probes media metadata and decodes individual video frames + whole
//! audio tracks for the suite's video apps (Reel, Pulse). It is intentionally
//! free of any app, GPU, or UI types: a [`VideoFrame`] is a flat 8-bit RGBA
//! buffer and an [`AudioBuffer`] is interleaved `f32` — the caller uploads /
//! plays them however it likes.
//!
//! **Backend: the ffmpeg / ffprobe CLI.** Rather than link against libav (the
//! `ffmpeg-sys` / `ffmpeg-next` bindings need `pkg-config` and lag new FFmpeg
//! releases), prism-media shells out to the `ffmpeg` and `ffprobe` *binaries*
//! via [`std::process::Command`]. This is version-tolerant (works with FFmpeg
//! 8.x and needs no linking) and keeps the decode path behind a small surface
//! ([`probe`], [`decode_frame_at`], [`decode_audio`]) so it can be swapped for
//! an in-process libav backend later without touching callers.
//!
//! The binary paths default to `ffmpeg` / `ffprobe` (resolved on `PATH`) and are
//! overridable via the `PRISM_FFMPEG` / `PRISM_FFPROBE` environment variables.
//!
//! **Graceful degradation.** A missing binary surfaces as
//! [`MediaError::BinaryNotFound`] (never a panic), so a caller can fall back to a
//! placeholder when FFmpeg isn't installed.
//!
//! ## Module map
//!
//! The crate's surface is re-exported from this root so callers keep using
//! `prism_media::probe`, `prism_media::decode_frame_at`, etc. The
//! implementation is split by concern:
//! - [`probe`] / [`probe_audio`] — `probe.rs`
//! - [`decode_frame_at`] / [`decode_audio`] — `decode.rs`
//! - [`EncodeParams`] / [`encode_h264`] / [`encode_h264_args`] /
//!   [`ffmpeg_available`] — `encode.rs`
//! - [`AudioMix`] / [`encode_h264_with_audio`] /
//!   [`encode_h264_args_with_audio`] — `audio.rs`

use thiserror::Error;

mod audio;
mod decode;
mod encode;
mod probe;

#[cfg(test)]
mod test_util;

// --- Public surface re-exports ----------------------------------------------
// The implementation lives in the submodules above; re-export every previously
// crate-root `pub` item from here so `prism_media::X` keeps resolving.
pub use audio::{encode_h264_args_with_audio, encode_h264_with_audio, AudioMix};
pub use decode::{decode_audio, decode_frame_at};
pub use encode::{encode_h264, encode_h264_args, ffmpeg_available, EncodeParams};
pub use probe::{probe, probe_audio};

/// Errors from probing or decoding media.
#[derive(Debug, Error)]
pub enum MediaError {
    /// The `ffmpeg` / `ffprobe` binary could not be found / spawned. Callers
    /// should degrade gracefully (e.g. draw a placeholder) rather than fail hard.
    #[error("ffmpeg/ffprobe binary not found: {0}")]
    BinaryNotFound(String),
    /// `ffprobe` ran but failed (non-zero exit), with its stderr.
    #[error("probe failed: {0}")]
    Probe(String),
    /// `ffmpeg` ran but failed (non-zero exit / short read), with detail.
    #[error("decode failed: {0}")]
    Decode(String),
    /// The probe JSON could not be parsed / lacked an expected field.
    #[error("parse failed: {0}")]
    Parse(String),
    /// An underlying I/O error spawning a process or reading its output.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Audio-stream metadata from a probe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioInfo {
    /// Samples per second per channel (e.g. 48000).
    pub sample_rate: u32,
    /// Number of channels (1 = mono, 2 = stereo, …).
    pub channels: u16,
    /// The audio codec name reported by ffprobe (e.g. `aac`), if known.
    pub codec: Option<String>,
}

/// Probed media metadata: container duration plus the first video stream's
/// geometry / rate and whether the file carries audio.
#[derive(Clone, Debug, PartialEq)]
pub struct MediaInfo {
    /// Total duration in seconds (container `format.duration`).
    pub duration_secs: f64,
    /// Video width in pixels (first video stream).
    pub width: u32,
    /// Video height in pixels (first video stream).
    pub height: u32,
    /// Frames per second (first video stream's `avg_frame_rate`).
    pub fps: f64,
    /// True if the file carries at least one audio stream.
    pub has_audio: bool,
    /// The video codec name (e.g. `h264`), if a video stream was found.
    pub video_codec: Option<String>,
    /// The first audio stream's metadata, if any.
    pub audio: Option<AudioInfo>,
}

/// A single decoded video frame: tightly packed **8-bit RGBA, straight
/// (non-premultiplied) alpha, sRGB**, `width * height * 4` bytes, top-left
/// origin. This matches what egui / wgpu expect for an sRGB texture upload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    /// `width * height * 4` bytes of straight-alpha sRGB RGBA.
    pub rgba: Vec<u8>,
}

/// A decoded audio track: interleaved 32-bit float PCM in `[-1, 1]`.
///
/// `samples.len() == frames * channels`; channel `c` of frame `f` is
/// `samples[f * channels + c]`. Whole-file decode (no streaming) for now.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioBuffer {
    pub sample_rate: u32,
    pub channels: u16,
    /// Interleaved `f32` samples (`frames * channels` long).
    pub samples: Vec<f32>,
}

/// The configured `ffmpeg` binary (`$PRISM_FFMPEG`, else `ffmpeg`).
pub(crate) fn ffmpeg_bin() -> String {
    std::env::var("PRISM_FFMPEG").unwrap_or_else(|_| "ffmpeg".to_string())
}

/// The configured `ffprobe` binary (`$PRISM_FFPROBE`, else `ffprobe`).
pub(crate) fn ffprobe_bin() -> String {
    std::env::var("PRISM_FFPROBE").unwrap_or_else(|_| "ffprobe".to_string())
}

/// Map a spawn error: a not-found binary becomes [`MediaError::BinaryNotFound`]
/// (so callers can degrade), any other I/O error is passed through.
pub(crate) fn spawn_err(bin: &str, e: std::io::Error) -> MediaError {
    if e.kind() == std::io::ErrorKind::NotFound {
        MediaError::BinaryNotFound(bin.to_string())
    } else {
        MediaError::Io(e)
    }
}
