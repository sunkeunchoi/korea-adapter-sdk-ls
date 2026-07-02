//! One tolerant-numeric parsing seam for the whole adapter (U7, R13).
//!
//! The LS wire carries every numeric as a string (`string_or_number`), often blank
//! for an optional field. Two parsing shapes are needed and the choice between them
//! is a **deliberate, per-call-site decision** (KTD9):
//!
//! - [`lossy_i64`] — the WS hot path. An empty or garbage value decodes to `0` so a
//!   registration-ACK / all-default frame never aborts a reader task. The silent
//!   zero is acceptable there because an all-default row is filtered from emission
//!   upstream (`is_ack`), never turned into a tick.
//! - [`strict_i64`] — the instruments/ingest path. An empty value is still `0` (the
//!   masters leave optional numerics blank), but a non-empty **non-numeric** value is
//!   a **named** [`AdapterError::FieldParse`] rather than a silent zero, so a
//!   malformed master/bar field surfaces as a diagnosable error, never a wrong price.
//!
//! Both accept an integer or a decimal form (truncating toward zero), since the
//! gateway occasionally decorates integer prices with a `.00`. This is the crate's
//! **only** lossy integer parser — every WS/instrument/ingest site routes through
//! here so the silent-zero vs named-error choice is visible at each call.

use crate::error::{AdapterError, AdapterResult};

/// Parse a stringly-typed numeric to `i64`, tolerating blanks and garbage by
/// returning `0` (the WS hot path — KTD9). Empty/whitespace → `0`; an integer or a
/// truncating decimal → its value; anything else → `0`.
pub fn lossy_i64(s: &str) -> i64 {
    let t = s.trim();
    if t.is_empty() {
        0
    } else if let Ok(i) = t.parse::<i64>() {
        i
    } else if let Ok(f) = t.parse::<f64>() {
        f.trunc() as i64
    } else {
        0
    }
}

/// Parse a stringly-typed numeric to `i64`, naming a malformed value as an error
/// (the instruments/ingest path — KTD9).
///
/// Empty/whitespace resolves to `0` (optional master/bar numerics are left blank).
/// A non-empty integer or truncating decimal parses to its value. A non-empty
/// non-numeric value is a [`AdapterError::FieldParse`] carrying `field` and the raw
/// value — never a silent zero.
///
/// # Errors
///
/// [`AdapterError::FieldParse`] if `value` is non-empty and not an integer/decimal.
pub fn strict_i64(field: &str, value: &str) -> AdapterResult<i64> {
    let v = value.trim();
    if v.is_empty() {
        return Ok(0);
    }
    if let Ok(i) = v.parse::<i64>() {
        return Ok(i);
    }
    if let Ok(f) = v.parse::<f64>() {
        return Ok(f.trunc() as i64);
    }
    Err(AdapterError::FieldParse {
        field: field.to_string(),
        value: value.to_string(),
        reason: "expected an integer amount".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lossy_tolerates_blanks_and_garbage() {
        assert_eq!(lossy_i64(""), 0);
        assert_eq!(lossy_i64("   "), 0);
        assert_eq!(lossy_i64("60500"), 60_500);
        assert_eq!(lossy_i64("  42 "), 42);
        // Decimal truncates toward zero (gateway `.00` decoration).
        assert_eq!(lossy_i64("60500.99"), 60_500);
        // Garbage is a silent zero on the hot path.
        assert_eq!(lossy_i64("N/A"), 0);
        // Negatives pass through (callers clamp with `.max(0)` where needed).
        assert_eq!(lossy_i64("-5"), -5);
    }

    #[test]
    fn strict_blank_is_zero_but_garbage_is_named() {
        assert_eq!(strict_i64("recprice", "").unwrap(), 0);
        assert_eq!(strict_i64("recprice", "  ").unwrap(), 0);
        assert_eq!(strict_i64("recprice", "60000").unwrap(), 60_000);
        // Decimal truncates.
        assert_eq!(strict_i64("open", "105.00").unwrap(), 105);
        // Garbage is a NAMED error, not a zero.
        let err = strict_i64("recprice", "not-a-number").unwrap_err();
        match err {
            AdapterError::FieldParse { field, value, .. } => {
                assert_eq!(field, "recprice");
                assert_eq!(value, "not-a-number");
            }
            other => panic!("expected FieldParse, got {other:?}"),
        }
    }
}
