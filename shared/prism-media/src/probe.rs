//! Media probing: shells out to `ffprobe` and parses its JSON into
//! [`MediaInfo`] / [`AudioInfo`]. Backs [`probe`] (video media) and
//! [`probe_audio`] (audio-only files).

use std::path::Path;
use std::process::Command;

use serde::Deserialize;

use crate::{ffprobe_bin, spawn_err, AudioInfo, MediaError, MediaInfo};

// --- ffprobe JSON shape -----------------------------------------------------

#[derive(Deserialize)]
pub(crate) struct ProbeJson {
    #[serde(default)]
    streams: Vec<ProbeStream>,
    #[serde(default)]
    format: ProbeFormat,
}

#[derive(Deserialize, Default)]
struct ProbeFormat {
    /// Duration is a string in ffprobe JSON (e.g. `"1.000000"`).
    duration: Option<String>,
}

#[derive(Deserialize)]
struct ProbeStream {
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    avg_frame_rate: Option<String>,
    r_frame_rate: Option<String>,
    /// ffprobe reports sample_rate as a string (e.g. `"48000"`).
    sample_rate: Option<String>,
    channels: Option<u16>,
}

/// Parse an ffprobe rational rate string (`"30000/1001"`, `"25/1"`, `"0/0"`)
/// into fps. A zero / malformed denominator yields `0.0`.
pub(crate) fn parse_rate(s: &str) -> f64 {
    let mut it = s.split('/');
    let num: f64 = it.next().and_then(|n| n.parse().ok()).unwrap_or(0.0);
    let den: f64 = it.next().and_then(|d| d.parse().ok()).unwrap_or(0.0);
    if den.abs() < f64::EPSILON {
        0.0
    } else {
        num / den
    }
}

/// Run `ffprobe -v quiet -print_format json -show_format -show_streams <path>`
/// and parse the JSON. Shared by [`probe`] and [`probe_audio`].
pub(crate) fn run_ffprobe(path: &Path) -> Result<ProbeJson, MediaError> {
    let bin = ffprobe_bin();
    let output = Command::new(&bin)
        .args(["-v", "quiet", "-print_format", "json", "-show_format", "-show_streams"])
        .arg(path)
        .output()
        .map_err(|e| spawn_err(&bin, e))?;

    if !output.status.success() {
        return Err(MediaError::Probe(format!(
            "ffprobe exited {} for {}",
            output.status,
            path.display()
        )));
    }

    serde_json::from_slice(&output.stdout)
        .map_err(|e| MediaError::Parse(format!("ffprobe json: {e}")))
}

/// The container duration (seconds) from a parsed probe, `0.0` when absent.
fn duration_of(json: &ProbeJson) -> f64 {
    json.format
        .duration
        .as_deref()
        .and_then(|d| d.parse::<f64>().ok())
        .unwrap_or(0.0)
}

/// Build an [`AudioInfo`] from the first audio stream in a parsed probe, if any.
fn first_audio_info(json: &ProbeJson) -> Option<AudioInfo> {
    json.streams
        .iter()
        .find(|s| s.codec_type.as_deref() == Some("audio"))
        .map(|a| AudioInfo {
            sample_rate: a
                .sample_rate
                .as_deref()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0),
            channels: a.channels.unwrap_or(0),
            codec: a.codec_name.clone(),
        })
}

/// Probe `path` for its container duration and first video/audio stream info via
/// `ffprobe -v quiet -print_format json -show_format -show_streams <path>`.
///
/// Requires a video stream (it is the video-media probe; for an **audio-only**
/// file use [`probe_audio`]).
pub fn probe(path: impl AsRef<Path>) -> Result<MediaInfo, MediaError> {
    let path = path.as_ref();
    let json = run_ffprobe(path)?;

    let video = json
        .streams
        .iter()
        .find(|s| s.codec_type.as_deref() == Some("video"))
        .ok_or_else(|| MediaError::Parse("no video stream".to_string()))?;

    let width = video.width.unwrap_or(0);
    let height = video.height.unwrap_or(0);
    // Prefer avg_frame_rate; fall back to r_frame_rate when it is 0/unknown.
    let fps = video
        .avg_frame_rate
        .as_deref()
        .map(parse_rate)
        .filter(|f| *f > 0.0)
        .or_else(|| video.r_frame_rate.as_deref().map(parse_rate))
        .unwrap_or(0.0);

    let audio_info = first_audio_info(&json);

    Ok(MediaInfo {
        duration_secs: duration_of(&json),
        width,
        height,
        fps,
        has_audio: audio_info.is_some(),
        video_codec: video.codec_name.clone(),
        audio: audio_info,
    })
}

/// Probe an **audio-only** file (or any file's audio) for its container duration
/// and first audio stream — the audio analogue of [`probe`], which requires a
/// video stream and so rejects a pure audio file (`.mp3`, `.wav`, …).
///
/// Returns a [`MediaInfo`] with `width`/`height`/`fps` zeroed and `video_codec`
/// `None` (there is no video), `has_audio` / `audio` set from the first audio
/// stream. Errors with [`MediaError::Parse`] when the file carries no audio
/// stream at all.
pub fn probe_audio(path: impl AsRef<Path>) -> Result<MediaInfo, MediaError> {
    let path = path.as_ref();
    let json = run_ffprobe(path)?;

    let audio_info =
        first_audio_info(&json).ok_or_else(|| MediaError::Parse("no audio stream".to_string()))?;

    Ok(MediaInfo {
        duration_secs: duration_of(&json),
        width: 0,
        height: 0,
        fps: 0.0,
        has_audio: true,
        video_codec: None,
        audio: Some(audio_info),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_binary_is_binary_not_found() {
        // An override pointing at a nonexistent binary must surface as
        // BinaryNotFound (never a panic / generic Io), so callers degrade.
        std::env::set_var("PRISM_FFPROBE", "prism_media_definitely_missing_binary");
        let res = probe("whatever.mp4");
        std::env::remove_var("PRISM_FFPROBE");
        assert!(matches!(res, Err(MediaError::BinaryNotFound(_))), "got {res:?}");
    }

    #[test]
    fn parse_rate_handles_rationals_and_zero() {
        assert!((parse_rate("30/1") - 30.0).abs() < 1e-9);
        assert!((parse_rate("30000/1001") - 29.97).abs() < 0.01);
        assert_eq!(parse_rate("0/0"), 0.0);
        assert_eq!(parse_rate("garbage"), 0.0);
    }
}
