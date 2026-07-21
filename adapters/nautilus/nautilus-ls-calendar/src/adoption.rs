//! The per-consumer adoption seam value (KTD8).
//!
//! [`CalendarAdoption`] lives in the calendar leaf crate so EVERY consumer
//! (`nautilus_ls`, `lab`) shares one type. Once the KRX weekday era was retired
//! (issue #189, U6–U10) the only surviving posture is
//! [`Enforced`](CalendarAdoption::Enforced): the calendar decides with NO weekday
//! fallback and a load failure fails closed (the consumer decides). The former
//! `Legacy`/`Shadow` postures — and the weekday primitives they guarded — are gone.
//!
//! The type is retained (rather than deleted outright) because it still names the
//! posture a composition root injected and threads through the redacted startup
//! record; the core never reads env to pick a state — a composition root injects it
//! (KTD5).

use serde::{Deserialize, Serialize};

/// Which adoption posture a consumer runs the calendar under (KTD8). After the #189
/// weekday retirement the sole posture is [`Enforced`](CalendarAdoption::Enforced).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalendarAdoption {
    /// The calendar decides with no weekday fallback; load failure fails closed.
    Enforced,
}

impl Default for CalendarAdoption {
    /// The only posture after the #189 weekday retirement: [`Enforced`](CalendarAdoption::Enforced).
    fn default() -> Self {
        CalendarAdoption::Enforced
    }
}

impl CalendarAdoption {
    /// The stable lower-case token (`"enforced"`) used in diagnostics and env resolution.
    pub fn as_str(self) -> &'static str {
        match self {
            CalendarAdoption::Enforced => "enforced",
        }
    }

    /// Parse a case-insensitive token into an adoption state; `None` on junk (a
    /// composition root falls back to [`Default`] on `None`). The retired `legacy`/
    /// `shadow` tokens no longer parse.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "enforced" => Some(CalendarAdoption::Enforced),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_enforced() {
        assert_eq!(CalendarAdoption::default(), CalendarAdoption::Enforced);
    }

    #[test]
    fn round_trips_through_str_and_parse() {
        assert_eq!(
            CalendarAdoption::parse(CalendarAdoption::Enforced.as_str()),
            Some(CalendarAdoption::Enforced)
        );
        assert_eq!(CalendarAdoption::parse(" ENFORCED "), Some(CalendarAdoption::Enforced));
        assert_eq!(CalendarAdoption::parse("nonsense"), None);
        // The retired postures no longer parse.
        assert_eq!(CalendarAdoption::parse("legacy"), None);
        assert_eq!(CalendarAdoption::parse("shadow"), None);
    }
}
