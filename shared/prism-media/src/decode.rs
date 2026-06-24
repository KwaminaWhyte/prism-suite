//! Frame + audio decoding: shells out to `ffmpeg` to pull a single video frame
//! ([`decode_frame_at`]) or a whole audio track ([`decode_audio`]) into the flat
//! buffers the suite's apps upload / play.

use std::path::Path;
use std::process::Command;

use crate::probe::probe;
use crate::{ffmpeg_bin, spawn_err, AudioBuffer, MediaError, VideoFrame};

/// Decode a single video frame at `t_secs` into the file as straight-alpha sRGB
/// RGBA.
///
/// Runs `ffmpeg -ss <t> -i <path> -frames:v 1 -f rawvideo -pix_fmt rgba [-vf
/// scale=w:h] -v error -` and reads exactly `width * height * 4` bytes from
/// stdout. The output geometry is `scale` when given, otherwise the file's
/// probed `width`/`height` (so the caller need not probe first for native-size
/// frames). `-ss` before `-i` is an input seek (fast, keyframe-accurate enough
/// for scrubbing).
pub fn decode_frame_at(
    path: impl AsRef<Path>,
    t_secs: f64,
    scale: Option<(u32, u32)>,
) -> Result<VideoFrame, MediaError> {
    let path = path.as_ref();

    // Determine the output geometry: explicit scale, else the probed size.
    let (w, h) = match scale {
        Some((w, h)) => (w, h),
        None => {
            let info = probe(path)?;
            (info.width, info.height)
        }
    };
    if w == 0 || h == 0 {
        return Err(MediaError::Decode(format!(
            "zero-size frame ({w}x{h}) for {}",
            path.display()
        )));
    }

    let bin = ffmpeg_bin();
    let mut cmd = Command::new(&bin);
    // Input seek before -i for a fast seek to ~t.
    cmd.arg("-ss").arg(format!("{t_secs}"));
    cmd.arg("-i").arg(path);
    cmd.args(["-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgba"]);
    if let Some((sw, sh)) = scale {
        cmd.arg("-vf").arg(format!("scale={sw}:{sh}"));
    }
    cmd.args(["-v", "error", "-"]);

    let output = cmd.output().map_err(|e| spawn_err(&bin, e))?;
    if !output.status.success() {
        return Err(MediaError::Decode(format!(
            "ffmpeg exited {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }

    let expected = (w as usize) * (h as usize) * 4;
    if output.stdout.len() < expected {
        return Err(MediaError::Decode(format!(
            "short frame read: got {} bytes, expected {expected} ({w}x{h})",
            output.stdout.len()
        )));
    }

    let mut rgba = output.stdout;
    rgba.truncate(expected);
    Ok(VideoFrame { width: w, height: h, rgba })
}

/// Decode the whole audio track to interleaved `f32` PCM at `sample_rate` /
/// `channels`.
///
/// Runs `ffmpeg -i <path> -f f32le -ac <channels> -ar <sample_rate> -v error -`
/// and reinterprets the little-endian `f32` stdout as interleaved samples.
/// Whole-file decode (no streaming) — acceptable for the current pass.
pub fn decode_audio(
    path: impl AsRef<Path>,
    sample_rate: u32,
    channels: u16,
) -> Result<AudioBuffer, MediaError> {
    let path = path.as_ref();
    let bin = ffmpeg_bin();
    let output = Command::new(&bin)
        .arg("-i")
        .arg(path)
        .args([
            "-f",
            "f32le",
            "-ac",
            &channels.to_string(),
            "-ar",
            &sample_rate.to_string(),
            "-v",
            "error",
            "-",
        ])
        .output()
        .map_err(|e| spawn_err(&bin, e))?;

    if !output.status.success() {
        return Err(MediaError::Decode(format!(
            "ffmpeg audio exited {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }

    // Reinterpret the LE f32 byte stream as samples (drop a trailing partial).
    let bytes = output.stdout;
    let n = bytes.len() / 4;
    let mut samples = Vec::with_capacity(n);
    for chunk in bytes.chunks_exact(4) {
        samples.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }

    Ok(AudioBuffer { sample_rate, channels, samples })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffmpeg_bin;
    use crate::probe::probe;
    use crate::test_util::{make_clip, temp_path};
    use crate::MediaError;
    use std::process::Command;

    #[test]
    fn probe_and_decode_generated_clip() {
        let clip = temp_path("probe.mp4");
        if !make_clip(&clip) {
            eprintln!("ffmpeg not available — skipping prism-media decode test");
            return;
        }

        // probe: ~1.0s, 64x48, ~10fps, a video codec, no audio.
        let info = match probe(&clip) {
            Ok(i) => i,
            Err(MediaError::BinaryNotFound(_)) => {
                let _ = std::fs::remove_file(&clip);
                return;
            }
            Err(e) => panic!("probe failed: {e}"),
        };
        assert!(
            (info.duration_secs - 1.0).abs() < 0.2,
            "duration ~1.0s, got {}",
            info.duration_secs
        );
        assert_eq!(info.width, 64);
        assert_eq!(info.height, 48);
        assert!((info.fps - 10.0).abs() < 0.5, "fps ~10, got {}", info.fps);
        assert!(info.video_codec.is_some());
        assert!(!info.has_audio);

        // decode native-size frame at 0.5s: 64*48*4 bytes.
        let frame = decode_frame_at(&clip, 0.5, None).expect("decode native frame");
        assert_eq!(frame.width, 64);
        assert_eq!(frame.height, 48);
        assert_eq!(frame.rgba.len(), 64 * 48 * 4);

        // decode scaled frame: 32*24*4 bytes.
        let scaled = decode_frame_at(&clip, 0.5, Some((32, 24))).expect("decode scaled frame");
        assert_eq!(scaled.width, 32);
        assert_eq!(scaled.height, 24);
        assert_eq!(scaled.rgba.len(), 32 * 24 * 4);

        let _ = std::fs::remove_file(&clip);
    }

    #[test]
    fn decode_audio_of_generated_clip() {
        // A clip WITH an audio track (sine) so decode_audio has something to read.
        let clip = temp_path("audio.mp4");
        let bin = ffmpeg_bin();
        let made = Command::new(&bin)
            .args([
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=1:size=64x48:rate=10",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=1",
                "-pix_fmt",
                "yuv420p",
                "-shortest",
                "-y",
            ])
            .arg(&clip)
            .args(["-v", "error"])
            .status();
        match made {
            Ok(s) if s.success() => {}
            _ => {
                eprintln!("ffmpeg not available — skipping prism-media audio test");
                return;
            }
        }

        let info = probe(&clip).expect("probe audio clip");
        assert!(info.has_audio);

        let buf = match decode_audio(&clip, 48000, 2) {
            Ok(b) => b,
            Err(MediaError::BinaryNotFound(_)) => {
                let _ = std::fs::remove_file(&clip);
                return;
            }
            Err(e) => panic!("decode_audio failed: {e}"),
        };
        assert_eq!(buf.sample_rate, 48000);
        assert_eq!(buf.channels, 2);
        // ~1s of stereo @48k ≈ 96000 interleaved samples; just assert non-empty
        // and an even (stereo-interleaved) length.
        assert!(!buf.samples.is_empty());
        assert_eq!(buf.samples.len() % 2, 0);

        let _ = std::fs::remove_file(&clip);
    }

    #[test]
    fn probe_audio_of_audio_only_file() {
        // An audio-only file (no video stream) that `probe` rejects but
        // `probe_audio` accepts, reporting duration + audio stream info.
        let clip = temp_path("audio_only.wav");
        let bin = ffmpeg_bin();
        let made = Command::new(&bin)
            .args([
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=1",
                "-ar",
                "44100",
                "-ac",
                "2",
                "-y",
            ])
            .arg(&clip)
            .args(["-v", "error"])
            .status();
        match made {
            Ok(s) if s.success() => {}
            _ => {
                eprintln!("ffmpeg not available — skipping probe_audio test");
                return;
            }
        }

        // `probe` rejects an audio-only file (no video stream).
        assert!(matches!(probe(&clip), Err(MediaError::Parse(_))));

        // `probe_audio` accepts it: ~1s, no video geometry, an audio stream.
        let info = match crate::probe::probe_audio(&clip) {
            Ok(i) => i,
            Err(MediaError::BinaryNotFound(_)) => {
                let _ = std::fs::remove_file(&clip);
                return;
            }
            Err(e) => panic!("probe_audio failed: {e}"),
        };
        assert!((info.duration_secs - 1.0).abs() < 0.2, "duration {}", info.duration_secs);
        assert_eq!(info.width, 0);
        assert_eq!(info.height, 0);
        assert!(info.video_codec.is_none());
        assert!(info.has_audio);
        let audio = info.audio.expect("audio stream");
        assert_eq!(audio.sample_rate, 44100);
        assert_eq!(audio.channels, 2);

        let _ = std::fs::remove_file(&clip);
    }
}
