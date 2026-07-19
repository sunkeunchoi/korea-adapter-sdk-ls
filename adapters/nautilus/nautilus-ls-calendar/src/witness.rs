//! KRX daily-market positive-witness rule (U6, KTD7).
//!
//! This unit turns an already-parsed KRX `stk_bydd_trd` daily-market response into a
//! positive Trading Session [`EvidenceRecord`] — but ONLY when the response qualifies.
//! It does NO transport and NO I/O: the response shape is synthetic in tests, and the
//! maintainer refresh tooling (U14) feeds real parsed responses through the same rule.
//!
//! # The witness rule (KTD7)
//!
//! A witness is accepted ONLY when ALL of the following hold:
//!
//! 1. the response is **successful** (`success == true`) and carries **no error envelope**
//!    (`error_code == None`),
//! 2. it is **structurally valid** (every row has a non-blank market label),
//! 3. it is **non-empty**,
//! 4. the requested date is **>= 2010-01-04** (the KTD7 lower bound), and
//! 5. it contains a **qualifying KOSPI row whose date matches the requested date**.
//!
//! Any other response — empty, malformed, failed, error-enveloped, date-mismatched,
//! pre-2010, or lacking a qualifying KOSPI row — is [`NonEvidence`](WitnessOutcome::NonEvidence)
//! with a typed [`NonWitnessReason`].
//!
//! # The critical safety property
//!
//! A non-qualifying response produces **no** [`EvidenceRecord`] at all. It never proves
//! `Closed`, and — because it emits nothing — it can never retract a prior positive
//! witness by absence. (The "absence never retracts" behavior itself lives in
//! [`reconcile`](crate::reconcile::reconcile) row 3; U6's contribution to that guarantee
//! is that a degenerate response yields `NonEvidence`, never a `Closed`-bearing or
//! invalidating record.)
//!
//! # How U14 builds the record
//!
//! [`witness_from_response`] returns a fully-formed [`EvidenceRecord`] with a *derivable
//! placeholder* id ([`default_witness_id`]) and a placeholder `source_id`
//! ([`KRX_DAILY_MARKET_SOURCE_HINT`]); its `date` is the requested date. U14 can either
//! mutate those fields on the returned record, or call [`build_witness_record`] directly
//! with its own id / source_id / recorded_at — both paths yield the same valid
//! PositiveWitness record.
//!
//! [`EvidenceRecord`]: crate::schema::EvidenceRecord

use chrono::{DateTime, NaiveDate, TimeZone, Utc};

use crate::schema::{EvidenceKind, EvidenceRecord};

/// The earliest date KTD7 admits a KRX daily-market witness for (the LS daily-market
/// history's practical lower bound).
pub const MIN_WITNESS_DATE: (i32, u32, u32) = (2010, 1, 4);

/// The placeholder `source_id` stamped on a witness produced by [`witness_from_response`].
/// U14 replaces it with the real [`Source::id`](crate::schema::Source::id) of its
/// [`SourceKind::KrxDailyMarket`](crate::schema::SourceKind::KrxDailyMarket) source.
pub const KRX_DAILY_MARKET_SOURCE_HINT: &str = "krx-daily-market";

/// One already-parsed row of a KRX `stk_bydd_trd` daily-market response.
///
/// Deliberately minimal: enough to express the market label and the date the row bears on,
/// which is all the witness rule needs. Kept LOCAL to this leaf crate — it must not depend
/// on `ls-sdk` (KTD1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KrxDailyRow {
    /// The civil date this row is for.
    pub date: NaiveDate,
    /// The market label (e.g. `"KOSPI"`). A blank label marks a structurally broken row.
    pub market: String,
}

impl KrxDailyRow {
    /// Whether this row is structurally well-formed (a non-blank market label).
    fn is_structurally_valid(&self) -> bool {
        !self.market.trim().is_empty()
    }

    /// Whether this row is a qualifying KOSPI row for `requested` — the market is KOSPI
    /// (case-insensitive) and the row's date matches the request.
    fn is_qualifying_kospi(&self, requested: NaiveDate) -> bool {
        self.date == requested && self.market.trim().eq_ignore_ascii_case("KOSPI")
    }
}

/// An already-parsed KRX `stk_bydd_trd` daily-market response.
///
/// This is the INPUT to [`witness_from_response`]. It is synthetic in tests; U14 feeds a
/// real parsed response through the identical rule. Kept LOCAL to this leaf crate (KTD1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KrxDailyMarketResponse {
    /// Whether the gateway reported success. `false` is a failed/errored response.
    pub success: bool,
    /// The date the response was requested for — the witness, if any, is dated to this.
    pub requested_date: NaiveDate,
    /// The parsed daily-market rows.
    pub rows: Vec<KrxDailyRow>,
    /// A gateway error code, if the response carried an error envelope.
    pub error_code: Option<String>,
}

/// Why a response did NOT qualify as a positive witness. Every variant means the same
/// safety outcome: no [`EvidenceRecord`] is produced — no `Closed`, no retraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NonWitnessReason {
    /// A successful, un-erroring response with no rows.
    Empty,
    /// A structurally broken response (a row with a blank market label).
    Malformed,
    /// The gateway reported failure (`success == false`) with no error envelope.
    Failed,
    /// The gateway returned an error envelope (`error_code` present).
    ErrorEnvelope,
    /// The rows do not carry the requested date (no row matches).
    DateMismatch,
    /// The requested date is before the KTD7 lower bound (2010-01-04).
    Pre2010,
    /// No qualifying KOSPI row for the requested date is present.
    NoQualifyingRow,
}

/// The outcome of applying the witness rule to a response: either a valid PositiveWitness
/// [`EvidenceRecord`] dated to the requested date, or typed [`NonWitnessReason`].
#[derive(Debug, Clone, PartialEq)]
pub enum WitnessOutcome {
    /// The response qualified: a valid PositiveWitness record (see [`witness_from_response`]
    /// for how the placeholder id/source_id are assigned).
    Witness(EvidenceRecord),
    /// The response did not qualify — no evidence is produced.
    NonEvidence(NonWitnessReason),
}

/// The derivable placeholder id stamped on a witness produced by [`witness_from_response`]
/// (`krx-witness-YYYY-MM-DD`). U14 replaces it with its own stable id.
pub fn default_witness_id(date: NaiveDate) -> String {
    format!("krx-witness-{date}")
}

/// Build a valid PositiveWitness [`EvidenceRecord`] dated to `date`, stamped with the
/// supplied `id`, `source_id`, and `recorded_at`. The ergonomic construction path for U14.
///
/// The record is always `valid`, un-superseded, and un-cited (a positive witness needs no
/// citation — see the reconciliation matrix).
pub fn build_witness_record(
    date: NaiveDate,
    id: impl Into<String>,
    source_id: impl Into<String>,
    recorded_at: DateTime<Utc>,
) -> EvidenceRecord {
    EvidenceRecord {
        id: id.into(),
        source_id: source_id.into(),
        date,
        kind: EvidenceKind::PositiveWitness,
        valid: true,
        superseded_by: None,
        citation: None,
        recorded_at,
    }
}

/// Apply the KRX daily-market positive-witness rule (KTD7) to an already-parsed `resp`.
///
/// Returns [`WitnessOutcome::Witness`] ONLY when the response is successful, structurally
/// valid, non-empty, dated >= 2010-01-04, and carries a qualifying KOSPI row on the
/// requested date; otherwise [`WitnessOutcome::NonEvidence`] with the typed reason. Pure
/// and side-effect-free — a non-qualifying response produces NO record (never `Closed`,
/// never a retraction).
pub fn witness_from_response(resp: &KrxDailyMarketResponse) -> WitnessOutcome {
    use WitnessOutcome::NonEvidence;

    // An error envelope is decisive regardless of the `success` flag.
    if resp.error_code.is_some() {
        return NonEvidence(NonWitnessReason::ErrorEnvelope);
    }
    // A failed response (no envelope) is non-evidence.
    if !resp.success {
        return NonEvidence(NonWitnessReason::Failed);
    }
    // A successful response with no rows is an empty (absence) response.
    if resp.rows.is_empty() {
        return NonEvidence(NonWitnessReason::Empty);
    }
    // A structurally broken response (any blank-market row) is malformed.
    if resp.rows.iter().any(|r| !r.is_structurally_valid()) {
        return NonEvidence(NonWitnessReason::Malformed);
    }
    // Out of KTD7 scope: a request before the daily-market lower bound.
    let (y, m, day) = MIN_WITNESS_DATE;
    let min_date = NaiveDate::from_ymd_opt(y, m, day).expect("MIN_WITNESS_DATE is valid");
    if resp.requested_date < min_date {
        return NonEvidence(NonWitnessReason::Pre2010);
    }
    // The rows must actually cover the requested date.
    if !resp.rows.iter().any(|r| r.date == resp.requested_date) {
        return NonEvidence(NonWitnessReason::DateMismatch);
    }
    // And one of them must be a qualifying KOSPI row for that date.
    if !resp
        .rows
        .iter()
        .any(|r| r.is_qualifying_kospi(resp.requested_date))
    {
        return NonEvidence(NonWitnessReason::NoQualifyingRow);
    }

    // Qualified — emit a valid PositiveWitness with derivable placeholder id/source_id and a
    // deterministic placeholder recorded_at (midnight UTC of the date). U14 overwrites these.
    let recorded_at = Utc.from_utc_datetime(
        &resp
            .requested_date
            .and_hms_opt(0, 0, 0)
            .expect("midnight is valid"),
    );
    WitnessOutcome::Witness(build_witness_record(
        resp.requested_date,
        default_witness_id(resp.requested_date),
        KRX_DAILY_MARKET_SOURCE_HINT,
        recorded_at,
    ))
}
