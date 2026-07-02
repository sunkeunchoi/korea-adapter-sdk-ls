//! Adapter error type.
//!
//! The adapter surfaces one error enum at its boundary. Transport/business
//! failures from the SDK are carried verbatim as [`AdapterError::Sdk`] so the
//! variant-keyed order-event mapping (KTD6) can classify on the underlying
//! [`ls_core::LsError`] variant; adapter-specific failures (unsupported instrument
//! domain, production-environment refusal, rule-data gaps) are their own variants.

use thiserror::Error;

/// The adapter's boundary error.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AdapterError {
    /// An underlying SDK transport/business error, carried verbatim so callers
    /// can match on the [`ls_core::LsError`] variant (the KTD6 mapping seam).
    #[error("ls-sdk error: {0}")]
    Sdk(#[from] ls_core::LsError),

    /// A non-domestic-equity instrument domain was requested in v1 (R2/AE3).
    /// The message names the requested domain so the caller sees an explicit
    /// unsupported-domain error rather than a silent wrong mapping.
    #[error("unsupported instrument domain in v1: {domain} (only domestic KRX equities are supported)")]
    UnsupportedDomain {
        /// The requested, unsupported domain (e.g. `overseas_stock`, `domestic_fo`).
        domain: String,
    },

    /// The adapter was configured for the production (real-money) environment.
    /// v1 is paper-only (R11); the adapter refuses to start (never silently
    /// downgrades or proceeds against real money).
    #[error("production (real-money) environment is not supported: this adapter is paper-only in v1 (set LS_TRADING_ENV=paper)")]
    ProductionRefused,

    /// A required field on a wire row failed to parse. Names the field so the
    /// failure is a diagnosable error, not a panic (U2 malformed-numeric test).
    #[error("failed to parse `{field}` from value {value:?}: {reason}")]
    FieldParse {
        /// The field name that failed.
        field: String,
        /// The raw value that failed to parse.
        value: String,
        /// Why it failed.
        reason: String,
    },

    /// KRX rule data could not resolve a tick band for the given price.
    #[error("no KRX tick band covers price {price} (market {market}, regime {regime})")]
    NoTickBand {
        /// The price with no covering band.
        price: i64,
        /// The market segment (`KOSPI`/`KOSDAQ`).
        market: String,
        /// The rule-data regime (`pre_2023`/`post_2023`).
        regime: String,
    },

    /// A config or credential-resolution failure specific to the adapter.
    #[error("adapter config error: {0}")]
    Config(String),

    /// An ingestion/catalog I/O failure.
    #[error("ingest error: {0}")]
    Ingest(String),

    /// A wrapped `anyhow` error from a nautilus API boundary.
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Convenience alias for adapter results.
pub type AdapterResult<T> = Result<T, AdapterError>;
