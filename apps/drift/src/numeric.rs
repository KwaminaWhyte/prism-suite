//! Pure parsing helpers for typeable numeric inputs.
//!
//! The Drift inspector / properties UI lets the user type raw text into numeric
//! fields (layer transform, keyframe value, document settings). These helpers
//! convert that free text into the clamped, typed values the corresponding
//! `Action::Set*` variants expect. They are intentionally GPUI-free and fully
//! unit-tested so the parsing rules stay verifiable in isolation from the view.
//!
//! Each helper tolerates trailing units / symbols a user might type (`px`, `%`,
//! `°`, whitespace) and rejects garbage by returning `None`, leaving the caller
//! to keep the previous value rather than clobber state with a `0`.

/// Strip whitespace plus any trailing unit/symbol characters a user commonly
/// types into a numeric field, then parse the remaining text as `f32`.
///
/// Accepts e.g. `"12.5"`, `" 12.5 "`, `"12.5px"`, `"80%"`, `"45°"`, `"-3"`.
/// Returns `None` for empty or non-numeric input.
pub fn parse_f32_loose(input: &str) -> Option<f32> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Keep the leading numeric run: digits, one decimal point, a leading sign,
    // and scientific notation. Drop a trailing unit like `px` / `%` / `°`.
    let numeric: String = trimmed
        .chars()
        .take_while(|c| {
            c.is_ascii_digit() || *c == '.' || *c == '-' || *c == '+' || *c == 'e' || *c == 'E'
        })
        .collect();
    numeric.parse::<f32>().ok().filter(|v| v.is_finite())
}

/// Parse a position / generic transform coordinate. No clamping (positions may
/// be negative or large), but rejects non-finite garbage.
pub fn parse_position(input: &str) -> Option<f32> {
    parse_f32_loose(input)
}

/// Parse a scale factor. Scale must stay strictly positive, so values ≤ 0 are
/// floored to a tiny epsilon (matching the existing inspector behaviour).
pub fn parse_scale(input: &str) -> Option<f32> {
    parse_f32_loose(input).map(|v| v.max(0.001))
}

/// Parse a rotation in degrees. Any finite value is accepted as-is.
pub fn parse_rotation(input: &str) -> Option<f32> {
    parse_f32_loose(input)
}

/// Parse an opacity typed as a 0–100 percentage and return it as a 0–1 fraction,
/// clamped to that range.
pub fn parse_opacity_percent(input: &str) -> Option<f32> {
    parse_f32_loose(input).map(|v| (v / 100.0).clamp(0.0, 1.0))
}

/// Parse a raw keyframe value (no clamping — keyframes can hold any property
/// value, including negatives).
pub fn parse_keyframe_value(input: &str) -> Option<f32> {
    parse_f32_loose(input)
}

/// Parse a frames-per-second value, clamped to the engine-supported 1–120 range
/// (matching `Action::SetDocumentFps`).
pub fn parse_fps(input: &str) -> Option<f32> {
    parse_f32_loose(input).map(|v| v.clamp(1.0, 120.0))
}

/// Parse a positive pixel dimension (document width / height), rounded to the
/// nearest whole pixel and floored at 1. Returns `None` for non-numeric input.
pub fn parse_dimension(input: &str) -> Option<u32> {
    parse_f32_loose(input).map(|v| v.round().max(1.0) as u32)
}

/// Parse a positive frame count (document duration), rounded to the nearest
/// whole frame and floored at 1.
pub fn parse_duration_frames(input: &str) -> Option<usize> {
    parse_f32_loose(input).map(|v| v.round().max(1.0) as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loose_plain_float() {
        assert_eq!(parse_f32_loose("12.5"), Some(12.5));
    }

    #[test]
    fn loose_trims_whitespace() {
        assert_eq!(parse_f32_loose("  -3.0  "), Some(-3.0));
    }

    #[test]
    fn loose_strips_px_unit() {
        assert_eq!(parse_f32_loose("1920px"), Some(1920.0));
    }

    #[test]
    fn loose_strips_percent() {
        assert_eq!(parse_f32_loose("80%"), Some(80.0));
    }

    #[test]
    fn loose_strips_degree() {
        assert_eq!(parse_f32_loose("45°"), Some(45.0));
    }

    #[test]
    fn loose_rejects_empty() {
        assert_eq!(parse_f32_loose("   "), None);
    }

    #[test]
    fn loose_rejects_garbage() {
        assert_eq!(parse_f32_loose("abc"), None);
        assert_eq!(parse_f32_loose("--"), None);
    }

    #[test]
    fn position_allows_negative() {
        assert_eq!(parse_position("-100.5"), Some(-100.5));
    }

    #[test]
    fn scale_floors_at_epsilon() {
        assert_eq!(parse_scale("0"), Some(0.001));
        assert_eq!(parse_scale("-2"), Some(0.001));
        assert_eq!(parse_scale("2.5"), Some(2.5));
    }

    #[test]
    fn rotation_passthrough() {
        assert_eq!(parse_rotation("-720"), Some(-720.0));
    }

    #[test]
    fn opacity_percent_to_fraction() {
        assert_eq!(parse_opacity_percent("100"), Some(1.0));
        assert_eq!(parse_opacity_percent("50"), Some(0.5));
        assert_eq!(parse_opacity_percent("0"), Some(0.0));
    }

    #[test]
    fn opacity_percent_clamps() {
        assert_eq!(parse_opacity_percent("250"), Some(1.0));
        assert_eq!(parse_opacity_percent("-10"), Some(0.0));
    }

    #[test]
    fn keyframe_value_allows_negative() {
        assert_eq!(parse_keyframe_value("-42.5"), Some(-42.5));
    }

    #[test]
    fn fps_clamps_range() {
        assert_eq!(parse_fps("24"), Some(24.0));
        assert_eq!(parse_fps("0.5"), Some(1.0));
        assert_eq!(parse_fps("500"), Some(120.0));
    }

    #[test]
    fn dimension_rounds_and_floors() {
        assert_eq!(parse_dimension("1920"), Some(1920));
        assert_eq!(parse_dimension("1920.4"), Some(1920));
        assert_eq!(parse_dimension("1920.6"), Some(1921));
        assert_eq!(parse_dimension("0"), Some(1));
        assert_eq!(parse_dimension("-5"), Some(1));
    }

    #[test]
    fn dimension_rejects_garbage() {
        assert_eq!(parse_dimension("wide"), None);
    }

    #[test]
    fn duration_rounds_and_floors() {
        assert_eq!(parse_duration_frames("240"), Some(240));
        assert_eq!(parse_duration_frames("240.7"), Some(241));
        assert_eq!(parse_duration_frames("0"), Some(1));
    }
}
