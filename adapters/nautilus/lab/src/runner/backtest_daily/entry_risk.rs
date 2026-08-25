//! The entry-risk projection seam (KTD3, R12): the join that carries the
//! client-order-keyed entry-risk ledger into the order of the single post-`end()`
//! cache read, plus the three assertions that catch the three ways it can break.

use std::collections::HashMap;

use nautilus_model::identifiers::ClientOrderId;
use nautilus_model::position::Position;

use crate::artifacts::performance::{ClientOrderEntryRiskLedger, EntryRisk};

/// The entry-risk ledger projected into the order of the single post-`end()` cache
/// read (KTD3, R12) — the `Vec<Option<EntryRisk>>` that
/// [`crate::artifacts::performance::PerformanceReport::from_positions_with_risk`]
/// consumes, plus the evidence its three seam assertions need.
///
/// `from_positions_with_risk` is **index-aligned, not keyed** (`risks.get(i)` for
/// `positions[i]`), and its doc records that a short slice silently leaves trailing
/// positions risk-less. Three distinct defects break three different statistics:
///
/// - **Truncation** sets `all_have_risk = false` and collapses `return_on_risk` to
///   `None` entirely.
/// - **Collapse** — several positions on a symbol joining to one risk — makes
///   `Σ risk_capital` and therefore net RoR wrong.
/// - **Mis-ordering** is a permutation: it leaves `Σ risk_capital` invariant and net
///   RoR unaffected, but corrupts per-trade `realized_r`, `mean_realized_r`, and the
///   per-symbol `max_risk_capital_share` fold.
///
/// Length equality catches the first, the `Some`-count catches the second, and only
/// the opening-order-id equality catches the third — a projection built in *ledger*
/// order rather than cache-read order has the right length and every slot filled.
///
/// The risk and the key it came from are held in **one** slot rather than two
/// parallel vectors, so no mutation can permute the risks while leaving the witness
/// keys in place.
#[derive(Debug, Clone, PartialEq)]
pub struct EntryRiskProjection {
    slots: Vec<Option<(ClientOrderId, EntryRisk)>>,
    opened_entries: usize,
    unopened_entries: Vec<ClientOrderId>,
}

impl EntryRiskProjection {
    /// Assemble a projection from already-computed parts.
    ///
    /// Production code calls [`project_entry_risks`]; this constructor exists so the
    /// three seam assertions can be exercised against a deliberately broken
    /// projection (a shortened slice, a collapsed ledger, a permutation) without the
    /// test having to reimplement the join.
    #[must_use]
    pub fn from_parts(
        slots: Vec<Option<(ClientOrderId, EntryRisk)>>,
        opened_entries: usize,
        unopened_entries: Vec<ClientOrderId>,
    ) -> Self {
        EntryRiskProjection { slots, opened_entries, unopened_entries }
    }

    /// The index-aligned slots: slot `i` is the ledger entry that opened
    /// `positions[i]`, paired with the client order id it was keyed by.
    #[must_use]
    pub fn slots(&self) -> &[Option<(ClientOrderId, EntryRisk)>] {
        &self.slots
    }

    /// The index-aligned risk slice `from_positions_with_risk` consumes.
    #[must_use]
    pub fn risks(&self) -> Vec<Option<EntryRisk>> {
        self.slots.iter().map(|s| s.map(|(_, r)| r)).collect()
    }

    /// How many recorded ledger entries the **stream** observed opening a position.
    /// This is the reconciliation basis for the `Some`-count assertion, and it is
    /// read off the ledger's stream-side witness rather than off the cache read or
    /// the slots — so neither a defective projection nor a cache read that has
    /// silently lost positions can move its own expectation.
    #[must_use]
    pub const fn opened_entries(&self) -> usize {
        self.opened_entries
    }

    /// Recorded entries whose order never opened a position — a venue or risk-engine
    /// rejection, or an order that never filled. A **named run-level diagnostic**,
    /// deliberately not a hard failure: the `Some`-count assertion is defined over
    /// the entries that opened a position precisely so a rejection reports rather
    /// than aborting an otherwise valid run.
    #[must_use]
    pub fn unopened_entries(&self) -> &[ClientOrderId] {
        &self.unopened_entries
    }

    /// The three seam assertions (KTD3). Panics — this is a corrupt-join guard on the
    /// denominator of the verdict statistic, not a recoverable condition.
    pub fn assert_aligned(&self, positions: &[Position]) {
        // (1) Truncation. A short slice leaves trailing positions risk-less, which
        // sets `all_have_risk = false` and collapses `return_on_risk` to `None`.
        assert_eq!(
            self.slots.len(),
            positions.len(),
            "entry-risk projection length {} != position count {} (KTD3 assertion 1): a short \
             slice silently leaves trailing positions risk-less and collapses return_on_risk",
            self.slots.len(),
            positions.len()
        );

        // (2) Collapse. Several positions joining to one ledger entry passes (1) but
        // makes Σ risk_capital — the denominator of net RoR — wrong.
        let filled = self.slots.iter().filter(|s| s.is_some()).count();
        assert_eq!(
            filled,
            self.opened_entries,
            "entry-risk projection filled {filled} of {} slots but the stream observed {} \
             recorded ledger entries opening a position (KTD3 assertion 2): a collapsed join or \
             a cache read that lost positions makes Σ risk_capital and therefore net \
             return_on_risk wrong",
            self.slots.len(),
            self.opened_entries
        );

        // (3) Permutation. Right length, every slot filled, every risk on the wrong
        // position: Σ risk_capital is invariant and net RoR unaffected, but per-trade
        // realized_r, mean_realized_r, and the per-symbol max_risk_capital_share fold
        // are all corrupt. The key is already on the read side, so this costs nothing.
        for (i, slot) in self.slots.iter().enumerate() {
            let Some((key, _)) = slot else { continue };
            assert_eq!(
                *key,
                positions[i].opening_order_id,
                "entry-risk projection slot {i} carries client order id {key} but position {} \
                 was opened by {} (KTD3 assertion 3): the projection is a permutation of \
                 cache-read order, which corrupts realized_r and max_risk_capital_share while \
                 leaving Σ risk_capital invariant",
                positions[i].id,
                positions[i].opening_order_id
            );
        }
    }
}

/// Project the client-order-keyed entry-risk ledger into the order of the single
/// post-`end()` cache read (KTD3, R12), joining on `Position.opening_order_id`.
///
/// The projection is built by walking `positions` — the cache read defines the order
/// — and never by walking the ledger, which would produce a permutation that both
/// count assertions pass.
///
/// A position with no recorded entry resolves to `None` and takes the legacy P&L path
/// rather than panicking.
#[must_use]
pub fn project_entry_risks(
    positions: &[Position],
    ledger: &ClientOrderEntryRiskLedger,
) -> EntryRiskProjection {
    let by_order: HashMap<ClientOrderId, EntryRisk> = ledger.snapshot().into_iter().collect();

    let slots: Vec<Option<(ClientOrderId, EntryRisk)>> = positions
        .iter()
        .map(|p| by_order.get(&p.opening_order_id).map(|r| (p.opening_order_id, *r)))
        .collect();

    EntryRiskProjection {
        slots,
        // Stream-side, not cache-read-side: see `EntryRiskProjection::opened_entries`.
        opened_entries: ledger.opened_entries().len(),
        unopened_entries: ledger.unopened_entries(),
    }
}
