//! Pure numeric-parse helpers for typeable inspector fields.
//!
//! Inspector numeric inputs (speed %, opacity %, audio gain dB, scale, position,
//! transition duration) are rendered as [`prism_ui::TextField`]s. When the user
//! presses Enter the typed string must be parsed → clamped → mapped to a
//! `Set*` [`Action`](crate::app_state::Action). These helpers do the *parsing*
//! half: they tolerate the unit suffix the field displays (e.g. `"85%"`,
//! `"-6 dB"`, `"1.5x"`, `"120 px"`), accept a leading `+`, ignore surrounding
//! whitespace, and clamp into a valid range. They are deliberately pure (no GPUI,
//! no `App`) so they're trivially unit-testable.
//!
//! Each returns `None` when the string has no parseable number, so the caller's
//! `make_action` can return `None` ("ignore this submission") and leave state
//! untouched rather than snapping the value to a clamp bound.

/// Strip a trailing unit/symbol suffix and surrounding whitespace, then parse a
/// leading signed decimal number. Tolerates a `+` sign, commas as thousands
/// separators, and any trailing non-numeric characters (the unit). Returns
/// `None` when no number is present.
///
/// Examples that parse to `Some(..)`: `"85%"`, `" -6 dB "`, `"1.5x"`, `"+120"`,
/// `"100 px"`, `"1,000"`. `"abc"` / `""` → `None`.
pub fn parse_number(s: &str) -> Option<f32> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    // Keep the leading sign, digits, one decimal point; drop commas and stop at
    // the first character that can't be part of the number (the unit suffix).
    let mut out = String::with_capacity(s.len());
    let mut seen_dot = false;
    let mut seen_digit = false;
    for (i, c) in s.chars().enumerate() {
        match c {
            '+' | '-' if i == 0 => out.push(c),
            ',' | '_' => {} // thousands / readability separators — skip
            '0'..='9' => {
                out.push(c);
                seen_digit = true;
            }
            '.' if !seen_dot => {
                out.push(c);
                seen_dot = true;
            }
            // Once we've started collecting digits, any other char ends the
            // number (e.g. the unit). Before any digit (other than a sign),
            // bail — leading garbage means "not a number".
            _ => {
                if seen_digit {
                    break;
                }
                // Allow whitespace between a sign and the first digit ("- 6").
                if c.is_whitespace() {
                    continue;
                }
                return None;
            }
        }
    }
    if !seen_digit {
        return None;
    }
    out.parse::<f32>().ok().filter(|v| v.is_finite())
}

/// Parse a number and clamp it into `[lo, hi]`. `None` when unparseable.
pub fn parse_clamped(s: &str, lo: f32, hi: f32) -> Option<f32> {
    parse_number(s).map(|v| v.clamp(lo, hi))
}

/// Parse a percentage field (the displayed value already *is* the percent, e.g.
/// `"85%"` → `85.0`) and clamp into `[lo_pct, hi_pct]`. Returned value is in
/// percent units (the speed/opacity actions decide how to scale it).
pub fn parse_percent(s: &str, lo_pct: f32, hi_pct: f32) -> Option<f32> {
    parse_clamped(s, lo_pct, hi_pct)
}

/// Parse a decibel field (`"-6 dB"` → `-6.0`) and convert to a linear gain
/// multiplier clamped into `[0, max_gain]`. `0 dB` → `1.0`. `None` when
/// unparseable.
pub fn parse_db_to_gain(s: &str, max_gain: f32) -> Option<f32> {
    let db = parse_number(s)?;
    // 10^(dB/20) is the amplitude (voltage) gain. Clamp the *linear* result.
    let gain = 10f32.powf(db / 20.0);
    Some(gain.clamp(0.0, max_gain))
}

/// Inverse of [`parse_db_to_gain`]: format a linear gain as a dB string for the
/// field's initial value. Silence (`gain <= 0`) renders as `-inf dB`.
pub fn gain_to_db_string(gain: f32) -> String {
    if gain <= 0.0 {
        return "-inf dB".to_string();
    }
    let db = 20.0 * gain.log10();
    format!("{db:.1} dB")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn plain_integer() {
        assert_eq!(parse_number("100"), Some(100.0));
    }

    #[test]
    fn decimal_and_sign() {
        assert!(approx(parse_number("1.5").unwrap(), 1.5));
        assert!(approx(parse_number("+2.25").unwrap(), 2.25));
        assert!(approx(parse_number("-6").unwrap(), -6.0));
    }

    #[test]
    fn tolerates_unit_suffixes() {
        assert!(approx(parse_number("85%").unwrap(), 85.0));
        assert!(approx(parse_number("-6 dB").unwrap(), -6.0));
        assert!(approx(parse_number("1.5x").unwrap(), 1.5));
        assert!(approx(parse_number("120 px").unwrap(), 120.0));
        assert!(approx(parse_number("  42 ").unwrap(), 42.0));
    }

    #[test]
    fn tolerates_separators() {
        assert!(approx(parse_number("1,000").unwrap(), 1000.0));
        assert!(approx(parse_number("1_000.5").unwrap(), 1000.5));
    }

    #[test]
    fn sign_with_space_before_digit() {
        assert!(approx(parse_number("- 6").unwrap(), -6.0));
    }

    #[test]
    fn rejects_non_numbers() {
        assert_eq!(parse_number(""), None);
        assert_eq!(parse_number("   "), None);
        assert_eq!(parse_number("abc"), None);
        assert_eq!(parse_number("%"), None);
        assert_eq!(parse_number("--5"), None);
    }

    #[test]
    fn clamps_into_range() {
        assert_eq!(parse_clamped("9999", 1.0, 1000.0), Some(1000.0));
        assert_eq!(parse_clamped("-5", 0.0, 100.0), Some(0.0));
        assert_eq!(parse_clamped("50", 0.0, 100.0), Some(50.0));
        assert_eq!(parse_clamped("nope", 0.0, 100.0), None);
    }

    #[test]
    fn percent_passthrough_and_clamp() {
        assert_eq!(parse_percent("85%", 1.0, 1000.0), Some(85.0));
        assert_eq!(parse_percent("0%", 1.0, 1000.0), Some(1.0)); // clamps up to lo
        assert_eq!(parse_percent("5000%", 1.0, 1000.0), Some(1000.0));
    }

    #[test]
    fn db_zero_is_unity_gain() {
        assert!(approx(parse_db_to_gain("0 dB", 4.0).unwrap(), 1.0));
        assert!(approx(parse_db_to_gain("0", 4.0).unwrap(), 1.0));
    }

    #[test]
    fn db_minus_six_is_about_half() {
        // -6.02 dB is exactly 0.5; -6 dB ≈ 0.501.
        let g = parse_db_to_gain("-6 dB", 4.0).unwrap();
        assert!(approx(g, 0.5012), "got {g}");
    }

    #[test]
    fn db_plus_six_doubles_then_clamps() {
        // +6 dB ≈ 2.0×.
        let g = parse_db_to_gain("+6", 4.0).unwrap();
        assert!(approx(g, 1.995), "got {g}");
        // A huge boost clamps to max_gain.
        assert_eq!(parse_db_to_gain("100 dB", 2.0), Some(2.0));
    }

    #[test]
    fn db_unparseable_is_none() {
        assert_eq!(parse_db_to_gain("loud", 4.0), None);
    }

    #[test]
    fn gain_db_round_trips() {
        // 1.0 → 0.0 dB → 1.0
        let s = gain_to_db_string(1.0);
        assert!(s.starts_with("0.0"), "got {s}");
        let back = parse_db_to_gain(&s, 4.0).unwrap();
        assert!(approx(back, 1.0));
    }

    #[test]
    fn gain_db_silence_is_neg_inf() {
        assert_eq!(gain_to_db_string(0.0), "-inf dB");
        // And -inf string is unparseable → None (caller ignores it).
        assert_eq!(parse_db_to_gain("-inf dB", 4.0), None);
    }
}
