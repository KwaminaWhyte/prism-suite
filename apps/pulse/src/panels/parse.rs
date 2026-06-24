//! Tiny pure parsers for turning typed text into clamped numeric values.
//!
//! Used by the typeable numeric inputs (comp settings, transform values, effect
//! scalar params) so a user can type a value into a `prism_ui::TextField` and
//! have it parsed + clamped to the property's legal range before it is
//! dispatched as the matching `Set*` action. Keeping these as free functions
//! with their own tests means the parse/clamp logic is exercised without a GPUI
//! window.

use std::ops::RangeInclusive;

/// Parse `text` as an `f32` and clamp it into `range`. Leading/trailing
/// whitespace and a single trailing unit suffix (`px`, `°`, `x`, `%`, `s`) are
/// tolerated so the value the field *displays* (e.g. `"120.0 px"`) round-trips
/// back through the parser. Returns `None` when no number can be extracted.
pub fn parse_f32_clamped(text: &str, range: RangeInclusive<f32>) -> Option<f32> {
    let n = parse_f32(text)?;
    if !n.is_finite() {
        return None;
    }
    Some(n.clamp(*range.start(), *range.end()))
}

/// Parse `text` as a non-negative `u32`, clamped into `[min, max]`. Tolerates a
/// trailing unit suffix (e.g. `"1920px"`) and a fractional input (truncated).
/// Returns `None` when no number can be extracted.
pub fn parse_u32_clamped(text: &str, min: u32, max: u32) -> Option<u32> {
    let n = parse_f32(text)?;
    if !n.is_finite() || n < 0.0 {
        // A negative dimension is meaningless; clamp to the floor instead of
        // rejecting outright so typing `-5` snaps to the minimum.
        return Some(min);
    }
    let v = n.trunc() as i64;
    Some(v.clamp(min as i64, max as i64) as u32)
}

/// Extract the leading numeric portion of `text`, ignoring surrounding
/// whitespace and any trailing non-numeric unit characters. Accepts a leading
/// sign, digits, one decimal point, and scientific notation.
fn parse_f32(text: &str) -> Option<f32> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Take the longest prefix that still parses as a float. This lets
    // "120.0 px", "45°", "1.5x", "80%" all yield their number.
    let mut end = 0;
    let bytes = trimmed.as_bytes();
    let mut seen_dot = false;
    let mut seen_e = false;
    for (i, &b) in bytes.iter().enumerate() {
        let c = b as char;
        let ok = match c {
            '0'..='9' => true,
            '+' | '-' => i == 0 || matches!(bytes[i - 1] as char, 'e' | 'E'),
            '.' if !seen_dot && !seen_e => {
                seen_dot = true;
                true
            }
            'e' | 'E' if !seen_e && i > 0 => {
                seen_e = true;
                true
            }
            _ => false,
        };
        if ok {
            end = i + 1;
        } else {
            break;
        }
    }
    trimmed.get(..end)?.parse::<f32>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_floats() {
        assert_eq!(parse_f32_clamped("120", 0.0..=1000.0), Some(120.0));
        assert_eq!(parse_f32_clamped("  45.5 ", -360.0..=360.0), Some(45.5));
        assert_eq!(parse_f32_clamped("-12.25", -100.0..=100.0), Some(-12.25));
    }

    #[test]
    fn tolerates_unit_suffixes() {
        assert_eq!(parse_f32_clamped("120.0 px", 0.0..=2000.0), Some(120.0));
        assert_eq!(parse_f32_clamped("45°", -360.0..=360.0), Some(45.0));
        assert_eq!(parse_f32_clamped("1.5x", 0.0..=5.0), Some(1.5));
        assert_eq!(parse_f32_clamped("80%", 0.0..=100.0), Some(80.0));
        assert_eq!(parse_f32_clamped("2.0s", 0.0..=60.0), Some(2.0));
    }

    #[test]
    fn clamps_to_range() {
        assert_eq!(parse_f32_clamped("9999", 0.0..=100.0), Some(100.0));
        assert_eq!(parse_f32_clamped("-9999", 0.0..=100.0), Some(0.0));
        assert_eq!(parse_f32_clamped("0.5", 1.0..=5.0), Some(1.0));
    }

    #[test]
    fn rejects_non_numbers() {
        assert_eq!(parse_f32_clamped("", 0.0..=1.0), None);
        assert_eq!(parse_f32_clamped("   ", 0.0..=1.0), None);
        assert_eq!(parse_f32_clamped("px", 0.0..=1.0), None);
        assert_eq!(parse_f32_clamped("abc", 0.0..=1.0), None);
    }

    #[test]
    fn rejects_non_finite() {
        assert_eq!(parse_f32_clamped("inf", 0.0..=1.0), None);
        assert_eq!(parse_f32_clamped("NaN", 0.0..=1.0), None);
    }

    #[test]
    fn scientific_notation() {
        assert_eq!(parse_f32_clamped("1e2", 0.0..=1000.0), Some(100.0));
        assert_eq!(parse_f32_clamped("1.5e1", 0.0..=1000.0), Some(15.0));
    }

    #[test]
    fn u32_parses_and_clamps() {
        assert_eq!(parse_u32_clamped("1920", 1, 8192), Some(1920));
        assert_eq!(parse_u32_clamped("1080px", 1, 8192), Some(1080));
        assert_eq!(parse_u32_clamped("0", 1, 8192), Some(1)); // below min -> min
        assert_eq!(parse_u32_clamped("99999", 1, 8192), Some(8192)); // above max -> max
    }

    #[test]
    fn u32_truncates_fraction_and_floors_negatives() {
        assert_eq!(parse_u32_clamped("1920.7", 1, 8192), Some(1920));
        assert_eq!(parse_u32_clamped("-5", 1, 8192), Some(1));
    }

    #[test]
    fn u32_rejects_empty() {
        assert_eq!(parse_u32_clamped("", 1, 8192), None);
        assert_eq!(parse_u32_clamped("abc", 1, 8192), None);
    }
}
