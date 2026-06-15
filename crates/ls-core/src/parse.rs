//! Ergonomic numeric string parsing helpers for LS SDK wire values.
//!
//! Korean broker APIs return prices, quantities, and rates as strings.
//! These helpers convert them to [`Decimal`] with fixed-precision arithmetic,
//! avoiding `f64` rounding errors.
//!
//! # Example
//! ```
//! use ls_core::{parse, LsResult};
//!
//! # fn main() -> LsResult<()> {
//! let price = parse::price("12345")?;
//! assert_eq!(price, rust_decimal::Decimal::from(12345));
//! # Ok(())
//! # }
//! ```

pub use rust_decimal::Decimal;

use std::borrow::Cow;

use crate::error::{LsError, LsResult};

/// Strip comma separators, borrowing the input when there are none.
/// Only the comma-formatted path (the rarer case) allocates.
fn strip_commas(s: &str) -> Cow<'_, str> {
    if s.contains(',') {
        Cow::Owned(s.replace(',', ""))
    } else {
        Cow::Borrowed(s)
    }
}

/// Parse a price string into a [`Decimal`].
///
/// Strips comma separators automatically. Rejects signed values.
///
/// # Errors
///
/// Returns [`LsError::Parse`] for empty strings, malformed input, or leading signs.
pub fn price(s: &str) -> LsResult<Decimal> {
    parse_unsigned(s, "price")
}

/// Parse a quantity string into a [`Decimal`].
///
/// Strips comma separators automatically. Rejects signed values.
///
/// # Errors
///
/// Returns [`LsError::Parse`] for empty strings, malformed input, or leading signs.
pub fn quantity(s: &str) -> LsResult<Decimal> {
    parse_unsigned(s, "quantity")
}

/// Parse a rate string into a [`Decimal`].
///
/// Strips comma separators automatically. Rejects signed values.
///
/// # Errors
///
/// Returns [`LsError::Parse`] for empty strings, malformed input, or leading signs.
pub fn rate(s: &str) -> LsResult<Decimal> {
    parse_unsigned(s, "rate")
}

/// Parse a signed delta string into a [`Decimal`].
///
/// Strips comma separators automatically. Accepts leading `+` or `-`.
///
/// # Errors
///
/// Returns [`LsError::Parse`] for empty strings or malformed input.
pub fn signed_delta(s: &str) -> LsResult<Decimal> {
    let cleaned = strip_commas(s);
    if cleaned.is_empty() {
        return Err(LsError::Parse("empty signed_delta string".to_string()));
    }
    Decimal::from_str_exact(&cleaned)
        .map_err(|e| LsError::Parse(format!("invalid signed_delta '{s}': {e}")))
}

fn parse_unsigned(s: &str, kind: &str) -> LsResult<Decimal> {
    let cleaned = strip_commas(s);
    if cleaned.is_empty() {
        return Err(LsError::Parse(format!("empty {kind} string")));
    }
    if cleaned.starts_with('+') || cleaned.starts_with('-') {
        return Err(LsError::Parse(format!("{kind} must be unsigned: '{s}'")));
    }
    Decimal::from_str_exact(&cleaned)
        .map_err(|e| LsError::Parse(format!("invalid {kind} '{s}': {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn price_valid_integer() {
        assert_eq!(price("12345").unwrap(), Decimal::from(12345));
    }

    #[test]
    fn price_valid_decimal() {
        assert_eq!(
            price("1234.56").unwrap(),
            Decimal::from_str_exact("1234.56").unwrap()
        );
    }

    #[test]
    fn price_with_commas() {
        assert_eq!(
            price("1,234.56").unwrap(),
            Decimal::from_str_exact("1234.56").unwrap()
        );
    }

    #[test]
    fn price_empty_fails() {
        let err = price("").unwrap_err();
        assert!(matches!(err, LsError::Parse(_)));
    }

    #[test]
    fn price_malformed_fails() {
        let err = price("abc").unwrap_err();
        assert!(matches!(err, LsError::Parse(_)));
    }

    #[test]
    fn price_rejects_sign() {
        let err = price("-123").unwrap_err();
        assert!(matches!(err, LsError::Parse(_)));
    }

    #[test]
    fn quantity_valid() {
        assert_eq!(quantity("100").unwrap(), Decimal::from(100));
    }

    #[test]
    fn rate_valid() {
        assert_eq!(
            rate("0.05").unwrap(),
            Decimal::from_str_exact("0.05").unwrap()
        );
    }

    #[test]
    fn signed_delta_positive() {
        assert_eq!(
            signed_delta("+123.45").unwrap(),
            Decimal::from_str_exact("123.45").unwrap()
        );
    }

    #[test]
    fn signed_delta_negative() {
        assert_eq!(
            signed_delta("-123.45").unwrap(),
            Decimal::from_str_exact("-123.45").unwrap()
        );
    }

    #[test]
    fn price_spec_sample_comma_formatted() {
        // Grounded in a real spec sample: `"itemprice": "127,000"` appears
        // verbatim in specs/ls_openapi_specs.json — guards against synthetic
        // data masking wrong field semantics.
        assert_eq!(price("127,000").unwrap(), Decimal::from(127_000));
    }

    #[test]
    fn signed_delta_with_commas() {
        assert_eq!(
            signed_delta("-1,234.56").unwrap(),
            Decimal::from_str_exact("-1234.56").unwrap()
        );
    }

    #[test]
    fn signed_delta_empty_fails() {
        let err = signed_delta("").unwrap_err();
        assert!(matches!(err, LsError::Parse(_)));
    }

    #[test]
    fn precision_many_fractional_digits() {
        let d = price("0.1234567890123456789012345678").unwrap();
        assert_eq!(
            d,
            Decimal::from_str_exact("0.1234567890123456789012345678").unwrap()
        );
    }
}
