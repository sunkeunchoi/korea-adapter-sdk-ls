//! Adapter-owned credential scrubbing for the bin targets.
//!
//! `ls-sdk-test-support`'s `scrub_secrets` is a dev-dependency, unavailable to bin
//! targets, so this mirrors the repo convention (`crates/ls-sdk-test-support/src/
//! secrets.rs`): mask account numbers and bearer tokens/appkeys out of any text
//! before it can reach stdout or a panic message. [`install`] wires a scrubbing
//! panic hook and relies on the SDK's tracing being silent by default (no
//! subscriber installed ⇒ dispatch spans — which echo the bearer token in ACK
//! frames — never surface).

/// Mask account- and secret-like tokens out of `s`.
///
/// Masks any maximal `[A-Za-z0-9-]` token that either (a) contains a 6+
/// consecutive-digit substring (an account number, with or without a `-NN`
/// product suffix) or (b) is 20+ alphanumeric chars (a bearer token / appkey).
/// Short numbers (quantities, prices, <6-digit order numbers) survive so a loud
/// failure can still name what it reports.
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

/// Install credential-safety hooks at binary startup, before any output.
///
/// Wraps the current panic hook so a panic payload is scrubbed before it reaches
/// stderr. The SDK's tracing dispatch spans stay silent because no tracing
/// subscriber is installed (tracing is a no-op without one) — so the bearer token
/// echoed in registration-ACK frames never reaches a log sink.
pub fn install() {
    // Deliberately NOT chained: chaining to the default hook would re-print the
    // unscrubbed payload after we print the scrubbed one. Taking it here still
    // replaces the default hook so only our scrubbed line reaches stderr.
    let _previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let scrubbed_location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_default();
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "<non-string panic payload>".to_string());
        eprintln!(
            "panic at {scrubbed_location}: {}",
            scrub_secrets(&payload)
        );
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_account_and_token_keeps_order_numbers() {
        assert_eq!(scrub_secrets("acct=20187511401 ok"), "acct=*** ok");
        assert_eq!(scrub_secrets("acct=20187511401-01 ok"), "acct=*** ok");
        let tok = "abcdefghijklmnopqrstuvwx1234";
        assert_eq!(scrub_secrets(&format!("tok={tok}")), "tok=***");
        assert_eq!(scrub_secrets("ordno=12345 qty=10"), "ordno=12345 qty=10");
    }

    #[test]
    fn install_is_idempotent_and_safe() {
        // Installing twice must not panic.
        install();
        install();
    }
}
