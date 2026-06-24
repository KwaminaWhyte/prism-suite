//! Pure number-parsing helpers shared by the typeable numeric `TextField`s
//! (tool-options brush params, the New / Image Size dialog, the free-transform
//! rotation/skew/scale entries).
//!
//! These are deliberately tolerant: they strip a single trailing unit suffix
//! (`px`, `%`, `°`, `deg`, `x`/`×`) and surrounding whitespace so a user can
//! type "200px" or "75%" into the same field that displays those formats. They
//! never panic and return `None` on anything that isn't a finite number, so the
//! caller can ignore a malformed entry (leaving the model unchanged).
//!
//! All logic is pure (no GPUI types) so it is unit-tested in isolation.

/// Strip surrounding whitespace and a single known trailing unit, returning the
/// bare numeric core. Recognised units (case-insensitive): `px`, `%`, `deg`,
/// `°`, `x`, `×`. A leading `+` is also dropped (so "+45" parses).
fn strip_units(input: &str) -> &str {
    let mut s = input.trim();
    s = s.strip_prefix('+').unwrap_or(s);
    // Order matters: try the longer suffixes first.
    for unit in ["px", "deg", "°", "%", "x", "×", "X"] {
        if let Some(stripped) = s.strip_suffix(unit) {
            s = stripped.trim_end();
            break;
        }
    }
    s.trim()
}

/// Parse a free-form numeric entry into an `f32`, tolerating a trailing unit.
/// Returns `None` for empty / non-numeric / non-finite input.
pub fn parse_f32(input: &str) -> Option<f32> {
    let core = strip_units(input);
    if core.is_empty() {
        return None;
    }
    let v: f32 = core.parse().ok()?;
    v.is_finite().then_some(v)
}

/// Parse an entry that the UI shows as a percentage (e.g. brush opacity shown as
/// "75%") back into a 0..1 fraction. "75%" or "75" → 0.75; "0.75" is detected as
/// already-fractional only when no `%` is present AND the value is ≤ 1, so both
/// "75%" and "0.75" round-trip. Returns `None` on malformed input.
pub fn parse_percent_fraction(input: &str) -> Option<f32> {
    let trimmed = input.trim();
    let had_percent = trimmed.contains('%');
    let v = parse_f32(trimmed)?;
    let frac = if had_percent || v > 1.0 { v / 100.0 } else { v };
    Some(frac)
}

/// Parse a positive pixel dimension (width/height/resolution) into a `u32`,
/// rounding to the nearest whole pixel and clamping the minimum to 1. Returns
/// `None` for non-numeric, non-positive, or non-finite input.
pub fn parse_dimension(input: &str) -> Option<u32> {
    let v = parse_f32(input)?;
    if v < 0.5 {
        return None;
    }
    Some(v.round() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_plain_f32() {
        assert_eq!(parse_f32("42"), Some(42.0));
        assert_eq!(parse_f32("  3.5  "), Some(3.5));
        assert_eq!(parse_f32("-7.25"), Some(-7.25));
        assert_eq!(parse_f32("+45"), Some(45.0));
    }

    #[test]
    fn parse_f32_strips_units() {
        assert_eq!(parse_f32("200px"), Some(200.0));
        assert_eq!(parse_f32("75%"), Some(75.0));
        assert_eq!(parse_f32("90°"), Some(90.0));
        assert_eq!(parse_f32("30 deg"), Some(30.0));
        assert_eq!(parse_f32("2x"), Some(2.0));
        assert_eq!(parse_f32("1.5×"), Some(1.5));
    }

    #[test]
    fn parse_f32_rejects_junk() {
        assert_eq!(parse_f32(""), None);
        assert_eq!(parse_f32("   "), None);
        assert_eq!(parse_f32("abc"), None);
        assert_eq!(parse_f32("px"), None);
        assert_eq!(parse_f32("inf"), None); // non-finite rejected
        assert_eq!(parse_f32("NaN"), None);
    }

    #[test]
    fn percent_fraction_both_forms() {
        // Explicit percent sign.
        assert_eq!(parse_percent_fraction("75%"), Some(0.75));
        // Bare number > 1 treated as a percentage.
        assert_eq!(parse_percent_fraction("75"), Some(0.75));
        // Already-fractional 0..1 stays as-is.
        assert_eq!(parse_percent_fraction("0.5"), Some(0.5));
        assert_eq!(parse_percent_fraction("1"), Some(1.0));
        // 100% → 1.0
        assert_eq!(parse_percent_fraction("100%"), Some(1.0));
    }

    #[test]
    fn percent_fraction_rejects_junk() {
        assert_eq!(parse_percent_fraction("nope"), None);
        assert_eq!(parse_percent_fraction(""), None);
    }

    #[test]
    fn dimension_rounds_and_clamps() {
        assert_eq!(parse_dimension("1920"), Some(1920));
        assert_eq!(parse_dimension("1920.4"), Some(1920));
        assert_eq!(parse_dimension("1920.6"), Some(1921));
        assert_eq!(parse_dimension("1024px"), Some(1024));
        assert_eq!(parse_dimension("1"), Some(1));
    }

    #[test]
    fn dimension_rejects_nonpositive() {
        assert_eq!(parse_dimension("0"), None);
        assert_eq!(parse_dimension("0.2"), None);
        assert_eq!(parse_dimension("-50"), None);
        assert_eq!(parse_dimension("abc"), None);
        assert_eq!(parse_dimension(""), None);
    }
}
