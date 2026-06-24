//! H.264 (MP4) encode: parameters ([`EncodeParams`]), the pure argument-vector
//! builder ([`encode_h264_args`]), an ffmpeg-availability probe
//! ([`ffmpeg_available`]), and the silent frame-stream encoder
//! ([`encode_h264`]). The audio-muxing twin lives in [`crate::audio`].

use std::path::Path;
use std::process::Command;

use crate::{ffmpeg_bin, spawn_err, MediaError};

/// Parameters for an [`encode_h264`] run: the output geometry, frame rate, and a
/// destination path. The pixel format fed on stdin is always straight-alpha
/// `rgba` (matching [`crate::VideoFrame`] and the suite's CPU renderers); ffmpeg
/// converts it to `yuv420p` for broad-player compatibility.
#[derive(Clone, Debug, PartialEq)]
pub struct EncodeParams {
    /// Frame width in pixels (must be > 0; even for `yuv420p`).
    pub width: u32,
    /// Frame height in pixels (must be > 0; even for `yuv420p`).
    pub height: u32,
    /// Output frame rate (frames per second; must be > 0).
    pub fps: f64,
    /// libx264 Constant Rate Factor (0 = lossless, ~18 visually lossless, 23
    /// default, 51 worst). Lower is higher quality / bigger file.
    pub crf: u32,
    /// The x264 `-preset` (encode speed↔compression trade-off), e.g. `medium`.
    pub preset: String,
}

impl EncodeParams {
    /// Sensible defaults for a delivery H.264: CRF 18 (visually lossless),
    /// `medium` preset.
    pub fn new(width: u32, height: u32, fps: f64) -> Self {
        Self { width, height, fps, crf: 18, preset: "medium".to_string() }
    }
}

/// Build the **pure** ffmpeg argument vector for an H.264 encode that reads raw
/// `rgba` frames from stdin and writes an MP4 to `out`. Kept free of any I/O so
/// the exact invocation is unit-testable:
///
/// ```text
/// ffmpeg -y -f rawvideo -pix_fmt rgba -s WxH -r FPS -i -
///        -c:v libx264 -pix_fmt yuv420p -preset PRESET -crf CRF
///        -movflags +faststart out.mp4
/// ```
///
/// `-y` overwrites an existing file; the input `-` is stdin; `+faststart`
/// relocates the moov atom for web playback. The `fps` is formatted with enough
/// precision to carry NTSC fractional rates (e.g. `29.97`).
pub fn encode_h264_args(params: &EncodeParams, out: &Path) -> Vec<String> {
    vec![
        "-y".to_string(),
        "-f".to_string(),
        "rawvideo".to_string(),
        "-pix_fmt".to_string(),
        "rgba".to_string(),
        "-s".to_string(),
        format!("{}x{}", params.width, params.height),
        "-r".to_string(),
        format_rate(params.fps),
        "-i".to_string(),
        "-".to_string(),
        "-c:v".to_string(),
        "libx264".to_string(),
        "-pix_fmt".to_string(),
        "yuv420p".to_string(),
        "-preset".to_string(),
        params.preset.clone(),
        "-crf".to_string(),
        params.crf.to_string(),
        "-movflags".to_string(),
        "+faststart".to_string(),
        out.to_string_lossy().into_owned(),
    ]
}

/// Format a frame rate for ffmpeg's `-r`: an integer rate prints without a
/// decimal point, a fractional rate keeps up to two decimals (`29.97`).
pub(crate) fn format_rate(fps: f64) -> String {
    if (fps.fract()).abs() < 1e-6 {
        format!("{}", fps.round() as i64)
    } else {
        // Trim to 2 dp, then drop a trailing zero (e.g. 23.90 -> 23.9).
        let s = format!("{fps:.2}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// Returns `true` if the configured `ffmpeg` binary can be spawned (a `-version`
/// probe succeeds). Callers gate the actual encode on this so a missing binary
/// surfaces as a clear UI error instead of a failed encode. Never panics.
pub fn ffmpeg_available() -> bool {
    Command::new(ffmpeg_bin())
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Encode a stream of straight-alpha **`rgba`** frames (each exactly
/// `width * height * 4` bytes, top-left origin) to an H.264 MP4 at `out`.
///
/// Frames are produced lazily by `frames` (so the caller can render full-res
/// frames one at a time without holding them all in memory) and piped to
/// `ffmpeg` via stdin using the invocation from [`encode_h264_args`]. Returns
/// the number of frames written.
///
/// Errors:
/// - [`MediaError::BinaryNotFound`] when `ffmpeg` can't be spawned (gate with
///   [`ffmpeg_available`] for a clean UI message first);
/// - [`MediaError::Decode`] for a zero-size geometry, an empty stream, a frame
///   of the wrong byte length, or a non-zero ffmpeg exit (with its stderr).
///
/// This is the suite's shared, app-agnostic H.264 encoder (co-owned with Pulse);
/// the caller owns *what* to render (a Reel program frame, a Pulse comp frame).
pub fn encode_h264<I>(mut frames: I, params: &EncodeParams, out: &Path) -> Result<usize, MediaError>
where
    I: Iterator<Item = Vec<u8>>,
{
    if params.width == 0 || params.height == 0 {
        return Err(MediaError::Decode(format!(
            "zero-size encode geometry ({}x{})",
            params.width, params.height
        )));
    }
    let frame_bytes = (params.width as usize) * (params.height as usize) * 4;

    let bin = ffmpeg_bin();
    let args = encode_h264_args(params, out);
    let mut child = Command::new(&bin)
        .args(&args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| spawn_err(&bin, e))?;

    let mut written = 0usize;
    {
        use std::io::Write;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| MediaError::Decode("failed to open ffmpeg stdin".to_string()))?;
        for frame in frames.by_ref() {
            if frame.len() != frame_bytes {
                // Drop stdin (closing the pipe) and reap before reporting, so we
                // don't leave a zombie / blocked ffmpeg.
                drop(stdin);
                let _ = child.wait();
                return Err(MediaError::Decode(format!(
                    "frame {written} has {} bytes, expected {frame_bytes} ({}x{})",
                    frame.len(),
                    params.width,
                    params.height
                )));
            }
            if let Err(e) = stdin.write_all(&frame) {
                // A broken pipe means ffmpeg exited early; fall through to wait()
                // so its stderr explains why.
                if e.kind() != std::io::ErrorKind::BrokenPipe {
                    drop(stdin);
                    let _ = child.wait();
                    return Err(MediaError::Io(e));
                }
                break;
            }
            written += 1;
        }
        // Closing stdin (end of block) signals EOF to ffmpeg.
    }

    let output = child.wait_with_output().map_err(MediaError::Io)?;
    if !output.status.success() {
        return Err(MediaError::Decode(format!(
            "ffmpeg encode exited {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    if written == 0 {
        return Err(MediaError::Decode("no frames to encode".to_string()));
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::temp_path;

    #[test]
    fn encode_args_are_the_expected_invocation() {
        let params = EncodeParams { width: 1920, height: 1080, fps: 30.0, crf: 18, preset: "medium".to_string() };
        let args = encode_h264_args(&params, Path::new("/tmp/out.mp4"));
        let expected: Vec<String> = [
            "-y", "-f", "rawvideo", "-pix_fmt", "rgba", "-s", "1920x1080", "-r", "30", "-i", "-",
            "-c:v", "libx264", "-pix_fmt", "yuv420p", "-preset", "medium", "-crf", "18",
            "-movflags", "+faststart", "/tmp/out.mp4",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(args, expected);
    }

    #[test]
    fn encode_args_carry_geometry_rate_crf_preset() {
        let params = EncodeParams { width: 640, height: 360, fps: 23.976, crf: 23, preset: "veryfast".to_string() };
        let args = encode_h264_args(&params, Path::new("clip.mp4"));
        assert!(args.windows(2).any(|w| w[0] == "-s" && w[1] == "640x360"));
        // 23.976 -> "23.98" trimmed to two-dp.
        assert!(args.windows(2).any(|w| w[0] == "-r" && w[1] == "23.98"), "rate arg {args:?}");
        assert!(args.windows(2).any(|w| w[0] == "-crf" && w[1] == "23"));
        assert!(args.windows(2).any(|w| w[0] == "-preset" && w[1] == "veryfast"));
        assert_eq!(args.last().unwrap(), "clip.mp4");
    }

    #[test]
    fn format_rate_integer_and_ntsc() {
        assert_eq!(format_rate(30.0), "30");
        assert_eq!(format_rate(24.0), "24");
        assert_eq!(format_rate(29.97), "29.97");
        assert_eq!(format_rate(59.94), "59.94");
    }

    #[test]
    fn encode_default_params() {
        let p = EncodeParams::new(1280, 720, 25.0);
        assert_eq!((p.width, p.height), (1280, 720));
        assert_eq!(p.crf, 18);
        assert_eq!(p.preset, "medium");
    }

    #[test]
    fn encode_rejects_zero_geometry() {
        let params = EncodeParams::new(0, 0, 30.0);
        let frames = std::iter::empty::<Vec<u8>>();
        let out = temp_path("zero.mp4");
        assert!(matches!(
            encode_h264(frames, &params, &out),
            Err(MediaError::Decode(_))
        ));
    }

    #[test]
    fn encode_real_roundtrip_then_probe() {
        // Gated like the decode tests: skip silently if ffmpeg is absent.
        if !ffmpeg_available() {
            eprintln!("ffmpeg not available — skipping prism-media encode test");
            return;
        }
        let (w, h, fps) = (32u32, 24u32, 10.0f64);
        let params = EncodeParams::new(w, h, fps);
        let out = temp_path("encode.mp4");
        // 10 solid-magenta frames.
        let frames = (0..10).map(move |_| {
            let mut f = Vec::with_capacity((w * h * 4) as usize);
            for _ in 0..(w * h) {
                f.extend_from_slice(&[255, 0, 255, 255]);
            }
            f
        });
        let n = encode_h264(frames, &params, &out).expect("encode");
        assert_eq!(n, 10);
        let info = crate::probe::probe(&out).expect("probe encoded mp4");
        assert_eq!(info.width, w);
        assert_eq!(info.height, h);
        assert!(info.video_codec.is_some());
        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn encode_rejects_wrong_frame_size() {
        if !ffmpeg_available() {
            eprintln!("ffmpeg not available — skipping wrong-frame-size encode test");
            return;
        }
        let params = EncodeParams::new(16, 16, 10.0);
        let out = temp_path("badsize.mp4");
        // A frame too short for 16*16*4.
        let frames = std::iter::once(vec![0u8; 10]);
        assert!(matches!(
            encode_h264(frames, &params, &out),
            Err(MediaError::Decode(_))
        ));
        let _ = std::fs::remove_file(&out);
    }
}
