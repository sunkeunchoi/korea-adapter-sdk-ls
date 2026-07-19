//! The per-consumer adoption seam value (KTD8).
//!
//! [`CalendarAdoption`] lives in the calendar leaf crate so EVERY consumer
//! (`nautilus_ls`, `lab`, and the Phase C migrations U9–U13) shares one type.
//! It carries no behavior beyond naming which posture a composition root injected:
//!
//! - [`Legacy`](CalendarAdoption::Legacy) — the weekday path is unchanged; the
//!   calendar is not consulted.
//! - [`Shadow`](CalendarAdoption::Shadow) — the calendar decision is computed and
//!   recorded to a non-persisted diagnostic channel, but the legacy action stays
//!   authoritative. This is the composed DEFAULT in this slice.
//! - [`Enforced`](CalendarAdoption::Enforced) — the calendar decides with NO
//!   weekday fallback; a load failure fails closed (the consumer decides).
//!
//! The core never reads env to pick a state — a composition root injects it (KTD5).

use serde::{Deserialize, Serialize};

/// Which adoption posture a consumer runs the calendar under (KTD8). The composed
/// default in this slice is [`Shadow`](CalendarAdoption::Shadow).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalendarAdoption {
    /// Weekday path unchanged; the calendar is not consulted.
    Legacy,
    /// Compute + record the calendar decision; the legacy action stays authoritative.
    Shadow,
    /// The calendar decides with no weekday fallback; load failure fails closed.
    Enforced,
}

impl Default for CalendarAdoption {
    /// The composed default in this offline slice (KTD8): [`Shadow`](CalendarAdoption::Shadow).
    fn default() -> Self {
        CalendarAdoption::Shadow
    }
}

impl CalendarAdoption {
    /// The stable lower-case token (`"legacy"` / `"shadow"` / `"enforced"`) used in
    /// diagnostics and env resolution.
    pub fn as_str(self) -> &'static str {
        match self {
            CalendarAdoption::Legacy => "legacy",
            CalendarAdoption::Shadow => "shadow",
            CalendarAdoption::Enforced => "enforced",
        }
    }

    /// Parse a case-insensitive token back into an adoption state; `None` on junk (a
    /// composition root falls back to [`Default`] on `None`).
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "legacy" => Some(CalendarAdoption::Legacy),
            "shadow" => Some(CalendarAdoption::Shadow),
            "enforced" => Some(CalendarAdoption::Enforced),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_shadow() {
        assert_eq!(CalendarAdoption::default(), CalendarAdoption::Shadow);
    }

    #[test]
    fn round_trips_through_str_and_parse() {
        for state in [
            CalendarAdoption::Legacy,
            CalendarAdoption::Shadow,
            CalendarAdoption::Enforced,
        ] {
            assert_eq!(CalendarAdoption::parse(state.as_str()), Some(state));
        }
        assert_eq!(CalendarAdoption::parse(" ENFORCED "), Some(CalendarAdoption::Enforced));
        assert_eq!(CalendarAdoption::parse("nonsense"), None);
    }
}
