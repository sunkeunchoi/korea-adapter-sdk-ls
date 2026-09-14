//! `rehearsal/book.json` (U9, KTD11) — the record of what the BROKER
//! confirmed at the end of the last session, and the leg facts only this file remembers.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use nautilus_ls::execution::ExpectedBook;
use nautilus_model::identifiers::{ClientOrderId, InstrumentId};
use serde::{Deserialize, Serialize};

use crate::runner::pnl::BookLeg as PnlBookLeg;
use crate::strategy::daily::BookLeg as StrategyBookLeg;

use super::*;

// ---------------------------------------------------------------------------
// rehearsal/book.json (KTD11)
// ---------------------------------------------------------------------------

/// One broker-confirmed overnight leg, as `rehearsal/book.json` records it.
///
/// The broker can confirm only the symbol and the quantity (that is all
/// [`ExpectedBook`] carries, and all `verify_book_on` checks). Everything else here is a
/// leg fact only this file remembers — which is exactly why a leg missing one of them
/// refuses the mount rather than getting a default: a stop invented at restore time is a
/// stop the frozen mechanism never set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RehearsalBookLeg {
    /// The 6-digit KRX short code.
    pub shcode: String,
    /// Held quantity — positive (the daily lineage is long-only).
    pub quantity: i64,
    /// The realized entry fill price.
    pub entry_price: f64,
    /// The entry-fixed stop, carried unchanged across sessions (R12).
    pub stop_price: f64,
    /// The PRIOR session's close in integer KRW — the breaker's day basis (KTD13).
    pub prior_close: i64,
    /// The KST session date (`YYYY-MM-DD`) the leg opened on.
    ///
    /// A DATE, not the positional ordinal the strategy counts holds in. The ordinal is an
    /// index into a session list rebuilt every mount, and its backward half is the proven
    /// sessions — so a past `Unknown` day resolving to a session shifts every later index by
    /// one. A persisted index would then silently mean a different session than the one the
    /// leg opened on, lengthening or shortening every open hold with no diff and no error.
    /// The date is stable under that resolution; [`RehearsalBook::strategy_legs`] converts it
    /// back to an index against the CURRENT list, so entry and current always share a space.
    pub entry_date: String,
    /// Which run opened the leg (KTD2's leg-level label): a book may carry legs entered
    /// under a placeholder signal and legs entered under the frozen one, and the
    /// comparison report must not mix them.
    pub entered_under: String,
    /// The opening order's client id, used to seed the risk-ledger join.
    pub opening_order_id: String,
}

/// `rehearsal/book.json` — what the account held when the last session's teardown
/// confirmed it (KTD11).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RehearsalBook {
    /// Schema version.
    pub version: u32,
    /// The KST session date (`YYYY-MM-DD`) whose teardown wrote this snapshot.
    pub session_date: String,
    /// The run that wrote it.
    pub run_id: String,
    /// The epoch the session list is counted from — recorded so a base change is a refusal
    /// rather than a silent re-dating of every hold.
    pub ordinal_epoch: String,
    /// The legs, shcode-ordered.
    pub legs: Vec<RehearsalBookLeg>,
}

impl RehearsalBook {
    /// The empty book a freshly bootstrapped rehearsal home starts from.
    #[must_use]
    pub fn empty(session_date: &str) -> Self {
        RehearsalBook {
            version: BOOK_VERSION,
            session_date: session_date.to_string(),
            run_id: String::new(),
            ordinal_epoch: SESSION_ORDINAL_EPOCH.to_string(),
            legs: Vec::new(),
        }
    }

    /// `<data_home>/rehearsal/book.json`.
    #[must_use]
    pub fn path(data_home: &Path) -> PathBuf {
        data_home.join("rehearsal").join("book.json")
    }

    /// Restore the book, or an empty one when the file does not exist yet.
    ///
    /// An ABSENT file is a flat start, which is legitimate on a freshly bootstrapped home.
    /// An UNREADABLE file is not: it is a book that exists and cannot be understood, and
    /// treating that as flat would make the pre-mount probe refuse every held position as
    /// "unexpected" — or, worse on an account that really is flat, pass.
    ///
    /// # Errors
    ///
    /// A read/parse failure, or an unknown `version`/`ordinal_epoch`.
    pub fn load(data_home: &Path) -> anyhow::Result<Self> {
        let path = Self::path(data_home);
        if !path.exists() {
            return Ok(RehearsalBook::empty(""));
        }
        let bytes = std::fs::read(&path)
            .map_err(|e| anyhow::anyhow!("reading {}: {e}", path.display()))?;
        let book: RehearsalBook = serde_json::from_slice(&bytes)
            .map_err(|e| anyhow::anyhow!("parsing {}: {e}", path.display()))?;
        if book.version != BOOK_VERSION {
            anyhow::bail!(
                "rehearsal/book.json is version {} and this binary writes version \
                 {BOOK_VERSION} — refusing rather than restoring legs from fields it may be \
                 misreading. A version-1 book stored each leg's entry as a positional session \
                 ordinal, which cannot be converted here: resolving it back to a date needs the \
                 calendar it was counted against, and that calendar has since moved. Re-adopt \
                 the book from the account",
                book.version
            );
        }
        if book.ordinal_epoch != SESSION_ORDINAL_EPOCH {
            anyhow::bail!(
                "rehearsal/book.json counts session ordinals from {} but this binary counts from \
                 {SESSION_ORDINAL_EPOCH} — every open hold would be re-dated silently; adopt the \
                 book to re-anchor it",
                book.ordinal_epoch
            );
        }
        Ok(book)
    }

    /// Every field the broker cannot confirm (KTD6).
    ///
    /// # Errors
    ///
    /// The offending legs, named.
    pub fn validate_fields(&self) -> anyhow::Result<()> {
        let mut bad: Vec<String> = Vec::new();
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        for leg in &self.legs {
            let why = if leg.shcode.trim().len() != 6 || !leg.shcode.bytes().all(|b| b.is_ascii_digit()) {
                Some("shcode is not six digits".to_string())
            } else if !seen.insert(leg.shcode.trim()) {
                Some("duplicate leg for this symbol".to_string())
            } else if leg.quantity <= 0 {
                Some(format!("quantity {} is not positive", leg.quantity))
            } else if !leg.entry_price.is_finite() || leg.entry_price <= 0.0 {
                Some(format!("entry_price {} is not a positive number", leg.entry_price))
            } else if !leg.stop_price.is_finite() || leg.stop_price <= 0.0 {
                Some(format!("stop_price {} is not a positive number", leg.stop_price))
            } else if leg.stop_price >= leg.entry_price {
                // The risk per share IS `entry − stop` (R12), so a non-positive one makes
                // every derived size and realized-r meaningless rather than merely odd.
                Some(format!(
                    "stop_price {} is not below entry_price {} — the entry-fixed risk per share \
                     would be non-positive",
                    leg.stop_price, leg.entry_price
                ))
            } else if leg.prior_close <= 0 {
                Some(format!("prior_close {} is not positive", leg.prior_close))
            } else if NaiveDate::parse_from_str(leg.entry_date.trim(), "%Y-%m-%d").is_err() {
                Some(format!("entry_date {:?} is not a YYYY-MM-DD date", leg.entry_date))
            } else if leg.entered_under.trim().is_empty() {
                Some("entered_under is empty".to_string())
            } else if leg.opening_order_id.trim().is_empty() {
                Some("opening_order_id is empty".to_string())
            } else {
                None
            };
            if let Some(why) = why {
                bad.push(format!("{}: {why}", leg.shcode.trim()));
            }
        }
        if bad.is_empty() {
            return Ok(());
        }
        anyhow::bail!(
            "rehearsal/book.json carries {} unusable leg(s) — {}. These are facts the broker \
             cannot supply, so the session refuses rather than inventing them; repair the book or \
             re-adopt it",
            bad.len(),
            bad.join("; ")
        )
    }

    /// Refuse a book whose stamp is not the immediately preceding proven trading session
    /// (KTD11).
    ///
    /// Freshness is measured against the PROVEN calendar, not against "yesterday": a Monday
    /// mount inherits Friday's book, and a mount after a four-day holiday inherits the book
    /// from before it. A stamp older than that means at least one session's teardown never
    /// wrote — so the legs, the ordinals, and the stops are all from a state the account has
    /// since moved on from.
    ///
    /// An empty book with an empty stamp is exempt: a bootstrapped home has no previous
    /// session to be stale against.
    ///
    /// # Errors
    ///
    /// A stale or future stamp.
    pub fn assert_fresh(&self, previous_session: NaiveDate) -> anyhow::Result<()> {
        if self.legs.is_empty() && self.session_date.trim().is_empty() {
            return Ok(());
        }
        let stamped = NaiveDate::parse_from_str(self.session_date.trim(), "%Y-%m-%d").map_err(|e| {
            anyhow::anyhow!(
                "rehearsal/book.json session_date {:?} is not a YYYY-MM-DD KST session date ({e})",
                self.session_date
            )
        })?;
        // STRICTLY older is the refusal. Equality is the ordinary case, but a stamp BETWEEN
        // the last proven session and today is healthy too: the KRX witness is retrospective,
        // so yesterday's session is routinely still `Unknown` this morning and a book written
        // by yesterday's own teardown carries a date the calendar has not caught up to.
        // Refusing on inequality would fail a correct book for a calendar lag the operator
        // cannot fix, which is the shape that trains people to bypass the gate.
        if stamped < previous_session {
            anyhow::bail!(
                "rehearsal/book.json is stamped {stamped} but the previous proven trading session \
                 is {previous_session} — a session's teardown did not write the book, so its legs, \
                 dates and stops predate the account's current state. Repair the book against the \
                 account before mounting"
            );
        }
        Ok(())
    }

    /// The broker-confirmable projection: symbol → quantity.
    #[must_use]
    pub fn expected_book(&self) -> ExpectedBook {
        ExpectedBook::from_pairs(
            self.legs.iter().map(|l| (l.shcode.trim().to_string(), l.quantity)),
        )
    }

    /// The breaker's accounting projection of the legs (U8, KTD13).
    #[must_use]
    pub fn pnl_legs(&self) -> Vec<PnlBookLeg> {
        self.legs
            .iter()
            .map(|l| PnlBookLeg {
                symbol: l.shcode.trim().to_string(),
                qty: l.quantity,
                prior_close: l.prior_close,
                // The book's own stop, so an overnight holding that never prints this
                // session is still marked at a bound rather than the configured worst case.
                stop_price: Some(l.stop_price.round() as i64),
            })
            .collect()
    }

    /// The strategy's restore projection (`seed_open_legs`), with each leg's entry ordinal
    /// re-derived against THIS mount's session list.
    ///
    /// The conversion is the point. The strategy counts a hold as `current_index -
    /// entry_index` into one list, and that list is rebuilt every mount — so the only way the
    /// subtraction can stay honest across a calendar that keeps resolving `Unknown` days is to
    /// persist the date and resolve it here, against the same list `current_index` came from.
    ///
    /// A leg whose entry date is not IN the list is refused rather than clamped: it means the
    /// calendar no longer agrees that a session happened on the day this position opened, and
    /// silently snapping it to a neighbour would change the hold length of a live position.
    ///
    /// # Errors
    ///
    /// An shcode that does not map to a KRX instrument id, an unparseable entry date, or an
    /// entry date absent from `sessions`.
    pub fn strategy_legs(&self, sessions: &[NaiveDate]) -> anyhow::Result<Vec<StrategyBookLeg>> {
        self.legs
            .iter()
            .map(|l| {
                let entered = NaiveDate::parse_from_str(l.entry_date.trim(), "%Y-%m-%d")
                    .map_err(|e| {
                        anyhow::anyhow!(
                            "leg {} carries entry_date {:?}, which is not a YYYY-MM-DD KST \
                             session date ({e})",
                            l.shcode.trim(),
                            l.entry_date
                        )
                    })?;
                let entry_session_ordinal = sessions.binary_search(&entered).map_err(|_| {
                    anyhow::anyhow!(
                        "leg {} opened on {entered}, but this mount's session list does not \
                         contain that day — the calendar no longer agrees a session happened \
                         then, so the leg's hold length cannot be measured. Reconcile the book \
                         rather than letting the hold silently change",
                        l.shcode.trim()
                    )
                })?;
                Ok(StrategyBookLeg {
                    instrument_id: instrument_id_for(l.shcode.trim())?,
                    opening_order_id: ClientOrderId::from(l.opening_order_id.trim()),
                    entry_price: l.entry_price,
                    stop_price: l.stop_price,
                    quantity: l.quantity as f64,
                    entry_session_ordinal,
                })
            })
            .collect()
    }

    /// Write the book atomically (tmp + rename — the `RunWriter` precedent).
    ///
    /// # Errors
    ///
    /// A create/write/rename failure.
    pub fn write(&self, data_home: &Path) -> anyhow::Result<PathBuf> {
        let path = Self::path(data_home);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("json.tmp");
        let mut text = serde_json::to_string_pretty(self)?;
        text.push('\n');
        std::fs::write(&tmp, text)?;
        // Rename is the atomic step: a crash mid-write leaves the PREVIOUS book intact,
        // which is the one state the next session's probe can still act on.
        std::fs::rename(&tmp, &path)?;
        Ok(path)
    }

    /// The next session's book, assembled from the teardown's t0424 snapshot plus the leg
    /// facts only this file remembers (KTD11).
    ///
    /// The snapshot is AUTHORITATIVE for membership and quantity: a symbol the broker no
    /// longer reports is gone from the book even if the ledger thought it was held, and a
    /// partially-exited leg carries the broker's remaining quantity. What carries over from
    /// the previous book is the ENTRY-FIXED state — the stop, the ordinal, the label — and a
    /// leg the previous book has never seen comes from `entered`.
    ///
    /// `closes` re-bases every leg's `prior_close` on THIS session's close. That field is
    /// the only one here that is not entry-fixed: it is the day basis the next session's
    /// breaker marks against (KTD13), so a leg held for twelve sessions carries twelve
    /// different values for it and exactly one of them is right on any given day.
    #[must_use]
    pub fn from_snapshot(
        snapshot: &std::collections::BTreeMap<String, i64>,
        previous: &RehearsalBook,
        entered: &[RehearsalBookLeg],
        closes: &HashMap<String, i64>,
        session_date: &str,
        run_id: &str,
    ) -> (Self, Vec<String>) {
        let mut known: HashMap<&str, &RehearsalBookLeg> = HashMap::new();
        for leg in previous.legs.iter().chain(entered.iter()) {
            // A freshly entered leg WINS over an inherited one of the same symbol: under
            // Netting a re-entry after an exit is the same position id, and the live facts
            // are the new entry's.
            known.insert(leg.shcode.trim(), leg);
        }
        let legs = snapshot
            .iter()
            .filter(|(_, qty)| **qty > 0)
            .filter_map(|(symbol, qty)| {
                known.get(symbol.trim()).map(|leg| RehearsalBookLeg {
                    quantity: *qty,
                    // KTD13: `prior_close` is the DAY basis the next session's breaker marks
                    // against, so it advances every session. Carrying the previous value
                    // forward would make an inherited leg's open mark an inception-to-date
                    // P&L — the exact one-sided error `seed_book_legs` exists to prevent —
                    // and it would grow with every session the leg is held.
                    prior_close: closes
                        .get(symbol.trim())
                        .copied()
                        .filter(|c| *c > 0)
                        .unwrap_or(leg.prior_close),
                    ..(*leg).clone()
                })
            })
            .collect();
        // A basis that could NOT be advanced is reported, never written silently. The stamp
        // moves to today regardless — the broker confirmed these holdings today — so a leg
        // keeping yesterday's `prior_close` is a book that LOOKS current and prices a
        // multi-session move as if it were one day's. The next mount's probe checks dates and
        // quantities, so nothing downstream would catch it; the breaker would simply measure
        // the wrong day. This happens on the paths that never read a close: an R33 no-decision
        // session, `--stop-before-orders`, and a failed post-auction read.
        let stale_basis: Vec<String> = snapshot
            .iter()
            .filter(|(_, qty)| **qty > 0)
            .map(|(s, _)| s.trim())
            .filter(|s| !closes.get(*s).is_some_and(|c| *c > 0))
            .filter(|s| known.contains_key(*s))
            .map(str::to_string)
            .collect();
        (
            RehearsalBook {
                version: BOOK_VERSION,
                session_date: session_date.to_string(),
                run_id: run_id.to_string(),
                ordinal_epoch: SESSION_ORDINAL_EPOCH.to_string(),
                legs,
            },
            stale_basis,
        )
    }

    /// Symbols the broker reports but whose leg facts this book cannot supply.
    ///
    /// [`Self::from_snapshot`] DROPS them, because a leg with no stop is a leg the strategy
    /// refuses to restore — so they must be reported rather than quietly lost: the next
    /// session's probe will refuse on them, and the operator needs to know why before they
    /// get there.
    #[must_use]
    pub fn unattributed(
        snapshot: &std::collections::BTreeMap<String, i64>,
        previous: &RehearsalBook,
        entered: &[RehearsalBookLeg],
    ) -> Vec<String> {
        let known: BTreeSet<&str> = previous
            .legs
            .iter()
            .chain(entered.iter())
            .map(|l| l.shcode.trim())
            .collect();
        snapshot
            .iter()
            .filter(|(_, qty)| **qty > 0)
            .map(|(s, _)| s.trim())
            .filter(|s| !known.contains(s))
            .map(str::to_string)
            .collect()
    }
}

/// `{shcode}.XKRX`.
///
/// # Errors
///
/// An shcode that is not six ASCII digits.
pub fn instrument_id_for(shcode: &str) -> anyhow::Result<InstrumentId> {
    let code = shcode.trim();
    if code.len() != 6 || !code.bytes().all(|b| b.is_ascii_digit()) {
        anyhow::bail!("{code:?} is not a six-digit KRX short code");
    }
    Ok(InstrumentId::from(format!("{code}.{}", nautilus_ls::KRX_VENUE).as_str()))
}

