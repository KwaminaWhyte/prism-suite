//! Audio-aware encode: muxes an interleaved-`f32` program mix ([`AudioMix`]) into
//! an H.264 MP4 as an AAC track ([`encode_h264_with_audio`]), plus the pure
//! argument-vector builder ([`encode_h264_args_with_audio`]) and the canonical
//! float-WAV serializer the mux uses for ffmpeg's second input.

use std::path::Path;
use std::process::Command;

#[allow(unused_imports)] // `encode_h264_args` is referenced only by a doc-link.
use crate::encode::{encode_h264_args, format_rate, EncodeParams};
use crate::{ffmpeg_bin, spawn_err, MediaError};

/// An interleaved `f32` PCM program-audio mix to mux into an encoded video: the
/// samples (channel `c` of frame `f` is `samples[f * channels + c]`) plus the
/// `sample_rate` and `channels` they were rendered at. Built by the caller's
/// audio mixer over the same time range as the video frames; muxed by
/// [`encode_h264_with_audio`] as a second ffmpeg input so the exported MP4 has
/// sound. Mirrors [`crate::AudioBuffer`] but is the *encode* input (named
/// distinctly so the encode surface reads clearly).
#[derive(Clone, Debug, PartialEq)]
pub struct AudioMix {
    /// Interleaved `f32` samples in `[-1, 1]` (`frames * channels` long).
    pub samples: Vec<f32>,
    /// Sample rate (Hz) the mix was rendered at.
    pub sample_rate: u32,
    /// Channel count of the interleaved mix (1 = mono, 2 = stereo, …).
    pub channels: u16,
}

impl AudioMix {
    /// Build a mix from interleaved `f32` samples + their rate / channels.
    pub fn new(samples: Vec<f32>, sample_rate: u32, channels: u16) -> Self {
        Self { samples, sample_rate, channels }
    }

    /// True when there is nothing to mux (no samples, or a degenerate
    /// rate / channel count) — the caller should fall back to a silent encode.
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty() || self.sample_rate == 0 || self.channels == 0
    }
}

/// Build the **pure** ffmpeg argument vector for an H.264 encode that reads raw
/// `rgba` frames from stdin **and muxes a separate audio file** (`audio`, a WAV
/// on disk) into the MP4 at `out`. Kept free of any I/O so the exact invocation
/// is unit-testable:
///
/// ```text
/// ffmpeg -y -f rawvideo -pix_fmt rgba -s WxH -r FPS -i -
///        -i audio.wav
///        -map 0:v:0 -map 1:a:0
///        -c:v libx264 -pix_fmt yuv420p -preset PRESET -crf CRF
///        -c:a aac -b:a 192k -shortest
///        -movflags +faststart out.mp4
/// ```
///
/// The first input (`-`) is the rgba video over stdin; the second (`-i
/// audio.wav`) is the rendered program audio. Explicit `-map`s select the video
/// stream from input 0 and the audio stream from input 1. `-c:a aac -b:a 192k`
/// encodes broadly-compatible AAC; `-shortest` ends the file at the shorter of
/// the two streams so a slight audio/video length mismatch can't leave a
/// trailing silent/black tail. The video options match [`encode_h264_args`].
pub fn encode_h264_args_with_audio(params: &EncodeParams, audio: &Path, out: &Path) -> Vec<String> {
    vec![
        "-y".to_string(),
        // Input 0: raw rgba video on stdin.
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
        // Input 1: the rendered program audio (a WAV on disk).
        "-i".to_string(),
        audio.to_string_lossy().into_owned(),
        // Map the video from input 0 and the audio from input 1 explicitly.
        "-map".to_string(),
        "0:v:0".to_string(),
        "-map".to_string(),
        "1:a:0".to_string(),
        // Video codec (matches the silent path).
        "-c:v".to_string(),
        "libx264".to_string(),
        "-pix_fmt".to_string(),
        "yuv420p".to_string(),
        "-preset".to_string(),
        params.preset.clone(),
        "-crf".to_string(),
        params.crf.to_string(),
        // Audio codec.
        "-c:a".to_string(),
        "aac".to_string(),
        "-b:a".to_string(),
        "192k".to_string(),
        // End at the shorter stream so neither leaves a trailing tail.
        "-shortest".to_string(),
        "-movflags".to_string(),
        "+faststart".to_string(),
        out.to_string_lossy().into_owned(),
    ]
}

/// Serialize an interleaved `f32` [`AudioMix`] to a canonical **WAVE** file
/// (`WAVE_FORMAT_IEEE_FLOAT`, 32-bit float) as a byte vector. Pure (no I/O) so
/// the header math is unit-testable; [`encode_h264_with_audio`] writes the bytes
/// to a temp file for ffmpeg's second input.
///
/// The header is the standard 44-byte canonical layout (`RIFF`/`WAVE`/`fmt `
/// (16) with audio format `3` = IEEE float / `data`), little-endian throughout.
pub(crate) fn wav_bytes(mix: &AudioMix) -> Vec<u8> {
    let channels = mix.channels.max(1);
    let sample_rate = if mix.sample_rate > 0 { mix.sample_rate } else { 48_000 };
    let bits_per_sample: u16 = 32;
    let block_align: u16 = channels * (bits_per_sample / 8);
    let byte_rate: u32 = sample_rate * block_align as u32;
    let data_len: u32 = (mix.samples.len() * 4) as u32;
    let riff_len: u32 = 36 + data_len; // everything after the first 8 bytes.

    let mut out = Vec::with_capacity(44 + mix.samples.len() * 4);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&riff_len.to_le_bytes());
    out.extend_from_slice(b"WAVE");
    // fmt chunk (16 bytes, PCM/float).
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&3u16.to_le_bytes()); // 3 = IEEE float.
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&bits_per_sample.to_le_bytes());
    // data chunk.
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for &s in &mix.samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// Encode a stream of straight-alpha **`rgba`** frames (each exactly
/// `width * height * 4` bytes) to an H.264 MP4 at `out`, **muxing `audio`** (an
/// interleaved-`f32` program mix) in as an AAC track so the file has sound.
///
/// The audio is written to a temporary WAV next to `out` and handed to ffmpeg as
/// a second input via [`encode_h264_args_with_audio`]; the temp file is removed
/// after the encode (success or failure). Frames are produced lazily by `frames`
/// and piped over stdin, exactly like [`crate::encode_h264`] — only the arg
/// vector and the extra input differ, so a long export still never holds every
/// frame in memory.
///
/// This is the **additive** audio-aware twin of [`crate::encode_h264`] (the
/// silent path is unchanged); the caller renders the program audio mix over the
/// same time range as the frames. An [`AudioMix::is_empty`] mix should be handled
/// by the caller (fall back to [`crate::encode_h264`]); passing one here still
/// produces a valid (silent-ish) file but is not the intended use. Returns the
/// number of frames written. Errors mirror [`crate::encode_h264`], plus an
/// [`MediaError::Io`] if the temp WAV can't be written.
pub fn encode_h264_with_audio<I>(
    mut frames: I,
    params: &EncodeParams,
    audio: &AudioMix,
    out: &Path,
) -> Result<usize, MediaError>
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

    // Write the mix to a temp WAV beside the output (process-id-scoped name).
    let wav_path = {
        let mut p = std::env::temp_dir();
        p.push(format!("prism_media_mux_{}.wav", std::process::id()));
        p
    };
    std::fs::write(&wav_path, wav_bytes(audio))?;
    // Clean up the temp WAV no matter how we leave this function.
    struct TempFile(std::path::PathBuf);
    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let _guard = TempFile(wav_path.clone());

    let bin = ffmpeg_bin();
    let args = encode_h264_args_with_audio(params, &wav_path, out);
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
                if e.kind() != std::io::ErrorKind::BrokenPipe {
                    drop(stdin);
                    let _ = child.wait();
                    return Err(MediaError::Io(e));
                }
                break;
            }
            written += 1;
        }
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
    use crate::encode::ffmpeg_available;
    use crate::test_util::temp_path;

    #[test]
    fn audio_mux_args_are_the_expected_invocation() {
        let params = EncodeParams { width: 1920, height: 1080, fps: 30.0, crf: 18, preset: "medium".to_string() };
        let args = encode_h264_args_with_audio(&params, Path::new("/tmp/a.wav"), Path::new("/tmp/out.mp4"));
        // Two inputs: the rgba stdin pipe and the audio WAV.
        assert!(args.windows(2).any(|w| w[0] == "-i" && w[1] == "-"), "stdin video input");
        assert!(args.windows(2).any(|w| w[0] == "-i" && w[1] == "/tmp/a.wav"), "audio input");
        // Explicit stream maps select video from input 0, audio from input 1.
        assert!(args.windows(2).any(|w| w[0] == "-map" && w[1] == "0:v:0"), "video map");
        assert!(args.windows(2).any(|w| w[0] == "-map" && w[1] == "1:a:0"), "audio map");
        // AAC audio codec + -shortest so neither stream leaves a tail.
        assert!(args.windows(2).any(|w| w[0] == "-c:a" && w[1] == "aac"), "aac codec");
        assert!(args.contains(&"-shortest".to_string()), "shortest");
        // Video options still present (matches the silent path).
        assert!(args.windows(2).any(|w| w[0] == "-c:v" && w[1] == "libx264"));
        assert!(args.windows(2).any(|w| w[0] == "-s" && w[1] == "1920x1080"));
        assert!(args.windows(2).any(|w| w[0] == "-r" && w[1] == "30"));
        assert_eq!(args.last().unwrap(), "/tmp/out.mp4");
    }

    #[test]
    fn audio_mix_emptiness() {
        assert!(AudioMix::new(vec![], 48_000, 2).is_empty(), "no samples");
        assert!(AudioMix::new(vec![0.0; 4], 0, 2).is_empty(), "zero rate");
        assert!(AudioMix::new(vec![0.0; 4], 48_000, 0).is_empty(), "zero channels");
        assert!(!AudioMix::new(vec![0.1; 4], 48_000, 2).is_empty(), "valid mix");
    }

    #[test]
    fn wav_bytes_header_is_canonical_float_wave() {
        // 2 stereo frames (4 samples) @ 48k → 44-byte header + 16 data bytes.
        let mix = AudioMix::new(vec![0.0, 1.0, -1.0, 0.5], 48_000, 2);
        let bytes = wav_bytes(&mix);
        assert_eq!(bytes.len(), 44 + 16, "44-byte header + 4*4 data bytes");
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(&bytes[12..16], b"fmt ");
        // fmt chunk size 16.
        assert_eq!(u32::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]), 16);
        // audio format 3 (IEEE float), 2 channels.
        assert_eq!(u16::from_le_bytes([bytes[20], bytes[21]]), 3);
        assert_eq!(u16::from_le_bytes([bytes[22], bytes[23]]), 2);
        // sample rate 48000.
        assert_eq!(u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]), 48_000);
        // bits per sample 32.
        assert_eq!(u16::from_le_bytes([bytes[34], bytes[35]]), 32);
        // data chunk header + length.
        assert_eq!(&bytes[36..40], b"data");
        assert_eq!(u32::from_le_bytes([bytes[40], bytes[41], bytes[42], bytes[43]]), 16);
        // First sample round-trips.
        assert_eq!(f32::from_le_bytes([bytes[44], bytes[45], bytes[46], bytes[47]]), 0.0);
    }

    #[test]
    fn encode_with_audio_rejects_zero_geometry() {
        let params = EncodeParams::new(0, 0, 30.0);
        let frames = std::iter::empty::<Vec<u8>>();
        let mix = AudioMix::new(vec![0.0; 100], 48_000, 2);
        let out = temp_path("zero_audio.mp4");
        assert!(matches!(
            encode_h264_with_audio(frames, &params, &mix, &out),
            Err(MediaError::Decode(_))
        ));
    }

    #[test]
    fn encode_with_audio_real_roundtrip_then_probe() {
        if !ffmpeg_available() {
            eprintln!("ffmpeg not available — skipping prism-media audio-mux test");
            return;
        }
        let (w, h, fps) = (32u32, 24u32, 10.0f64);
        let params = EncodeParams::new(w, h, fps);
        let out = temp_path("encode_audio.mp4");
        // 10 solid-magenta frames.
        let frames = (0..10).map(move |_| {
            let mut f = Vec::with_capacity((w * h * 4) as usize);
            for _ in 0..(w * h) {
                f.extend_from_slice(&[255, 0, 255, 255]);
            }
            f
        });
        // ~1s of a quiet stereo tone at 48k.
        let rate = 48_000u32;
        let mut samples = Vec::with_capacity(rate as usize * 2);
        for i in 0..rate as usize {
            let s = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / rate as f32).sin() * 0.25;
            samples.push(s);
            samples.push(s);
        }
        let mix = AudioMix::new(samples, rate, 2);
        let n = encode_h264_with_audio(frames, &params, &mix, &out).expect("encode+mux");
        assert_eq!(n, 10);
        let info = crate::probe::probe(&out).expect("probe muxed mp4");
        assert_eq!(info.width, w);
        assert_eq!(info.height, h);
        assert!(info.has_audio, "muxed mp4 must carry an audio stream");
        let _ = std::fs::remove_file(&out);
    }
}
