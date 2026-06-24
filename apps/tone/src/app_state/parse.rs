//! Free-text numeric parsing helpers for Tone's typeable inputs.
//!
//! These are pure functions (no `App` access) that turn what a user types into a
//! `prism_ui::TextField` into a clamped, unit-tolerant value ready to feed the
//! matching `Set*` [`Action`](super::Action). Each returns `None` when the input
//! has no parseable number, so a caller can leave the value unchanged on an
//! empty / garbage field rather than zeroing it.
//!
//! Unit tolerance is forgiving on purpose — a producer typing `-6 dB`, `-6db`,
//! or just `-6` into a gain field all mean the same thing. A leading sign is
//! always honoured. Surrounding whitespace is ignored.

/// Strip a case-insensitive unit suffix (e.g. `"db"`, `"st"`, `"%"`) and any
/// surrounding whitespace, returning the trimmed numeric core. The suffix match
/// is case-insensitive; multiple candidate suffixes are tried in order.
fn strip_units(input: &str, suffixes: &[&str]) -> String {
    let mut s = input.trim();
    let lower = s.to_ascii_lowercase();
    for suf in suffixes {
        if lower.ends_with(&suf.to_ascii_lowercase()) {
            s = s[..s.len() - suf.len()].trim_end();
            break;
        }
    }
    s.trim().to_string()
}

/// Parse a per-clip / channel gain expressed in **decibels**.
///
/// Accepts an optional `dB` suffix (case-insensitive) and a leading sign.
/// Clamped to the same `-60.0..=12.0` range enforced by
/// [`Action::SetClipGainDb`](super::Action::SetClipGainDb). Returns `None` for
/// non-numeric input.
pub fn parse_db(input: &str) -> Option<f32> {
    let core = strip_units(input, &["db", "dbfs"]);
    core.parse::<f32>().ok().map(|v| v.clamp(-60.0, 12.0))
}

/// Parse a pitch shift expressed in **semitones**.
///
/// Accepts an optional `st` / `semitones` / `semi` suffix and a leading sign.
/// Clamped to `-24.0..=24.0`, matching
/// [`Action::SetClipPitchF32`](super::Action::SetClipPitchF32). Returns `None`
/// for non-numeric input.
pub fn parse_semitones(input: &str) -> Option<f32> {
    let core = strip_units(input, &["semitones", "semi", "st"]);
    core.parse::<f32>().ok().map(|v| v.clamp(-24.0, 24.0))
}

/// Parse a clip length expressed in **beats**.
///
/// Accepts an optional `beats` / `beat` / `b` suffix. The result is forced to be
/// at least one 1/16th beat (`0.0625`), matching the minimum enforced by
/// [`Action::ResizeClip`](super::Action::ResizeClip). Returns `None` for
/// non-numeric input.
pub fn parse_beats(input: &str) -> Option<f32> {
    let core = strip_units(input, &["beats", "beat", "b"]);
    core.parse::<f32>().ok().map(|v| v.max(0.0625))
}

/// Parse a track/channel volume expressed as a **percentage** of unity gain
/// (100% = 1.0). Returns the linear multiplier.
///
/// Accepts an optional `%` suffix; a bare number is also treated as a percentage
/// (so `100` and `100%` both mean unity). Clamped to the linear `0.0..=2.0`
/// range enforced by [`Action::SetTrackVolume`](super::Action::SetTrackVolume)
/// (i.e. 0%..=200%). Returns `None` for non-numeric input.
pub fn parse_volume_percent(input: &str) -> Option<f32> {
    let core = strip_units(input, &["%"]);
    core.parse::<f32>()
        .ok()
        .map(|pct| (pct / 100.0).clamp(0.0, 2.0))
}

/// Parse a stereo pan position into the `-1.0..=1.0` range used by
/// [`Action::SetTrackPan`](super::Action::SetTrackPan).
///
/// Accepts several producer-friendly spellings:
/// * `C` / `center` / `centre` / `0` → centre (`0.0`)
/// * `L` / `L50` / `50L` / `left` → left (negative); a bare `L` is full-left
/// * `R` / `R50` / `50R` / `right` → right (positive); a bare `R` is full-right
/// * a signed percentage `-50` / `+50%` → `-0.5` / `0.5` (negative = left)
///
/// Percentages are taken as a fraction of full deflection (100% = hard L/R).
/// Returns `None` for non-numeric, non-keyword input.
pub fn parse_pan(input: &str) -> Option<f32> {
    let t = input.trim();
    if t.is_empty() {
        return None;
    }
    let lower = t.to_ascii_lowercase();

    // Whole-word keywords.
    match lower.as_str() {
        "c" | "0" | "center" | "centre" | "mid" => return Some(0.0),
        "l" | "left" => return Some(-1.0),
        "r" | "right" => return Some(1.0),
        _ => {}
    }

    // Side-letter forms: a leading or trailing L/R sets the direction, the rest
    // is a percentage of full deflection. e.g. "L50", "50R", "l 25".
    let (sign, rest) = if let Some(stripped) = lower.strip_prefix('l') {
        (-1.0_f32, stripped)
    } else if let Some(stripped) = lower.strip_prefix('r') {
        (1.0_f32, stripped)
    } else if let Some(stripped) = lower.strip_suffix('l') {
        (-1.0_f32, stripped)
    } else if let Some(stripped) = lower.strip_suffix('r') {
        (1.0_f32, stripped)
    } else {
        // Plain signed percentage: negative = left, positive = right.
        let core = strip_units(t, &["%"]);
        return core.parse::<f32>().ok().map(|pct| (pct / 100.0).clamp(-1.0, 1.0));
    };

    let core = strip_units(rest.trim(), &["%"]);
    let pct = core.parse::<f32>().unwrap_or(100.0).abs();
    Some((sign * pct / 100.0).clamp(-1.0, 1.0))
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn db_plain_and_suffixed() {
        assert_eq!(parse_db("-6"), Some(-6.0));
        assert_eq!(parse_db("-6 dB"), Some(-6.0));
        assert_eq!(parse_db("3db"), Some(3.0));
        assert_eq!(parse_db("  +0 DBFS "), Some(0.0));
    }

    #[test]
    fn db_clamps_to_range() {
        assert_eq!(parse_db("100"), Some(12.0));
        assert_eq!(parse_db("-200"), Some(-60.0));
    }

    #[test]
    fn db_garbage_is_none() {
        assert_eq!(parse_db(""), None);
        assert_eq!(parse_db("loud"), None);
        assert_eq!(parse_db("db"), None);
    }

    #[test]
    fn semitones_plain_and_suffixed() {
        assert_eq!(parse_semitones("7"), Some(7.0));
        assert_eq!(parse_semitones("-5 st"), Some(-5.0));
        assert_eq!(parse_semitones("12 semitones"), Some(12.0));
        assert_eq!(parse_semitones("+3.5semi"), Some(3.5));
    }

    #[test]
    fn semitones_clamps() {
        assert_eq!(parse_semitones("48"), Some(24.0));
        assert_eq!(parse_semitones("-48"), Some(-24.0));
    }

    #[test]
    fn semitones_garbage_is_none() {
        assert_eq!(parse_semitones(""), None);
        assert_eq!(parse_semitones("up"), None);
    }

    #[test]
    fn beats_plain_and_suffixed() {
        assert_eq!(parse_beats("4"), Some(4.0));
        assert_eq!(parse_beats("8 beats"), Some(8.0));
        assert_eq!(parse_beats("2.5b"), Some(2.5));
    }

    #[test]
    fn beats_enforces_minimum() {
        assert_eq!(parse_beats("0"), Some(0.0625));
        assert_eq!(parse_beats("-3"), Some(0.0625));
    }

    #[test]
    fn beats_garbage_is_none() {
        assert_eq!(parse_beats(""), None);
        assert_eq!(parse_beats("long"), None);
    }

    #[test]
    fn volume_percent_plain_and_suffixed() {
        assert_eq!(parse_volume_percent("100"), Some(1.0));
        assert_eq!(parse_volume_percent("100%"), Some(1.0));
        assert_eq!(parse_volume_percent("50 %"), Some(0.5));
        assert_eq!(parse_volume_percent("0"), Some(0.0));
    }

    #[test]
    fn volume_percent_clamps() {
        assert_eq!(parse_volume_percent("300"), Some(2.0));
        assert_eq!(parse_volume_percent("-10"), Some(0.0));
    }

    #[test]
    fn volume_percent_garbage_is_none() {
        assert_eq!(parse_volume_percent(""), None);
        assert_eq!(parse_volume_percent("loud"), None);
    }

    #[test]
    fn pan_keywords() {
        assert_eq!(parse_pan("C"), Some(0.0));
        assert_eq!(parse_pan("center"), Some(0.0));
        assert_eq!(parse_pan("L"), Some(-1.0));
        assert_eq!(parse_pan("right"), Some(1.0));
        assert_eq!(parse_pan("0"), Some(0.0));
    }

    #[test]
    fn pan_side_letter_percent() {
        assert_eq!(parse_pan("L50"), Some(-0.5));
        assert_eq!(parse_pan("50R"), Some(0.5));
        assert_eq!(parse_pan("R 25"), Some(0.25));
        assert_eq!(parse_pan("l100"), Some(-1.0));
    }

    #[test]
    fn pan_signed_percent() {
        assert_eq!(parse_pan("-50"), Some(-0.5));
        assert_eq!(parse_pan("+50%"), Some(0.5));
        assert_eq!(parse_pan("200"), Some(1.0)); // clamps to hard right
    }

    #[test]
    fn pan_garbage_is_none() {
        assert_eq!(parse_pan(""), None);
        assert_eq!(parse_pan("middle-ish"), None);
    }

    #[test]
    fn db_to_gain_roundtrip_via_parse() {
        // -60 dB clamps to the floor; 0 dB → unity; 12 dB is the ceiling.
        assert_eq!(parse_db("0"), Some(0.0));
        let unity = 10f32.powf(parse_db("0").unwrap() / 20.0);
        assert!((unity - 1.0).abs() < 1e-6);
    }
}
