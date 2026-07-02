//! Marketability / daily-band guard for the operator exec tester (U6, R14, KTD8).
//!
//! `node_exec_tester` submits a **resting, non-marketable** limit order. An operator
//! who fat-fingers a price at/above the best ask would fill immediately (defeating
//! the resting-order safety posture); one outside the daily price band would be
//! rejected by the venue. This guard fetches t8450 (통합 주식현재가호가조회2 —
//! `price`, `offerho1` best ask, `uplmtprice`/`dnlmtprice` daily limits) and
//! **refuses before any order is placed** when the operator price is ≥ best ask,
//! outside `[dnlmtprice, uplmtprice]`, or when any band field is unparseable/zero.
//! Fail-closed: an unreadable band refuses (never proceeds on a guess).

use crate::parse::strict_i64;

/// Evaluate an operator resting price against the current best ask + daily band.
/// Returns the parsed price on success, or a human-readable refusal reason.
///
/// Fail-closed (KTD8): a missing ask side (`offerho1` zero/absent), an unparseable
/// band field, or a price at/above the ask or outside the band all refuse.
pub fn check_resting_price(
    price: &str,
    offerho1: &str,
    dnlmtprice: &str,
    uplmtprice: &str,
) -> Result<i64, String> {
    let price = strict_i64("LS_NODE_PRICE", price)
        .map_err(|_| format!("operator price {price:?} is not an integer — refusing"))?;
    let best_ask = strict_i64("offerho1", offerho1)
        .map_err(|_| format!("best ask (offerho1) {offerho1:?} unparseable — refusing (fail-closed)"))?;
    let lower = strict_i64("dnlmtprice", dnlmtprice)
        .map_err(|_| format!("lower limit (dnlmtprice) {dnlmtprice:?} unparseable — refusing"))?;
    let upper = strict_i64("uplmtprice", uplmtprice)
        .map_err(|_| format!("upper limit (uplmtprice) {uplmtprice:?} unparseable — refusing"))?;

    if best_ask <= 0 {
        return Err("no ask side (offerho1 is zero/absent) — cannot prove the price is non-marketable; refusing".to_string());
    }
    if lower <= 0 || upper <= 0 || upper < lower {
        return Err(format!(
            "daily band [{lower}, {upper}] is unusable (zero or inverted) — refusing (fail-closed)"
        ));
    }
    if price >= best_ask {
        return Err(format!(
            "price {price} is at/above the best ask {best_ask} — would fill immediately (not a resting order); refusing"
        ));
    }
    if price < lower || price > upper {
        return Err(format!(
            "price {price} is outside the daily band [{lower}, {upper}] — the venue would reject it; refusing"
        ));
    }
    Ok(price)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Best ask 60,600; daily band [42,000, 78,000].
    const ASK: &str = "60600";
    const LOWER: &str = "42000";
    const UPPER: &str = "78000";

    #[test]
    fn a_valid_resting_price_below_ask_and_in_band_passes() {
        assert_eq!(check_resting_price("60000", ASK, LOWER, UPPER).unwrap(), 60_000);
    }

    #[test]
    fn price_at_or_above_best_ask_refuses() {
        // AE6: at the ask.
        let e = check_resting_price("60600", ASK, LOWER, UPPER).unwrap_err();
        assert!(e.contains("best ask"), "{e}");
        // Above the ask.
        assert!(check_resting_price("60700", ASK, LOWER, UPPER).is_err());
    }

    #[test]
    fn price_below_lower_limit_refuses() {
        let e = check_resting_price("41000", ASK, LOWER, UPPER).unwrap_err();
        assert!(e.contains("daily band"), "{e}");
    }

    #[test]
    fn price_above_upper_limit_refuses() {
        // Above the band but also above the ask; either refusal is fine — must refuse.
        assert!(check_resting_price("79000", ASK, LOWER, UPPER).is_err());
    }

    #[test]
    fn unparseable_band_fields_refuse_fail_closed() {
        assert!(check_resting_price("60000", "N/A", LOWER, UPPER).is_err());
        assert!(check_resting_price("60000", ASK, "oops", UPPER).is_err());
        assert!(check_resting_price("60000", ASK, LOWER, "bad").is_err());
        assert!(check_resting_price("not-a-price", ASK, LOWER, UPPER).is_err());
    }

    #[test]
    fn zero_or_absent_ask_refuses() {
        let e = check_resting_price("60000", "0", LOWER, UPPER).unwrap_err();
        assert!(e.contains("no ask side"), "{e}");
        assert!(check_resting_price("60000", "", LOWER, UPPER).is_err());
    }
}
