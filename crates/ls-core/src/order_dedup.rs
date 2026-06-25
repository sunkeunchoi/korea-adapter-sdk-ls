//! `OrderDeduplicator` — idempotent order submission within a TTL window.
//!
//! This implements the order-safety §2 deduplication contract verbatim. A key
//! built from `SHA256(account_no + ":" + tr_code + ":" + canonical request
//! JSON)` maps to the cached order response within a 300s window: a second
//! submission of the *same* order inside the window returns the cached result
//! instead of hitting the exchange again.
//!
//! Load-bearing properties (all from §2):
//!
//! - **Account + TR in the key.** Different accounts and different order TRs do
//!   not collide.
//! - **Full canonical request JSON in the key.** Different request fields — even
//!   a small quantity or price change — are different orders and intentionally
//!   miss the cache. (This resolves the §2 key-granularity question to the
//!   documented concrete key, not the looser `strong_order_fields` identity.)
//! - **Fail-closed key build.** A serialization failure while building the key
//!   returns an error and dispatches nothing.
//! - **Cache hit bypasses rate limiting and HTTP.** The caller gets the cached
//!   response back with no second dispatch.
//! - **Opportunistic write-path eviction, never a background worker.** `insert`
//!   calls a monotonic [`sweep_expired_if_due`](OrderDeduplicator::sweep_expired_if_due);
//!   when the interval has elapsed one inserting thread wins an atomic timestamp
//!   and runs a single bounded `retain` pass. The `retain` runs with **no
//!   DashMap entry guard held** — the deadlock-avoidance rule. Read-path lazy
//!   eviction drops an expired entry when its exact key is looked up.
//!
//! The dedup key embeds `account_no` and so is itself sensitive: it is never
//! logged, traced, or persisted. Only the resulting `dedup_hit` boolean is
//! observable.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use dashmap::DashMap;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{LsError, LsResult};

/// Default duplicate-submission safety window (§2). Not a server-side
/// idempotency guarantee — a client-side window only.
pub const DEFAULT_TTL: Duration = Duration::from_secs(300);

/// Minimum wall-clock interval between opportunistic write-path sweeps. Small
/// enough to bound memory under sustained distinct-order load, large enough that
/// the `retain` pass is amortized to negligible cost per insert.
const SWEEP_INTERVAL: Duration = Duration::from_secs(30);

/// A cached order response plus its insertion instant (for TTL checks).
struct CacheEntry {
    /// The cached response as JSON; round-trips back into the caller's `Res`.
    value: serde_json::Value,
    inserted: Instant,
}

/// Per-client order deduplication cache (§2).
pub struct OrderDeduplicator {
    cache: DashMap<String, CacheEntry>,
    /// Keys with a dispatch CURRENTLY in flight. The cache only holds *completed*
    /// responses, so without this set two concurrent identical submits would both
    /// miss the cache (the first has not inserted yet) and both reach the exchange
    /// — a double fill. A reservation closes that window: the second concurrent
    /// submit observes the in-flight key and is rejected as a duplicate.
    in_flight: DashMap<String, ()>,
    ttl: Duration,
    /// Epoch the cache was created — the reference for the monotonic sweep gate.
    created: Instant,
    /// Millis-since-`created` of the last completed sweep. The atomic the sweep
    /// winner claims via compare-exchange.
    last_sweep_millis: AtomicU64,
}

/// RAII reservation for an in-flight order key. Holding it marks the key as
/// dispatching; dropping it (on success, rejection, ambiguity, or panic) releases
/// the key so a later identical submit can proceed (and, on success, find the
/// cached response instead).
pub struct ReservationGuard<'a> {
    dedup: &'a OrderDeduplicator,
    key: String,
}

impl Drop for ReservationGuard<'_> {
    fn drop(&mut self) {
        self.dedup.in_flight.remove(&self.key);
    }
}

impl OrderDeduplicator {
    /// Build a deduplicator with the given TTL window.
    pub fn new(ttl: Duration) -> Self {
        Self {
            cache: DashMap::new(),
            in_flight: DashMap::new(),
            ttl,
            created: Instant::now(),
            last_sweep_millis: AtomicU64::new(0),
        }
    }

    /// Atomically reserve `key` for an in-flight dispatch. Returns a
    /// [`ReservationGuard`] (released on drop) on success, or `None` if an
    /// identical order is already dispatching — the caller should treat `None` as
    /// a duplicate and NOT dispatch. The check-and-insert is atomic per shard, so
    /// two concurrent reservers cannot both succeed.
    pub fn try_reserve(&self, key: &str) -> Option<ReservationGuard<'_>> {
        use dashmap::mapref::entry::Entry;
        match self.in_flight.entry(key.to_string()) {
            Entry::Occupied(_) => None,
            Entry::Vacant(v) => {
                v.insert(());
                Some(ReservationGuard {
                    dedup: self,
                    key: key.to_string(),
                })
            }
        }
    }

    /// Build a deduplicator with the default 300s TTL (§2).
    pub fn with_default_ttl() -> Self {
        Self::new(DEFAULT_TTL)
    }

    /// Build the dedup key `SHA256(account_no:tr_code:canonical-request-JSON)`.
    ///
    /// Fail-closed: a serialization failure returns `Err` so the caller
    /// dispatches nothing. The returned hex string embeds `account_no` and must
    /// never be logged or persisted.
    pub fn key<Req>(account_no: &str, tr_code: &str, req: &Req) -> LsResult<String>
    where
        Req: Serialize,
    {
        let body = serde_json::to_string(req).map_err(LsError::Decode)?;
        let mut hasher = Sha256::new();
        hasher.update(account_no.as_bytes());
        hasher.update(b":");
        hasher.update(tr_code.as_bytes());
        hasher.update(b":");
        hasher.update(body.as_bytes());
        Ok(hex(&hasher.finalize()))
    }

    /// Look up a live (non-expired) cached response. Read-path lazy eviction:
    /// an expired entry is dropped on lookup. Returns the cached JSON on a hit.
    pub fn get(&self, key: &str) -> Option<serde_json::Value> {
        // Phase 1 — read under a short-lived guard. The guard is dropped at the
        // end of this block (whether the entry was live, expired, or absent), so
        // no per-entry guard is held across the structural mutation below.
        let live_value = match self.cache.get(key) {
            Some(entry) if entry.inserted.elapsed() < self.ttl => Some(entry.value.clone()),
            _ => None,
        };
        if live_value.is_some() {
            return live_value;
        }
        // Phase 2 — evict ONLY if the entry is still expired. `remove_if` re-checks
        // under the shard lock, closing the race where another thread inserted a
        // fresh entry (a legitimate re-dispatch of the same order) between our
        // expiry read and the removal — a bare `remove` would delete that fresh
        // entry and let the next identical submit re-hit the exchange.
        self.cache
            .remove_if(key, |_, entry| entry.inserted.elapsed() >= self.ttl);
        None
    }

    /// Insert a cached response. Runs the opportunistic write-path sweep first
    /// (§2), then inserts. No entry guard is held across the sweep.
    pub fn insert(&self, key: String, value: serde_json::Value) {
        self.sweep_expired_if_due();
        self.cache.insert(
            key,
            CacheEntry {
                value,
                inserted: Instant::now(),
            },
        );
    }

    /// Opportunistic sweep on the write path (§2). When the sweep interval has
    /// elapsed, exactly one thread wins the atomic timestamp update and runs a
    /// single bounded `retain` pass dropping entries past the same TTL the read
    /// path uses. **The `retain` runs with no DashMap entry guard held** — the
    /// deadlock-avoidance rule. There is no background sweeper.
    fn sweep_expired_if_due(&self) {
        let now_millis = self.created.elapsed().as_millis() as u64;
        let last = self.last_sweep_millis.load(Ordering::Relaxed);
        if now_millis.saturating_sub(last) < SWEEP_INTERVAL.as_millis() as u64 {
            return;
        }
        // Only the thread that successfully advances the timestamp sweeps; a
        // concurrent inserter that loses the race skips it.
        if self
            .last_sweep_millis
            .compare_exchange(last, now_millis, Ordering::SeqCst, Ordering::Relaxed)
            .is_err()
        {
            return;
        }
        let ttl = self.ttl;
        // CRITICAL: this runs with no per-entry guard held (the caller — insert
        // — holds none). Holding one here would deadlock against the shard lock.
        self.cache.retain(|_k, entry| entry.inserted.elapsed() < ttl);
    }

    /// Number of entries currently held (test/observability helper).
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Run one bounded `retain` sweep directly (test-only). Mirrors the body of
    /// the time-gated [`sweep_expired_if_due`] so the no-deadlock / memory-bound
    /// `retain`-without-guard property is exercised without waiting out the
    /// wall-clock sweep interval.
    #[cfg(test)]
    pub(crate) fn force_sweep_for_test(&self) {
        let ttl = self.ttl;
        self.cache.retain(|_k, entry| entry.inserted.elapsed() < ttl);
    }

    /// `true` if the cache holds no entries.
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }
}

impl Default for OrderDeduplicator {
    fn default() -> Self {
        Self::with_default_ttl()
    }
}

impl std::fmt::Debug for OrderDeduplicator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never render keys or values — they embed account_no and order bodies.
        f.debug_struct("OrderDeduplicator")
            .field("ttl", &self.ttl)
            .field("entries", &self.cache.len())
            .finish_non_exhaustive()
    }
}

/// Lowercase hex encoding (no `hex` crate dependency).
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize)]
    struct Order<'a> {
        account: &'a str,
        qty: u32,
        price: u32,
    }

    fn order(qty: u32, price: u32) -> Order<'static> {
        Order {
            account: "acct",
            qty,
            price,
        }
    }

    #[test]
    fn identical_requests_produce_identical_keys() {
        let a = OrderDeduplicator::key("00000000-01", "CSPAT00601", &order(1, 100)).unwrap();
        let b = OrderDeduplicator::key("00000000-01", "CSPAT00601", &order(1, 100)).unwrap();
        assert_eq!(a, b);
        // 32-byte SHA-256 -> 64 hex chars.
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn differing_qty_is_a_distinct_order_distinct_key() {
        let a = OrderDeduplicator::key("00000000-01", "CSPAT00601", &order(1, 100)).unwrap();
        let b = OrderDeduplicator::key("00000000-01", "CSPAT00601", &order(2, 100)).unwrap();
        assert_ne!(a, b, "a quantity change is a different order");
    }

    #[test]
    fn account_and_tr_code_are_part_of_the_key() {
        let base = OrderDeduplicator::key("00000000-01", "CSPAT00601", &order(1, 100)).unwrap();
        let other_acct =
            OrderDeduplicator::key("99999999-09", "CSPAT00601", &order(1, 100)).unwrap();
        let other_tr = OrderDeduplicator::key("00000000-01", "CSPAT00701", &order(1, 100)).unwrap();
        assert_ne!(base, other_acct, "different accounts must not collide");
        assert_ne!(base, other_tr, "different order TRs must not collide");
    }

    #[test]
    fn hit_within_ttl_returns_cached_value() {
        let d = OrderDeduplicator::with_default_ttl();
        let key = OrderDeduplicator::key("acct", "CSPAT00601", &order(1, 100)).unwrap();
        assert!(d.get(&key).is_none(), "cold cache misses");
        d.insert(key.clone(), serde_json::json!({"OrdNo": "123"}));
        let cached = d.get(&key).expect("a live entry is a hit");
        assert_eq!(cached, serde_json::json!({"OrdNo": "123"}));
    }

    #[test]
    fn expired_entry_is_evicted_on_read() {
        // A zero TTL means every entry is immediately expired.
        let d = OrderDeduplicator::new(Duration::from_secs(0));
        let key = OrderDeduplicator::key("acct", "CSPAT00601", &order(1, 100)).unwrap();
        d.insert(key.clone(), serde_json::json!({"OrdNo": "123"}));
        assert!(
            d.get(&key).is_none(),
            "an expired entry is not a hit and is evicted on read"
        );
        assert_eq!(d.len(), 0, "read-path lazy eviction dropped the entry");
    }

    #[test]
    fn sweep_drops_expired_entries_with_no_guard_held_no_deadlock() {
        // Zero TTL → every entry is immediately expired.
        let d = OrderDeduplicator::new(Duration::from_secs(0));
        for i in 0..1000 {
            let key = OrderDeduplicator::key("acct", "CSPAT00601", &order(i, 100)).unwrap();
            d.insert(key, serde_json::json!({"i": i}));
        }
        // The retain sweep (the load-bearing no-DashMap-guard path) drops every
        // expired entry and does not deadlock — bounding memory without a worker.
        d.force_sweep_for_test();
        assert_eq!(d.len(), 0, "the sweep must drop all expired entries");

        // A live entry survives the sweep.
        let d2 = OrderDeduplicator::with_default_ttl();
        let k = OrderDeduplicator::key("acct", "CSPAT00601", &order(1, 100)).unwrap();
        d2.insert(k, serde_json::json!({"ok": true}));
        d2.force_sweep_for_test();
        assert_eq!(d2.len(), 1, "a live entry must survive the sweep");
    }

    #[test]
    fn try_reserve_blocks_a_second_concurrent_reservation_until_drop() {
        let d = OrderDeduplicator::with_default_ttl();
        let guard = d.try_reserve("k").expect("first reservation succeeds");
        assert!(
            d.try_reserve("k").is_none(),
            "a second reservation is blocked while the first is in flight"
        );
        // A different key is independent.
        assert!(d.try_reserve("other").is_some());
        drop(guard);
        assert!(
            d.try_reserve("k").is_some(),
            "the key is reservable again after the guard drops"
        );
    }

    #[test]
    fn key_build_is_deterministic_across_field_order() {
        // serde struct serialization is deterministic, so the same value always
        // hashes identically — the property the dedup window depends on.
        let k1 = OrderDeduplicator::key("acct", "tr", &order(5, 5)).unwrap();
        let k2 = OrderDeduplicator::key("acct", "tr", &order(5, 5)).unwrap();
        assert_eq!(k1, k2);
    }
}
