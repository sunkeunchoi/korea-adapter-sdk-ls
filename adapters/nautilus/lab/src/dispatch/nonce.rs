//! The operator-nonce gate (KTD5) — a pure, fail-closed authorization for the
//! consequential, per-session operator actions: deferring a deferrable red (U2),
//! clearing a persisted kill-switch trip (U5), and requesting an escalation or a
//! re-registration (U10). Mirrors the order-smoke convention (`crates/ls-sdk/tests/
//! order_smoke.rs`): a fresh unix-seconds nonce minted this wave, a 600s TTL, and a
//! loud refusal in any unattended/no-TTY context (agent shells have no TTY, so a
//! refusal must never look like it ran).

/// The TTL for a per-action human-minted nonce (seconds). A stale nonce degrades to an
/// expired timestamp within minutes and can never re-authorize an action, so a static
/// constant is worthless.
pub const NONCE_TTL_SECS: i64 = 600;

/// Forward-skew tolerance (seconds), so a small clock difference between the operator's
/// shell and the runner does not reject a fresh nonce.
pub const NONCE_MAX_SKEW_SECS: i64 = 60;

/// The gathered inputs for the operator-nonce decision, separated so the fail-closed
/// decision is a pure function offline tests can exercise across every scenario —
/// including no-TTY, which cannot be forced in-process.
#[derive(Debug, Clone)]
pub struct OperatorGate {
    /// `Some(reason)` when an unattended/CI marker is detected (a CI env var, or no TTY).
    pub unattended_marker: Option<String>,
    /// The raw nonce value, if the operator supplied one.
    pub nonce: Option<String>,
    /// The current unix time (seconds) for TTL validation.
    pub now_unix: i64,
}

impl OperatorGate {
    /// Authorize a consequential operator action. Refuses unless BOTH hold: no
    /// unattended/CI marker is present, AND a fresh (within-TTL) unix-seconds nonce was
    /// minted this wave. The active fresh nonce is the human-present signal — passive CI
    /// detection alone cannot distinguish an agent wave from an unmarked headless runner.
    ///
    /// # Errors
    ///
    /// A human-readable refusal reason string on any failing condition.
    pub fn authorize(&self, action: &str) -> Result<(), String> {
        if let Some(reason) = &self.unattended_marker {
            return Err(format!(
                "refusing {action}: detected unattended context ({reason}); this action is bounded \
                 to interactive, operator-present waves"
            ));
        }
        let Some(nonce) = self.nonce.as_deref() else {
            return Err(format!(
                "refusing {action}: operator nonce absent (mint a fresh one: \
                 `export LS_DISPATCH_NONCE=$(date +%s)`)"
            ));
        };
        validate_nonce(action, nonce, self.now_unix)
    }
}

/// Validate a per-action nonce: a fresh unix-seconds timestamp within TTL. A non-numeric
/// value (a static well-known constant) fails to parse; an old value is expired; a
/// far-future value is rejected as implausible skew. So "valid nonce" can never
/// degenerate to "env var present".
pub fn validate_nonce(action: &str, nonce: &str, now_unix: i64) -> Result<(), String> {
    let nonce = nonce.trim();
    if nonce.is_empty() {
        return Err(format!("refusing {action}: LS_DISPATCH_NONCE is empty"));
    }
    let issued: i64 = nonce.parse().map_err(|_| {
        format!(
            "refusing {action}: LS_DISPATCH_NONCE must be a fresh unix-seconds timestamp minted \
             this wave (`date +%s`), not a static constant"
        )
    })?;
    let age = now_unix - issued;
    if age > NONCE_TTL_SECS {
        return Err(format!(
            "refusing {action}: LS_DISPATCH_NONCE is stale ({age}s old > {NONCE_TTL_SECS}s TTL) — \
             a replayed or hardcoded nonce cannot re-authorize; mint a fresh one this wave"
        ));
    }
    if age < -NONCE_MAX_SKEW_SECS {
        return Err(format!(
            "refusing {action}: LS_DISPATCH_NONCE is from the future (skew {}s) — rejecting",
            -age
        ));
    }
    Ok(())
}

/// Detect an unattended/CI marker: `CI` or `GITHUB_ACTIONS` set, or stdin not a TTY.
/// The gate treats any of these as unattended (agent Bash tools have no TTY).
pub fn detect_unattended_marker() -> Option<String> {
    for var in ["CI", "GITHUB_ACTIONS"] {
        if std::env::var(var).map(|v| !v.is_empty()).unwrap_or(false) {
            return Some(format!("{var} is set"));
        }
    }
    if !std::io::IsTerminal::is_terminal(&std::io::stdin()) {
        return Some("stdin is not a TTY".to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gate(nonce: Option<&str>, now: i64) -> OperatorGate {
        OperatorGate {
            unattended_marker: None,
            nonce: nonce.map(str::to_string),
            now_unix: now,
        }
    }

    #[test]
    fn fresh_nonce_authorizes() {
        assert!(gate(Some("1000"), 1200).authorize("defer").is_ok());
    }

    #[test]
    fn absent_nonce_refused() {
        let err = gate(None, 1200).authorize("defer").unwrap_err();
        assert!(err.contains("nonce absent"), "{err}");
    }

    #[test]
    fn stale_nonce_refused() {
        let err = gate(Some("1000"), 1000 + NONCE_TTL_SECS + 1).authorize("defer").unwrap_err();
        assert!(err.contains("stale"), "{err}");
    }

    #[test]
    fn non_numeric_nonce_refused() {
        let err = gate(Some("hunter2"), 1200).authorize("defer").unwrap_err();
        assert!(err.contains("fresh unix-seconds timestamp"), "{err}");
    }

    #[test]
    fn future_nonce_refused() {
        let err = gate(Some("100000"), 1000).authorize("defer").unwrap_err();
        assert!(err.contains("future"), "{err}");
    }

    #[test]
    fn unattended_marker_refuses_even_with_fresh_nonce() {
        let g = OperatorGate {
            unattended_marker: Some("CI is set".into()),
            nonce: Some("1200".into()),
            now_unix: 1200,
        };
        let err = g.authorize("defer").unwrap_err();
        assert!(err.contains("unattended"), "{err}");
    }
}
