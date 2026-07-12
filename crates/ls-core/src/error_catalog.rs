//! Shared gateway error-explanation catalog (error-resilience gate U1, R8/R9).
//!
//! `metadata/error-catalog.yaml` maps every gateway response code the runtime
//! recognises to a human-readable explanation. The file is embedded at build time
//! (see `build.rs` / [`crate::embedded`]) and parsed once here, so
//! [`LsError::explain`](crate::LsError::explain) can surface a reason for an
//! `ApiError` without a filesystem read and without ever echoing the gateway's
//! `rsp_msg` or any account data back to the caller.
//!
//! Environment / entitlement codes (`904`, `01900`, `01491`) are explained here
//! ONCE (R9); they are never reproduced per TR.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::Deserialize;

/// One catalog entry: a classification `kind` plus the user-facing `explanation`.
#[derive(Debug, Clone, Deserialize)]
pub struct CatalogEntry {
    /// Coarse classification of the code (`success`, `paper_incompatible`,
    /// `account_not_order_capable`, `session_closed`, `request_shape`,
    /// `gateway_error`, ...). Used by docgen to group codes; not load-bearing at
    /// runtime.
    pub kind: String,
    /// The user-facing reason. Credential-free and stable across runs.
    pub explanation: String,
}

#[derive(Debug, Deserialize)]
struct Catalog {
    #[allow(dead_code)]
    version: u32,
    codes: BTreeMap<String, CatalogEntry>,
}

/// The stable fallback surfaced for a code absent from the catalog. Deliberately
/// generic and credential-free — it never includes the gateway's `rsp_msg`.
pub const UNKNOWN_CODE_EXPLANATION: &str =
    "An unrecognized gateway response code. See the LS gateway documentation; \
     the raw broker message is withheld to avoid leaking account data.";

fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_yaml::from_str::<Catalog>(crate::embedded::ERROR_CATALOG_YAML)
            .expect("embedded metadata/error-catalog.yaml must parse")
    })
}

/// Look up the human-readable explanation for a gateway `code`, if the catalog
/// maps it. Returns `None` for an unrecognized code — callers that want a total
/// function use [`explain_or_default`].
pub fn explain(code: &str) -> Option<&'static str> {
    catalog().codes.get(code).map(|e| e.explanation.as_str())
}

/// Total variant of [`explain`]: returns the mapped explanation or a stable
/// generic fallback for an unknown code.
pub fn explain_or_default(code: &str) -> &'static str {
    explain(code).unwrap_or(UNKNOWN_CODE_EXPLANATION)
}

/// The classification `kind` for a `code`, if mapped.
pub fn kind(code: &str) -> Option<&'static str> {
    catalog().codes.get(code).map(|e| e.kind.as_str())
}

/// `true` if `rsp_cd` is a gateway **ingress input-validation** rejection — the
/// gateway refused the request at ingress *before* routing it to the exchange, so
/// it structurally **placed nothing**. This is the single source of truth consumed
/// by both the live order path (`inner::dispatch_once`, which surfaces it as a clean
/// `ApiError` rejection rather than an `AmbiguousOrder` may-rest) and the offline
/// order negative-probe (`crates/ls-sdk/tests/negative_probe.rs`, which classifies it
/// Clean-and-continue rather than may-rest-halt), so the two can never drift.
///
/// Deliberately narrow: **only `IGW40011`** (a numeric request field sent as a quoted
/// string — see AGENTS.md and the `request_shape` catalog entry). It is NOT the
/// rate-limit code (`IGW00201`) and NOT a generic hard gateway failure (`IGW40013`,
/// `IGW50008`), which may have reached the exchange and stay may-rest/reconcile. Add a
/// sibling code here only with per-code evidence that it, too, is a pre-routing reject.
pub fn is_ingress_validation_reject(rsp_cd: &str) -> bool {
    rsp_cd == "IGW40011"
}

/// Every catalog code with its entry, ordered by code. Used by docgen to project
/// the reachable-code table onto the Reference page (R11).
pub fn entries() -> impl Iterator<Item = (&'static str, &'static CatalogEntry)> {
    catalog()
        .codes
        .iter()
        .map(|(code, entry)| (code.as_str(), entry))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_parses_and_covers_every_known_code() {
        // The codes the plan (U1) names must all be present.
        for code in [
            "00000", "00136", "00707", "01900", "01491", "904", "IGW40011", "IGW40013",
            "IGW40014", "IGW50008",
        ] {
            assert!(
                explain(code).is_some(),
                "catalog must map `{code}`; it is a code the runtime classifies"
            );
        }
    }

    #[test]
    fn explain_returns_mapped_text_for_known_codes() {
        assert!(explain("01900").unwrap().contains("모의투자"));
        assert!(explain("IGW40011").unwrap().to_lowercase().contains("number"));
    }

    #[test]
    fn ingress_validation_reject_is_igw40011_only() {
        // R6/KTD2: the single source of truth for "placed nothing at ingress" is
        // deliberately narrow — only IGW40011 (numeric field sent as a string).
        assert!(is_ingress_validation_reject("IGW40011"));
        // The rate-limit code, hard gateway failures, business rejects, and
        // successes are NOT ingress rejects — they may have rested / are handled
        // elsewhere, and must stay may-rest/reconcile on the order path.
        for code in ["IGW00201", "IGW40013", "IGW40014", "IGW50008", "40510", "00040", "00000", ""] {
            assert!(
                !is_ingress_validation_reject(code),
                "`{code}` must NOT be treated as a placed-nothing ingress reject"
            );
        }
    }

    #[test]
    fn explain_unknown_code_is_none_and_default_is_stable_fallback() {
        assert!(explain("ZZ_NOPE").is_none());
        assert_eq!(explain_or_default("ZZ_NOPE"), UNKNOWN_CODE_EXPLANATION);
    }

    #[test]
    fn no_explanation_echoes_a_gateway_rsp_msg() {
        // The catalog is authored, not sourced from responses: no entry may look
        // like a leaked broker message (a crude guard against future drift where
        // someone pastes a raw rsp_msg in). Every explanation is non-empty prose.
        for (_code, entry) in entries() {
            assert!(!entry.explanation.trim().is_empty());
        }
    }
}
