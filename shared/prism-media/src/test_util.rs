//! Shared test helpers used by the decode / encode / audio test modules. Gated
//! behind `#[cfg(test)]` so it is never part of the compiled crate.

use std::path::Path;
use std::process::Command;

use crate::ffmpeg_bin;

/// Generate a 1s 64x48 @10fps test clip into `path` via FFmpeg's lavfi
/// `testsrc`. Returns `false` (skip) when FFmpeg isn't installed, mirroring
/// the suite's "GPU test skips silently when no adapter" convention.
pub(crate) fn make_clip(path: &Path) -> bool {
    let bin = ffmpeg_bin();
    let status = Command::new(&bin)
        .args([
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=1:size=64x48:rate=10",
            "-pix_fmt",
            "yuv420p",
            "-y",
        ])
        .arg(path)
        .args(["-v", "error"])
        .status();
    match status {
        Ok(s) => s.success(),
        // ffmpeg missing → skip the suite of gated tests.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(_) => false,
    }
}

/// A scratch path under the OS temp dir (process-id-scoped to avoid clashes).
pub(crate) fn temp_path(name: &str) -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("prism_media_{}_{name}", std::process::id()));
    p
}
