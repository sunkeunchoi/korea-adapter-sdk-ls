//! Pure candidate scorers for the daily lineage's freezable ranking signals.
//!
//! Selection dispatch and refusal recording stay in `daily.rs`; this file owns the
//! candidate formulas so every implementation is included in `DAILY_SOURCE`.

use crate::strategy::orb::UniverseCandidate;

/// Prior-session turnover, descending.
pub(super) fn prior_turnover_desc(candidate: &UniverseCandidate) -> Option<f64> {
    candidate.prior_turnover.is_finite().then_some(candidate.prior_turnover)
}

/// Twelve-session price momentum excluding the immediately prior session.
///
/// `prior_closes` is oldest-to-newest and contains bars strictly before the decision
/// session. Thirteen bars are required: the oldest close is the denominator, the
/// penultimate close is the numerator, and the newest close is the excluded 1-session
/// reversal window. Higher return ranks first.
pub(super) fn momentum_12x1(prior_closes: &[f64]) -> Option<f64> {
    if prior_closes.len() < 13 {
        return None;
    }
    let window = &prior_closes[prior_closes.len() - 13..];
    let start = window[0];
    let end = window[11];
    if !start.is_finite() || start <= 0.0 || !end.is_finite() {
        return None;
    }
    let score = end / start - 1.0;
    score.is_finite().then_some(score)
}

