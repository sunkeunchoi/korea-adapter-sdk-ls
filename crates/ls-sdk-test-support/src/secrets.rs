//! Shared credential-safety helpers for the paper live smokes.
//!
//! Hoisted here (U2 of the all-lane closed-window flip wave) so both the
//! autonomous order smoke (`crates/ls-sdk/tests/order_smoke.rs`) and the
//! read/realtime live smokes (`crates/ls-sdk/tests/live_smoke.rs`) reuse ONE
//! scrubber — sibling test binaries cannot import each other's private fns.
//!
//! [`scrub_secrets`] masks account numbers and bearer tokens out of untrusted
//! broker text before it can reach a recorded evidence line or a panic message.
//! [`assert_nonempty_witness`] is the R4/KTD4 flip gate: a substantive modeled
//! field, never `body_len`/`00136`/an all-zero row, is what proves a flip.

/// Mask account- and secret-like tokens out of `s`.
///
/// Masks any maximal `[A-Za-z0-9-]` token that either (a) contains a 6+
/// consecutive-digit substring — an account number, with or without a `-NN`
/// product suffix (the suffix is inside the same token, so it is masked too), or
/// (b) is 20+ alphanumeric chars long — a bearer token / appkey. Short numbers
/// (quantities, prices) and order numbers (<6 digits, no suffix) SURVIVE, so a
/// loud failure can still name the order it is reporting.
///
/// NOTE: an 8-digit `YYYYMMDD` date trips the 6+-digit rule and is masked. Pass
/// dates hyphenated (`%Y-%m-%d`) into recorded lines, or treat the mask as
/// acceptable for untrusted broker free-text where a date is not load-bearing.
pub fn scrub_secrets(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut run = String::new();
    let flush = |out: &mut String, run: &mut String| {
        if run_is_sensitive(run) {
            out.push_str("***");
        } else {
            out.push_str(run);
        }
        run.clear();
    };
    for c in s.chars() {
        if c.is_ascii_alphanumeric() || c == '-' {
            run.push(c);
        } else {
            flush(&mut out, &mut run);
            out.push(c);
        }
    }
    flush(&mut out, &mut run);
    out
}

/// `true` if a `[A-Za-z0-9-]` token is account- or secret-like: a 6+ consecutive
/// digit run (account number) or a 20+ alphanumeric token (bearer token / appkey).
pub fn run_is_sensitive(run: &str) -> bool {
    let mut digits = 0usize;
    for c in run.chars() {
        if c.is_ascii_digit() {
            digits += 1;
            if digits >= 6 {
                return true;
            }
        } else {
            digits = 0;
        }
    }
    run.chars().filter(|c| c.is_ascii_alphanumeric()).count() >= 20
}

/// The R4/KTD4 non-empty-witness flip gate. A REST read flips ONLY when a
/// substantive modeled field holds a real value — never `body_len`, a bare
/// `00000`, an all-default/all-zero row, or `00136`. Returns `Ok(())` when
/// `witness` is non-empty and not the `"0"` zero-default; otherwise `Err` with a
/// scrubbed reason suitable for a PENDING record or a panic.
///
/// `field` is the modeled field's name (e.g. `"close"`), `witness` its decoded
/// string value (`string_or_number` fields decode to `String`).
pub fn assert_nonempty_witness(field: &str, witness: &str) -> Result<(), String> {
    let w = witness.trim();
    if w.is_empty() || w == "0" || w == "0.0" || w == "0.00" {
        return Err(format!(
            "witness field '{field}' is default/empty ('{}') — not a flip (PENDING)",
            scrub_secrets(w)
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_account_with_suffix_and_tokens_keeps_order_numbers() {
        // 11-digit account, with and without product suffix → masked.
        assert_eq!(scrub_secrets("acct=20187511401 ok"), "acct=*** ok");
        assert_eq!(scrub_secrets("acct=20187511401-01 ok"), "acct=*** ok");
        // 20+ char bearer token → masked.
        let tok = "abcdefghijklmnopqrstuvwx1234";
        assert_eq!(scrub_secrets(&format!("tok={tok}")), "tok=***");
        // Short order number (<6 digits) and quantity SURVIVE.
        assert_eq!(scrub_secrets("ordno=12345 qty=10"), "ordno=12345 qty=10");
    }

    #[test]
    fn witness_rejects_default_accepts_substantive() {
        assert!(assert_nonempty_witness("close", "").is_err());
        assert!(assert_nonempty_witness("close", "0").is_err());
        assert!(assert_nonempty_witness("close", "0.00").is_err());
        assert!(assert_nonempty_witness("close", "23145.50").is_ok());
        assert!(assert_nonempty_witness("name", "NASDAQ").is_ok());
    }
}
