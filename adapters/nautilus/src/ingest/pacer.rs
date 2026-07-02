//! Adapter-side per-TR pacer, above the SDK's category bucket (KTD4).
//!
//! The SDK enforces only category buckets; `EndpointPolicy.rate_limit_per_sec`
//! (t8410/t8412 = 1/s) is metadata the client does **not** enforce, and violating
//! it yields `IGW00201`. This pacer meters each TR independently, pacing to the
//! **stricter** of the per-TR cap and the category cap. It uses `tokio::time` so
//! tests can drive it under a paused clock (`#[tokio::test(start_paused = true)]`).

use std::time::Duration;

use ls_core::endpoint_policy::EndpointPolicy;
use tokio::sync::Mutex;
use tokio::time::Instant;

/// The default MarketData category cap the SDK enforces (5/s). The pacer never
/// paces *looser* than the per-TR cap, so this only tightens a TR whose per-TR cap
/// is absent or higher than the category.
pub const MARKET_DATA_CATEGORY_PER_SEC: u32 = 5;

/// A single-TR minimum-interval pacer. `acquire().await` returns immediately if the
/// TR is due, else sleeps until the next slot. Cloneable state is behind a `Mutex`
/// so one pacer can gate many concurrent chunk fetches for the same TR.
#[derive(Debug)]
pub struct Pacer {
    min_interval: Duration,
    next_allowed: Mutex<Option<Instant>>,
}

impl Pacer {
    /// Build a pacer at `rate` requests/second (clamped to ≥1).
    pub fn per_sec(rate: u32) -> Self {
        let rate = rate.max(1);
        Pacer {
            min_interval: Duration::from_secs_f64(1.0 / rate as f64),
            next_allowed: Mutex::new(None),
        }
    }

    /// Build a pacer for a TR, pacing to the **stricter** of the TR's
    /// `rate_limit_per_sec` (from its [`EndpointPolicy`]) and the category cap.
    /// A policy with no per-TR cap paces at the category cap alone (KTD4).
    pub fn for_policy(policy: &EndpointPolicy, category_per_sec: u32) -> Self {
        let per_tr = policy.rate_limit_per_sec.unwrap_or(category_per_sec);
        Pacer::per_sec(per_tr.min(category_per_sec))
    }

    /// The effective minimum interval between calls.
    pub fn min_interval(&self) -> Duration {
        self.min_interval
    }

    /// Wait until this TR is next allowed to fire, then reserve the slot.
    pub async fn acquire(&self) {
        let mut next = self.next_allowed.lock().await;
        let now = Instant::now();
        match *next {
            Some(t) if t > now => {
                tokio::time::sleep_until(t).await;
                *next = Some(t + self.min_interval);
            }
            _ => {
                *next = Some(now + self.min_interval);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ls_core::endpoint_policy::{T8410_POLICY, T8412_POLICY};

    #[test]
    fn policy_pacer_uses_stricter_of_tr_and_category() {
        // t8412 per-TR cap is 1/s; category is 5/s → stricter = 1/s → 1000ms.
        let pacer = Pacer::for_policy(&T8412_POLICY, MARKET_DATA_CATEGORY_PER_SEC);
        assert_eq!(pacer.min_interval(), Duration::from_millis(1000));
        let daily = Pacer::for_policy(&T8410_POLICY, MARKET_DATA_CATEGORY_PER_SEC);
        assert_eq!(daily.min_interval(), Duration::from_millis(1000));
    }

    #[tokio::test(start_paused = true)]
    async fn holds_t8412_to_one_per_second_under_a_burst() {
        let pacer = Pacer::for_policy(&T8412_POLICY, MARKET_DATA_CATEGORY_PER_SEC);
        let start = Instant::now();
        // Five queued chunk fetches. First is immediate; each subsequent waits 1s.
        for _ in 0..5 {
            pacer.acquire().await;
        }
        let elapsed = start.elapsed();
        // 5 acquisitions at 1/s ⇒ ≥ 4 seconds of pacing under the paused clock.
        assert!(
            elapsed >= Duration::from_secs(4),
            "expected ≥4s of pacing, got {elapsed:?}"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn faster_rate_paces_proportionally() {
        let pacer = Pacer::per_sec(5); // 200ms spacing
        let start = Instant::now();
        for _ in 0..3 {
            pacer.acquire().await;
        }
        assert!(start.elapsed() >= Duration::from_millis(400));
    }
}
