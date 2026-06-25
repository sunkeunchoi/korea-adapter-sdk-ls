//! Order reconciliation (order-safety §3) — resolve an ambiguous send against
//! live exchange state, and the redacting local-evidence record (§3 step 2, §5).
//!
//! Because order dispatch is no-retry (§1), an ambiguous outcome (transport
//! timeout / 5xx / an ambiguous `rsp_cd`) is expected and must be *resolved*, not
//! swallowed. After such a send the SDK queries `t0425` and matches candidate
//! orders by account, symbol, side, quantity, price, time window, and — when
//! known — the order number, classifying the result into the six-state model. It
//! retries only after proving no matching order was accepted, or on an explicit
//! operator override.
//!
//! The local-evidence record persists the order intent for an operator to read,
//! so it carries the same at-rest posture as the rest of the order surface: the
//! account identifier is **keyed-hashed (HMAC-SHA256), never bare `SHA256`** —
//! account numbers are low-entropy and a bare hash is reversible by brute force.
//! The "request hash" field equals the dedup key (which itself embeds
//! `account_no`), so it is keyed-hashed too. The bare dedup key is never written.

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use super::T0425Response;

type HmacSha256 = Hmac<Sha256>;

/// Stated retention bound for a reconciliation evidence record (days). Matches
/// the §4 manual-evidence freshness window — evidence older than this should be
/// re-collected before relying on it.
pub const EVIDENCE_RETENTION_DAYS: u32 = 7;

/// The six-state order reconciliation model (order-safety §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderState {
    /// LS accepted the order for processing (order present at the venue).
    Accepted,
    /// LS rejected the order (no order at the venue).
    Rejected,
    /// The SDK returned a cached response for an identical request (`dedup_hit`).
    Duplicate,
    /// A pending order was changed by a modify TR.
    Modified,
    /// A pending order was canceled by a cancel TR.
    Canceled,
    /// The SDK cannot prove accepted versus not accepted.
    Unknown,
}

impl OrderState {
    /// A short stable token for the evidence record / logs.
    pub fn as_str(&self) -> &'static str {
        match self {
            OrderState::Accepted => "accepted",
            OrderState::Rejected => "rejected",
            OrderState::Duplicate => "duplicate",
            OrderState::Modified => "modified",
            OrderState::Canceled => "canceled",
            OrderState::Unknown => "unknown",
        }
    }
}

/// The local intent for an order — what the caller tried to submit. Used both to
/// match against `t0425` rows and to build the redacting evidence record.
#[derive(Debug, Clone)]
pub struct OrderIntent {
    /// Account number (cleartext here; never persisted in the clear — see
    /// [`ReconciliationRecord`]).
    pub account_no: String,
    /// Symbol / 종목번호 (the `t0425` query filter and a match field).
    pub symbol: String,
    /// Side as the `CSPAT00601` `BnsTpCode` (`"1"` sell / `"2"` buy).
    pub side: String,
    /// Order quantity.
    pub qty: String,
    /// Order price.
    pub price: String,
    /// The order number from the `CSPAT00601` ack, when one was returned. The
    /// strongest match key.
    pub order_no: Option<String>,
}

/// The reconciliation outcome: the classified state plus whether a retry is safe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReconcileOutcome {
    /// The classified six-state outcome.
    pub state: OrderState,
    /// `true` only when reconciliation proved no matching order was accepted (a
    /// clean query with no match). Never `true` for a failed/ambiguous query.
    pub safe_to_retry: bool,
}

impl ReconcileOutcome {
    /// A `dedup_hit` short-circuit: Duplicate, never safe to retry.
    pub fn duplicate() -> Self {
        ReconcileOutcome {
            state: OrderState::Duplicate,
            safe_to_retry: false,
        }
    }
}

/// Normalize an order number for cross-TR equality: `CSPAT00601.OrdNo` and the
/// `t0425` row `ordno` both arrive as decimal strings (via `string_or_number`),
/// possibly with whitespace or leading zeros. Compare numerically when possible.
pub fn normalize_ordno(s: &str) -> String {
    let t = s.trim();
    t.parse::<u64>().map(|n| n.to_string()).unwrap_or_else(|_| t.to_string())
}

/// Map a `CSPAT00601` `BnsTpCode` (`"1"` sell / `"2"` buy) to the `t0425` row
/// `medosu` Korean side text (`"매도"` / `"매수"`).
fn side_matches(bnstpcode: &str, row_medosu: &str) -> bool {
    let expected = match bnstpcode.trim() {
        "1" => "매도",
        "2" => "매수",
        // An unrecognized code can't be matched on side; fall back to a loose
        // contains check so matching does not silently exclude.
        other => return row_medosu.contains(other),
    };
    row_medosu.contains(expected)
}

/// `true` if a `t0425` row corresponds to the intent.
///
/// When the order number is known it is the sole, strongest key (a venue order
/// number is unique). Otherwise corroborate on symbol + side + quantity + price.
fn row_matches(intent: &OrderIntent, row: &super::T0425OutBlock1) -> bool {
    if let Some(ordno) = &intent.order_no {
        return normalize_ordno(ordno) == normalize_ordno(&row.ordno);
    }
    row.expcode.trim() == intent.symbol.trim()
        && side_matches(&intent.side, &row.medosu)
        && row.qty.trim() == intent.qty.trim()
        && row.price.trim() == intent.price.trim()
}

/// Classify a matched row's `status` (상태) text into a state. A row that exists
/// in `t0425` means the order reached the venue, so the floor is Accepted.
fn classify_status(status: &str) -> OrderState {
    if status.contains("취소") {
        OrderState::Canceled
    } else if status.contains("정정") {
        OrderState::Modified
    } else if status.contains("거부") || status.contains("거절") {
        // Defensive: rejected orders normally carry no t0425 row, but honor an
        // explicit rejection text if present.
        OrderState::Rejected
    } else {
        // 접수 (received) / 체결 (filled) / 확인 (confirmed) / anything else: the
        // order is present at the venue.
        OrderState::Accepted
    }
}

/// Classify an ambiguous send against a `t0425` inquiry (order-safety §3).
///
/// - `dedup_hit` → Duplicate.
/// - `inquiry: None` (the query failed) → Unknown, **not** safe to retry.
/// - a matching row → its status classification (Accepted/Canceled/Modified),
///   not safe to retry — the order is at the venue.
/// - a clean query with no matching row → Unknown but **safe to retry**: a
///   successful query proved no matching order was accepted (§3 step 6).
pub fn reconcile(
    intent: &OrderIntent,
    inquiry: Option<&T0425Response>,
    dedup_hit: bool,
) -> ReconcileOutcome {
    if dedup_hit {
        return ReconcileOutcome::duplicate();
    }
    let Some(resp) = inquiry else {
        // Query failed — we cannot prove the order's presence or absence.
        return ReconcileOutcome {
            state: OrderState::Unknown,
            safe_to_retry: false,
        };
    };
    for row in &resp.outblock1 {
        if row_matches(intent, row) {
            return ReconcileOutcome {
                state: classify_status(&row.status),
                safe_to_retry: false,
            };
        }
    }
    // Clean query, no matching order: proven absent for this symbol's window.
    ReconcileOutcome {
        state: OrderState::Unknown,
        safe_to_retry: true,
    }
}

/// A redacting local-evidence record (order-safety §3 step 2 / §5).
///
/// Built through this single serializer — NEVER the raw response struct — so an
/// account number or the bare dedup key can never leak into an at-rest artifact.
/// `account_ref` and `request_ref` are HMAC-SHA256 keyed hashes (low-entropy
/// account numbers must not be bare-hashed). The HMAC key is supplied by the
/// operator and is NOT part of the record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconciliationRecord {
    /// Caller-supplied record timestamp (the runtime has no clock).
    pub recorded_at: String,
    /// The order TR code.
    pub tr_code: String,
    /// Keyed hash of the account number — NOT cleartext, NOT a bare SHA-256.
    pub account_ref: String,
    /// Keyed hash of the dedup key (which embeds the account) — the "request
    /// hash" of §3, never the bare dedup key.
    pub request_ref: String,
    /// Symbol / 종목번호.
    pub symbol: String,
    /// Side (`BnsTpCode`).
    pub side: String,
    /// Order quantity.
    pub qty: String,
    /// Order price.
    pub price: String,
    /// The reconciled state.
    pub state: String,
    /// Whether a retry was proven safe.
    pub safe_to_retry: bool,
    /// The error that triggered reconciliation, if any.
    pub error: Option<String>,
    /// Stated retention bound (days).
    pub retention_days: u32,
}

impl ReconciliationRecord {
    /// Build a record. `hmac_key` is the operator-held key for the keyed account
    /// / request hashes; it is consumed only to compute the refs and is never
    /// stored on the record. `dedup_key` is the bare order dedup key (kept out of
    /// the record — only its keyed hash, `request_ref`, is written).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        recorded_at: impl Into<String>,
        tr_code: impl Into<String>,
        hmac_key: &[u8],
        intent: &OrderIntent,
        dedup_key: &str,
        outcome: ReconcileOutcome,
        error: Option<String>,
    ) -> Self {
        ReconciliationRecord {
            recorded_at: recorded_at.into(),
            tr_code: tr_code.into(),
            account_ref: keyed_hash(hmac_key, intent.account_no.as_bytes()),
            request_ref: keyed_hash(hmac_key, dedup_key.as_bytes()),
            symbol: intent.symbol.clone(),
            side: intent.side.clone(),
            qty: intent.qty.clone(),
            price: intent.price.clone(),
            state: outcome.state.as_str().to_string(),
            safe_to_retry: outcome.safe_to_retry,
            error,
            retention_days: EVIDENCE_RETENTION_DAYS,
        }
    }

    /// Serialize to the credential-free JSON written to the evidence location.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

/// HMAC-SHA256 keyed hash, hex-encoded. Keyed (not bare) so a low-entropy input
/// such as an account number cannot be recovered by brute force from the record.
fn keyed_hash(key: &[u8], data: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(data);
    let bytes = mac.finalize().into_bytes();
    let mut s = String::with_capacity(bytes.len() * 2);
    use std::fmt::Write;
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orders::{T0425OutBlock1, T0425Response};

    fn intent(order_no: Option<&str>) -> OrderIntent {
        OrderIntent {
            account_no: "00000000-01".into(),
            symbol: "005930".into(),
            side: "2".into(), // buy
            qty: "2".into(),
            price: "60000".into(),
            order_no: order_no.map(|s| s.to_string()),
        }
    }

    fn row(ordno: &str, status: &str) -> T0425OutBlock1 {
        T0425OutBlock1 {
            ordno: ordno.into(),
            expcode: "005930".into(),
            medosu: "매수".into(),
            qty: "2".into(),
            price: "60000".into(),
            status: status.into(),
            ..Default::default()
        }
    }

    fn resp(rows: Vec<T0425OutBlock1>) -> T0425Response {
        T0425Response {
            rsp_cd: "00000".into(),
            outblock1: rows,
            ..Default::default()
        }
    }

    #[test]
    fn matching_order_number_classifies_accepted_not_resubmitted() {
        let out = reconcile(&intent(Some("84")), Some(&resp(vec![row("84", "접수")])), false);
        assert_eq!(out.state, OrderState::Accepted);
        assert!(!out.safe_to_retry, "an accepted order must never be retried");
    }

    #[test]
    fn ordno_normalizes_across_leading_zero_and_whitespace() {
        // CSPAT00601.OrdNo "84" vs a t0425 row " 0084 " must match.
        let out = reconcile(&intent(Some("84")), Some(&resp(vec![row(" 0084 ", "체결")])), false);
        assert_eq!(out.state, OrderState::Accepted);
    }

    #[test]
    fn no_matching_order_is_unknown_but_safe_to_retry() {
        let out = reconcile(&intent(Some("999")), Some(&resp(vec![row("84", "접수")])), false);
        assert_eq!(out.state, OrderState::Unknown);
        assert!(out.safe_to_retry, "a clean query proving absence is safe to retry");
    }

    #[test]
    fn empty_response_is_unknown_never_silent_accepted() {
        let out = reconcile(&intent(Some("84")), Some(&resp(vec![])), false);
        assert_eq!(out.state, OrderState::Unknown);
        assert_ne!(out.state, OrderState::Accepted);
    }

    #[test]
    fn failed_query_is_unknown_and_not_safe_to_retry() {
        let out = reconcile(&intent(Some("84")), None, false);
        assert_eq!(out.state, OrderState::Unknown);
        assert!(!out.safe_to_retry, "a failed query cannot prove absence");
    }

    #[test]
    fn dedup_hit_is_duplicate_without_a_query() {
        let out = reconcile(&intent(Some("84")), None, true);
        assert_eq!(out.state, OrderState::Duplicate);
    }

    #[test]
    fn match_without_order_number_uses_symbol_side_qty_price() {
        // No order number known: corroborate on the order fields.
        let out = reconcile(&intent(None), Some(&resp(vec![row("84", "접수")])), false);
        assert_eq!(out.state, OrderState::Accepted);
        // A side mismatch (sell vs the buy row) must NOT match.
        let mut sell = intent(None);
        sell.side = "1".into();
        let out2 = reconcile(&sell, Some(&resp(vec![row("84", "접수")])), false);
        assert_eq!(out2.state, OrderState::Unknown);
        assert!(out2.safe_to_retry);
    }

    #[test]
    fn canceled_and_modified_status_classify_distinctly() {
        assert_eq!(
            reconcile(&intent(Some("84")), Some(&resp(vec![row("84", "취소")])), false).state,
            OrderState::Canceled
        );
        assert_eq!(
            reconcile(&intent(Some("84")), Some(&resp(vec![row("84", "정정")])), false).state,
            OrderState::Modified
        );
    }

    #[test]
    fn evidence_record_redacts_account_and_request_hash() {
        let intent = intent(Some("84"));
        let dedup_key = "deadbeefcafebabe0123456789abcdef"; // a bare dedup key
        let outcome = ReconcileOutcome {
            state: OrderState::Accepted,
            safe_to_retry: false,
        };
        let rec = ReconciliationRecord::new(
            "2026-06-25T00:00:00Z",
            "CSPAT00601",
            b"operator-hmac-key",
            &intent,
            dedup_key,
            outcome,
            Some("timeout".into()),
        );
        let json = rec.to_json().unwrap();

        // No cleartext account number.
        assert!(!json.contains("00000000-01"), "account leaked: {json}");
        // Not a BARE sha256 of the account number.
        let bare_account = {
            use sha2::Digest;
            let d = Sha256::digest(intent.account_no.as_bytes());
            let mut s = String::new();
            use std::fmt::Write;
            for b in d {
                let _ = write!(s, "{b:02x}");
            }
            s
        };
        assert!(!json.contains(&bare_account), "bare SHA256(account) present");
        // The bare dedup key is never written (only its keyed hash).
        assert!(!json.contains(dedup_key), "bare dedup key leaked: {json}");
        // The keyed refs ARE present and are stable hex.
        assert_eq!(rec.account_ref.len(), 64);
        assert_eq!(rec.request_ref.len(), 64);
        assert_eq!(rec.retention_days, EVIDENCE_RETENTION_DAYS);
    }
}
