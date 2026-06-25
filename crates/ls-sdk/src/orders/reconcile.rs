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
use ls_core::LsError;
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use super::{T0425OutBlock1, T0425Response};

type HmacSha256 = Hmac<Sha256>;

/// Stated retention bound for a reconciliation evidence record (days). Matches
/// the §4 manual-evidence freshness window — evidence older than this should be
/// re-collected before relying on it.
pub const EVIDENCE_RETENTION_DAYS: u32 = 7;

/// Minimum HMAC key length (bytes). A short/empty key collapses HMAC-SHA256 to a
/// publicly-reproducible keyed hash, which for a low-entropy account number is
/// brute-forceable exactly like a bare SHA-256 — voiding the §5 redaction. The
/// record builder rejects a shorter key rather than producing a falsely-redacted
/// artifact.
pub const MIN_HMAC_KEY_LEN: usize = 32;

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

/// Which order action an intent represents. The reconciliation direction differs
/// by action: a submit asks "did a brand-new order appear?", a modify asks "did
/// the target change land?" (idempotent-by-target), and a cancel asks "is the
/// referenced order *gone*?" — with cancel failing toward still-live (R7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum OrderAction {
    /// A new-order submit (`CSPAT00601`). The default so existing call sites that
    /// build an intent for a submit reconcile unchanged.
    #[default]
    Submit,
    /// A modify (`CSPAT00701`) against an existing order number (`OrgOrdNo`).
    Modify,
    /// A cancel (`CSPAT00801`) against an existing order number (`OrgOrdNo`).
    Cancel,
}

/// The local intent for an order — what the caller tried to submit. Used both to
/// match against `t0425` rows and to build the redacting evidence record.
///
/// `#[non_exhaustive]`: build it through [`OrderIntent::submit`] /
/// [`OrderIntent::modify`] / [`OrderIntent::cancel`], never a struct literal — the
/// order surface keeps growing fields and the constructors set the action
/// discriminator + referenced order number coherently.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
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
    /// The order number from a *submit* ack, when one was returned. For a submit
    /// this is the strongest match key; for a modify/cancel it is the NEW order
    /// number and is usually unknown after an ambiguous send (use `org_order_no`).
    pub order_no: Option<String>,
    /// The *referenced* original order number (`OrgOrdNo`) for a modify/cancel —
    /// the order whose transition we are reconciling. Matched against both
    /// `t0425.ordno` (the original row) and `t0425.orgordno` (a modify/cancel-child
    /// row). `None`/empty for a submit.
    pub org_order_no: Option<String>,
    /// Submit (default), Modify, or Cancel — drives the reconciliation direction.
    pub action: OrderAction,
}

impl OrderIntent {
    /// Build a submit intent (the default action). `order_no` is the submit ack's
    /// order number when known.
    pub fn submit(
        account_no: impl Into<String>,
        symbol: impl Into<String>,
        side: impl Into<String>,
        qty: impl Into<String>,
        price: impl Into<String>,
        order_no: Option<String>,
    ) -> Self {
        OrderIntent {
            account_no: account_no.into(),
            symbol: symbol.into(),
            side: side.into(),
            qty: qty.into(),
            price: price.into(),
            order_no,
            org_order_no: None,
            action: OrderAction::Submit,
        }
    }

    /// Build a modify intent referencing an existing order number (`OrgOrdNo`).
    /// `qty`/`price` are the modify's absolute target values (KTD4).
    pub fn modify(
        account_no: impl Into<String>,
        symbol: impl Into<String>,
        side: impl Into<String>,
        qty: impl Into<String>,
        price: impl Into<String>,
        org_order_no: impl Into<String>,
    ) -> Self {
        OrderIntent {
            account_no: account_no.into(),
            symbol: symbol.into(),
            side: side.into(),
            qty: qty.into(),
            price: price.into(),
            order_no: None,
            org_order_no: Some(org_order_no.into()),
            action: OrderAction::Modify,
        }
    }

    /// Build a cancel intent referencing an existing order number (`OrgOrdNo`).
    pub fn cancel(
        account_no: impl Into<String>,
        symbol: impl Into<String>,
        side: impl Into<String>,
        qty: impl Into<String>,
        org_order_no: impl Into<String>,
    ) -> Self {
        OrderIntent {
            account_no: account_no.into(),
            symbol: symbol.into(),
            side: side.into(),
            qty: qty.into(),
            price: String::new(),
            order_no: None,
            org_order_no: Some(org_order_no.into()),
            action: OrderAction::Cancel,
        }
    }
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
    match bnstpcode.trim() {
        "1" => row_medosu.contains("매도"),
        "2" => row_medosu.contains("매수"),
        // An unrecognized/absent side code cannot be corroborated. Return `true`
        // (inconclusive, not excluding) so the row is still matched on its other
        // fields — never declaring a FALSE absence that would green-light a retry
        // of an order that may have landed. (In practice the side is always the
        // CSPAT00601 BnsTpCode "1"/"2".)
        _ => true,
    }
}

/// The order number an intent references for matching: the submit's own
/// `order_no`, or — for a modify/cancel — the referenced original `org_order_no`.
/// Returns the normalized number only when it is USABLE (non-empty, non-zero); a
/// `""`/`"0"` key would spuriously equal a blank row field and match an unrelated
/// row, so it yields `None` and the caller falls through to field corroboration.
fn reference_order_no(intent: &OrderIntent) -> Option<String> {
    let raw = match intent.action {
        OrderAction::Submit => intent.order_no.as_deref(),
        OrderAction::Modify | OrderAction::Cancel => intent.org_order_no.as_deref(),
    };
    let n = normalize_ordno(raw.unwrap_or(""));
    (!n.is_empty() && n != "0").then_some(n)
}

/// `true` if `row` is a modify/cancel-child row of the referenced order — i.e. its
/// `orgordno` (원주문번호) points back at `refno`. Only meaningful for a
/// modify/cancel intent; a submit has no children to find.
fn row_is_child_of(intent: &OrderIntent, row: &super::T0425OutBlock1, refno: &str) -> bool {
    matches!(intent.action, OrderAction::Modify | OrderAction::Cancel)
        && normalize_ordno(&row.orgordno) == refno
}

/// `true` if a `t0425` row corresponds to the intent.
///
/// When a usable reference order number is known it is the sole, strongest key (a
/// venue order number is unique). For a modify/cancel the row matches on EITHER the
/// original order (`row.ordno == OrgOrdNo`) OR a child row created by the
/// modify/cancel (`row.orgordno == OrgOrdNo`). Otherwise corroborate on symbol +
/// side + quantity + price.
fn row_matches(intent: &OrderIntent, row: &super::T0425OutBlock1) -> bool {
    if let Some(refno) = reference_order_no(intent) {
        return normalize_ordno(&row.ordno) == refno || row_is_child_of(intent, row, &refno);
    }
    // Field corroboration (also the no-order-number path). Price is compared
    // only for a priced (limit) order: a marketable/market order submits OrdPrc
    // "0" while the t0425 row carries the executed/venue price, so requiring
    // price equality there would wrongly exclude a landed order and falsely
    // green-light a retry. A zero/empty intent price drops the price predicate
    // (match on symbol+side+qty) — strictly more conservative against a double
    // fill.
    let intent_price = intent.price.trim();
    let price_ok = intent_price.is_empty()
        || intent_price == "0"
        || row.price.trim() == intent_price;
    row.expcode.trim() == intent.symbol.trim()
        && side_matches(&intent.side, &row.medosu)
        && row.qty.trim() == intent.qty.trim()
        && price_ok
}

/// Classify a matched row's `status` (상태) text into a state. A row that exists
/// in `t0425` means the order reached the venue, so the floor is Accepted.
fn classify_status(status: &str) -> OrderState {
    // Check the rejection markers FIRST: a composite status such as `정정거부`
    // (modification rejected) or `취소거부` (cancel rejected) means the modify /
    // cancel did NOT take effect and the original order is still live — so it
    // must not be read as Modified / Canceled.
    if status.contains("거부") || status.contains("거절") {
        OrderState::Rejected
    } else if status.contains("취소") {
        OrderState::Canceled
    } else if status.contains("정정") {
        OrderState::Modified
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
/// - matching rows → an **action-aware** classification (see [`reconcile_rows`]):
///   a submit reads the order's status; a modify lands only on `정정`/a child row;
///   a cancel fails toward still-live unless an explicit `취소` row is present.
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
    // A single response only proves absence if it is the TERMINAL page — an
    // unfinished `tr_cont` continuation means more rows exist that we have not
    // inspected, so a no-match there is NOT proven absence.
    reconcile_rows(intent, &resp.outblock1, response_is_terminal(resp), false)
}

/// `true` if a `t0425` response is the last page (no `tr_cont` continuation).
fn response_is_terminal(resp: &T0425Response) -> bool {
    let c = resp.tr_cont.trim();
    c.is_empty() || c.eq_ignore_ascii_case("n")
}

/// Classify an ambiguous send against the FULL set of `t0425` rows.
///
/// `query_complete` MUST be true only when every page was inspected (a terminal
/// single page, or an exhausted `collect_all`). This is the load-bearing
/// safety gate: `safe_to_retry` is `true` ONLY on a no-match over a *complete*
/// query — a truncated or partial query that finds no match fails toward
/// Unknown + not-safe, so an order sitting on an un-fetched page can never
/// green-light a resubmit (the double-fill the package exists to prevent).
pub fn reconcile_rows(
    intent: &OrderIntent,
    rows: &[T0425OutBlock1],
    query_complete: bool,
    dedup_hit: bool,
) -> ReconcileOutcome {
    if dedup_hit {
        return ReconcileOutcome::duplicate();
    }
    // Scan ALL matching rows — NEVER early-return on the first. A landed modify can
    // leave the original `OrgOrdNo` row at `접수` (which classifies Accepted), so a
    // first-row early-return would falsely report an un-applied modify as "landed";
    // and a cancel can show a still-`접수` original row alongside a `취소` row. We
    // must inspect every matched row and take the strongest classification.
    let matched: Vec<&T0425OutBlock1> = rows.iter().filter(|r| row_matches(intent, r)).collect();
    if matched.is_empty() {
        return ReconcileOutcome {
            state: OrderState::Unknown,
            // Proven absent ONLY if the query was complete.
            safe_to_retry: query_complete,
        };
    }

    let any = |state: OrderState| {
        matched
            .iter()
            .any(|r| classify_status(&r.status) == state)
    };
    let any_rejected = any(OrderState::Rejected);
    let any_canceled = any(OrderState::Canceled);
    let any_modified = any(OrderState::Modified);
    // A LIVE modify/cancel-child row (`orgordno == OrgOrdNo`) is direct evidence the
    // transition produced a new resting order — but ONLY when the child is itself
    // live (a `접수`/`체결`/`정정` status, i.e. classified Accepted or Modified). A
    // `취소`/`거부` child is NOT a landed-modify witness: a canceled or rejected
    // child must not be counted as a new resting order (it would mislabel a canceled
    // or rejected order as a landed modify — review-flagged).
    let has_live_child = reference_order_no(intent).is_some_and(|refno| {
        matched.iter().any(|r| {
            row_is_child_of(intent, r, &refno)
                && matches!(
                    classify_status(&r.status),
                    OrderState::Accepted | OrderState::Modified
                )
        })
    });

    match intent.action {
        // Submit: the matched row's status is the outcome (order is at the venue),
        // rejection-first across all matched rows (a venue order number is unique,
        // so multiple matches are implausible — but if present, the strongest
        // transition wins: Rejected > Canceled > Modified > Accepted). Never safe to
        // retry — an order is present.
        OrderAction::Submit => {
            let state = if any_rejected {
                OrderState::Rejected
            } else if any_canceled {
                OrderState::Canceled
            } else if any_modified {
                OrderState::Modified
            } else {
                OrderState::Accepted
            };
            ReconcileOutcome { state, safe_to_retry: false }
        }
        // Modify is absolute (idempotent-by-target, KTD4). Precedence across all
        // matched rows, strongest transition first:
        //   1. a `취소` row → the referenced order was Canceled (by someone) — the
        //      modify did NOT land and the order is gone; never read this as
        //      Modified, never clear retry.
        //   2. a `정정` row OR a LIVE child (new resting order) → Modified/landed.
        //      A landed child witness must NOT be masked by a `정정거부` sibling
        //      (e.g. a prior rejected modify of the same original) — a live child
        //      means an order rests, so we do not clear retry (review-flagged).
        //   3. a `정정거부` with no landed child → Rejected, but safe to re-send
        //      (the absolute target re-applies cleanly).
        //   4. a bare still-`접수`/`체결` original → not landed, safe to re-send.
        // We never classify a still-resting original row as Accepted/landed.
        OrderAction::Modify => {
            if any_canceled {
                ReconcileOutcome {
                    state: OrderState::Canceled,
                    safe_to_retry: false,
                }
            } else if any_modified || has_live_child {
                ReconcileOutcome {
                    state: OrderState::Modified,
                    safe_to_retry: false,
                }
            } else if any_rejected {
                // `정정거부` — modify rejected, original unchanged; idempotent-by-
                // target, so a re-send is safe.
                ReconcileOutcome {
                    state: OrderState::Rejected,
                    safe_to_retry: true,
                }
            } else {
                // Bare still-`접수`/`체결` original — not landed, safe to re-send.
                ReconcileOutcome {
                    state: OrderState::Unknown,
                    safe_to_retry: true,
                }
            }
        }
        // Cancel INVERTS the risk (R7, AE1): a matched row is canceled ONLY on an
        // explicit `취소` row. Anything else — a still-`접수`/체결 original, a `정정`,
        // or a `취소거부` rejection — means the order may still rest, so we fail
        // toward still-live (never Accepted) and never clear retry. NOTE the
        // ASYMMETRY vs Modify: a rejected cancel keeps `safe_to_retry: false` (the
        // order may still be live, so do not invite an auto-clear), whereas a
        // rejected modify is `safe_to_retry: true` (idempotent-by-target). Do NOT
        // "align" these — the inverted cancel risk is the whole point of this lane.
        OrderAction::Cancel => {
            if any_canceled {
                ReconcileOutcome {
                    state: OrderState::Canceled,
                    safe_to_retry: false,
                }
            } else {
                let state = if any_rejected {
                    OrderState::Rejected
                } else {
                    OrderState::Unknown
                };
                ReconcileOutcome { state, safe_to_retry: false }
            }
        }
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
    ///
    /// FAIL-CLOSED: a key shorter than [`MIN_HMAC_KEY_LEN`] is rejected with
    /// `LsError::Config` rather than producing a falsely-redacted record.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        recorded_at: impl Into<String>,
        tr_code: impl Into<String>,
        hmac_key: &[u8],
        intent: &OrderIntent,
        dedup_key: &str,
        outcome: ReconcileOutcome,
        error: Option<String>,
    ) -> Result<Self, LsError> {
        if hmac_key.len() < MIN_HMAC_KEY_LEN {
            return Err(LsError::Config(format!(
                "reconciliation HMAC key must be at least {MIN_HMAC_KEY_LEN} bytes to redact \
                 the low-entropy account number; got {} bytes",
                hmac_key.len()
            )));
        }
        Ok(ReconciliationRecord {
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
        })
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
            org_order_no: None,
            action: OrderAction::Submit,
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

    /// A modify/cancel-child row: its own `ordno` plus an `orgordno` pointing back
    /// at the referenced original order.
    fn child_row(ordno: &str, orgordno: &str, status: &str) -> T0425OutBlock1 {
        T0425OutBlock1 {
            orgordno: orgordno.into(),
            ..row(ordno, status)
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
    fn empty_or_zero_order_number_falls_through_to_field_corroboration() {
        // An empty ack order number must NOT spuriously equal a blank row ordno;
        // it falls through to symbol+side+qty+price corroboration instead.
        let mut empty = intent(Some(""));
        empty.order_no = Some("".into());
        // A row whose fields match (ordno blank) is found via corroboration.
        let blank_row = T0425OutBlock1 {
            ordno: "".into(),
            expcode: "005930".into(),
            medosu: "매수".into(),
            qty: "2".into(),
            price: "60000".into(),
            status: "접수".into(),
            ..Default::default()
        };
        assert_eq!(
            reconcile(&empty, Some(&resp(vec![blank_row])), false).state,
            OrderState::Accepted,
            "empty ordno must corroborate on fields, not match a blank ordno blindly"
        );
        // A "0" order number likewise corroborates rather than matching ordno 0.
        let mut zero = intent(Some("0"));
        zero.order_no = Some("0".into());
        // No field match (different symbol) -> not found, safe to retry.
        let other = T0425OutBlock1 {
            ordno: "0".into(),
            expcode: "000660".into(),
            medosu: "매수".into(),
            ..Default::default()
        };
        let out = reconcile(&zero, Some(&resp(vec![other])), false);
        assert_eq!(out.state, OrderState::Unknown);
        assert!(out.safe_to_retry);
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
    fn rejection_status_classifies_rejected_even_when_composite() {
        assert_eq!(classify_status("거부"), OrderState::Rejected);
        assert_eq!(classify_status("거절"), OrderState::Rejected);
        // A rejected modify/cancel must NOT read as Modified/Canceled — the
        // original order is still live.
        assert_eq!(classify_status("정정거부"), OrderState::Rejected);
        assert_eq!(classify_status("취소거부"), OrderState::Rejected);
        assert_eq!(
            reconcile(&intent(Some("84")), Some(&resp(vec![row("84", "거부")])), false).state,
            OrderState::Rejected
        );
    }

    #[test]
    fn no_match_on_a_non_terminal_page_is_not_safe_to_retry() {
        // A response with an unfinished tr_cont continuation is incomplete: a
        // no-match must NOT be treated as proven absence (the order could sit on
        // an un-fetched page — the double-fill guard).
        let mut truncated = resp(vec![row("84", "접수")]);
        truncated.tr_cont = "Y".into();
        let out = reconcile(&intent(Some("999")), Some(&truncated), false);
        assert_eq!(out.state, OrderState::Unknown);
        assert!(
            !out.safe_to_retry,
            "a truncated query proves nothing about absence"
        );
        // reconcile_rows with query_complete=false is likewise not safe.
        assert!(!reconcile_rows(&intent(Some("999")), &[], false, false).safe_to_retry);
        // A complete empty query IS safe.
        assert!(reconcile_rows(&intent(Some("999")), &[], true, false).safe_to_retry);
    }

    // ---- Modify/cancel order-state reconciliation (U2) ----------------------

    /// A modify whose referenced `OrgOrdNo` row shows `정정` classifies Modified.
    #[test]
    fn modify_with_jeongjeong_original_row_classifies_modified() {
        let intent = OrderIntent::modify("00000000-01", "005930", "2", "1", "8350", "84005");
        let out = reconcile_rows(&intent, &[row("84005", "정정")], true, false);
        assert_eq!(out.state, OrderState::Modified);
        assert!(!out.safe_to_retry);
    }

    /// A modify where a CHILD row exists (`orgordno == OrgOrdNo`) classifies
    /// Modified/landed even though the new child's own status is a fresh `접수`
    /// (KTD4: a landed modify creates a new order number).
    #[test]
    fn modify_with_child_row_classifies_modified_landed() {
        let intent = OrderIntent::modify("00000000-01", "005930", "2", "1", "8350", "84005");
        let rows = [
            row("84005", "접수"),               // the original, still resting
            child_row("84006", "84005", "접수"), // the modify's new child order
        ];
        let out = reconcile_rows(&intent, &rows, true, false);
        assert_eq!(out.state, OrderState::Modified, "a child row proves the modify landed");
        assert!(!out.safe_to_retry);
    }

    /// REGRESSION GUARD (review-flagged P1): a modify whose original `OrgOrdNo` row
    /// is still `접수` with NO `정정` row and NO child row is NOT landed — it must
    /// classify safe-to-retry (idempotent-by-target), NEVER Accepted. This proves
    /// the matcher does not early-return Accepted on a still-resting original row.
    #[test]
    fn modify_with_bare_jeopsu_original_is_not_landed_safe_to_retry() {
        let intent = OrderIntent::modify("00000000-01", "005930", "2", "1", "8350", "84005");
        let out = reconcile_rows(&intent, &[row("84005", "접수")], true, false);
        assert_ne!(out.state, OrderState::Accepted, "a still-resting original is NOT a landed modify");
        assert_ne!(out.state, OrderState::Modified);
        assert_eq!(out.state, OrderState::Unknown);
        assert!(out.safe_to_retry, "an un-applied absolute modify is safe to re-send");
    }

    /// A modify rejected at the venue (`정정거부`) classifies Rejected (the original
    /// is unchanged), and is safe to retry — the absolute target re-applies cleanly.
    #[test]
    fn modify_rejected_classifies_rejected_and_safe_to_retry() {
        let intent = OrderIntent::modify("00000000-01", "005930", "2", "1", "8350", "84005");
        let out = reconcile_rows(&intent, &[row("84005", "정정거부")], true, false);
        assert_eq!(out.state, OrderState::Rejected);
        assert!(out.safe_to_retry);
    }

    /// REVIEW-FLAGGED: a modify whose referenced order shows a `취소` child (the
    /// order was canceled by someone) must NOT read as Modified — the modify did
    /// not land and the order is gone. A `취소` child is not a landed-modify witness.
    #[test]
    fn modify_with_canceled_child_classifies_canceled_not_modified() {
        let intent = OrderIntent::modify("00000000-01", "005930", "2", "1", "8350", "84005");
        let rows = [
            row("84005", "접수"),               // the original
            child_row("84006", "84005", "취소"), // a cancel of the original landed
        ];
        let out = reconcile_rows(&intent, &rows, true, false);
        assert_eq!(out.state, OrderState::Canceled, "a 취소 child is not a landed modify");
        assert_ne!(out.state, OrderState::Modified);
        assert!(!out.safe_to_retry);
    }

    /// REVIEW-FLAGGED: a modify with a LIVE landed child (new resting order) must
    /// stay Modified even when a `정정거부` sibling is also present (e.g. a prior
    /// rejected modify of the same original). A live child means an order rests, so
    /// the rejection must NOT mask it into a safe-to-retry verdict.
    #[test]
    fn modify_with_landed_child_and_rejected_sibling_stays_modified() {
        let intent = OrderIntent::modify("00000000-01", "005930", "2", "1", "8350", "84005");
        let rows = [
            row("84005", "접수"),                   // the original
            child_row("84008", "84005", "정정거부"), // a prior rejected modify
            child_row("84007", "84005", "접수"),     // the modify that landed (live child)
        ];
        let out = reconcile_rows(&intent, &rows, true, false);
        assert_eq!(
            out.state,
            OrderState::Modified,
            "a live landed child must not be masked by a rejected sibling"
        );
        assert!(
            !out.safe_to_retry,
            "a resting child order must never green-light a blind re-send"
        );
    }

    /// AE1: a cancel whose `t0425` row still shows a resting `접수` with `ordrem > 0`
    /// classifies NOT-canceled / still-live — never Accepted — and never clears
    /// retry. The inverted-risk direction: a cancel is success ONLY on proof.
    #[test]
    fn cancel_with_still_resting_original_is_not_canceled_never_accepted() {
        let intent = OrderIntent::cancel("00000000-01", "005930", "2", "2", "84005");
        let mut resting = row("84005", "접수");
        resting.ordrem = "2".into(); // still unfilled at the venue
        let out = reconcile_rows(&intent, &[resting], true, false);
        assert_ne!(out.state, OrderState::Accepted, "a cancel must never read as Accepted");
        assert_ne!(out.state, OrderState::Canceled, "the order still rests — not canceled");
        assert_eq!(out.state, OrderState::Unknown);
        assert!(!out.safe_to_retry, "never clear retry while the order may rest");
    }

    /// STRONGEST-CLASSIFICATION (review-flagged P1): a cancel whose page contains
    /// BOTH a still-`접수` original row AND a `취소` row for the referenced order
    /// classifies Canceled — proving the matcher scans all rows rather than
    /// returning on the first `ordno`/`접수` hit.
    #[test]
    fn cancel_scans_all_rows_and_takes_strongest_canceled() {
        let intent = OrderIntent::cancel("00000000-01", "005930", "2", "1", "84005");
        let rows = [
            row("84005", "접수"),               // original row, would read Accepted alone
            child_row("84006", "84005", "취소"), // the cancel transition
        ];
        let out = reconcile_rows(&intent, &rows, true, false);
        assert_eq!(out.state, OrderState::Canceled, "a 취소 row outranks a still-접수 original");
        assert!(!out.safe_to_retry);
    }

    /// A cancel rejected at the venue (`취소거부`) is still-live: Rejected, never
    /// Canceled, never safe to assume gone.
    #[test]
    fn cancel_rejected_is_still_live_rejected() {
        let intent = OrderIntent::cancel("00000000-01", "005930", "2", "1", "84005");
        let out = reconcile_rows(&intent, &[row("84005", "취소거부")], true, false);
        assert_eq!(out.state, OrderState::Rejected);
        assert!(!out.safe_to_retry);
    }

    /// An ambiguous modify/cancel with NO matching row over a COMPLETE query is
    /// safe to retry; over an INCOMPLETE query it stays Unknown and never clears
    /// retry (the order could sit on an un-fetched page).
    #[test]
    fn modify_cancel_no_match_respects_query_completeness() {
        let intent = OrderIntent::cancel("00000000-01", "005930", "2", "1", "84005");
        // Unrelated order number only — no match.
        let rows = [row("99999", "접수")];
        assert!(reconcile_rows(&intent, &rows, true, false).safe_to_retry);
        assert!(!reconcile_rows(&intent, &rows, false, false).safe_to_retry);
    }

    /// An empty/zero referenced `OrgOrdNo` falls back to symbol/side/qty/price
    /// corroboration (resolving the origin open question) rather than blindly
    /// matching a blank row field.
    #[test]
    fn modify_cancel_empty_org_order_no_falls_back_to_corroboration() {
        // Cancel with an empty OrgOrdNo: corroborate on the order fields instead.
        let mut intent = OrderIntent::cancel("00000000-01", "005930", "2", "2", "");
        intent.org_order_no = Some(String::new());
        // A field-matching resting row is found via corroboration -> not-canceled.
        let mut resting = row("84005", "접수");
        resting.ordrem = "2".into();
        let out = reconcile_rows(&intent, &[resting], true, false);
        assert_eq!(out.state, OrderState::Unknown, "corroborated resting row is not-canceled");
        assert!(!out.safe_to_retry);
        // A blank reference must NOT spuriously match a blank `ordno` of an
        // unrelated symbol -> no match, safe to retry over a complete query.
        let other = T0425OutBlock1 {
            expcode: "000660".into(),
            ..Default::default()
        };
        assert!(reconcile_rows(&intent, &[other], true, false).safe_to_retry);
    }

    #[test]
    fn weak_hmac_key_is_rejected_fail_closed() {
        let intent = intent(Some("84"));
        let outcome = ReconcileOutcome {
            state: OrderState::Accepted,
            safe_to_retry: false,
        };
        let err = ReconciliationRecord::new(
            "t", "CSPAT00601", b"short", &intent, "deadbeef", outcome, None,
        )
        .expect_err("a sub-32-byte key must be rejected");
        assert!(matches!(err, LsError::Config(_)));
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
            b"operator-hmac-key-at-least-32-bytes-long!!",
            &intent,
            dedup_key,
            outcome,
            Some("timeout".into()),
        )
        .expect("a >=32-byte key is accepted");
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
