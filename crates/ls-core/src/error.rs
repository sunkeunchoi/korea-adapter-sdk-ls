//! Error taxonomy for the LS SDK.
//!
//! `LsError` covers all error variants produced by the runtime:
//! Auth, Http, WebSocket, Decode, ApiError, RateLimited, Config,
//! PaginationLimit, and Parse.
//!
//! `LsResult<T>` is the canonical return type used throughout the crate.

use thiserror::Error;

/// All errors produced by the SDK public API.
#[derive(Debug, Error)]
pub enum LsError {
    /// Authentication problem — empty credentials, failed OAuth2 exchange,
    /// expired revoked token, missing appkey/appsecretkey, etc.
    #[error("auth error: {0}")]
    Auth(String),

    /// Underlying HTTP transport error (timeout, TLS, DNS, connection reset,
    /// non-2xx status without parseable error envelope).
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// WebSocket protocol error.
    #[error("WebSocket error: {0}")]
    WebSocket(String),

    /// JSON deserialization / decode failure.
    #[error("decode error: {0}")]
    Decode(#[from] serde_json::Error),

    /// LS API returned a parseable error envelope with `code` + `message`.
    #[error("API error {code}: {message}")]
    ApiError { code: String, message: String },

    /// An order acknowledgement that can neither be proven Accepted nor safely
    /// classified Rejected. Carries the broker `code`/`message` when one was
    /// present (empty when the failure was transport-level on the order path).
    ///
    /// This is the order-dispatch "fail toward Unknown" signal (order-safety
    /// contract §1/§3): the generic-success code `00000`/empty, or any order
    /// response on a non-2xx HTTP status, lands here so a possibly-filled order
    /// is never blindly resubmitted. The caller routes it to reconciliation
    /// (query `t0425`, match against exchange state) rather than retrying.
    #[error("ambiguous order outcome {code}: {message}")]
    AmbiguousOrder { code: String, message: String },

    /// Rate limiter rejected the request.
    #[error("rate limited")]
    RateLimited,

    /// Invalid configuration supplied to the client constructor.
    #[error("config error: {0}")]
    Config(String),

    /// Pagination limit reached — not all pages were collected.
    #[error("pagination limit reached ({0} pages)")]
    PaginationLimit(usize),

    /// Numeric string parsing failure — empty, malformed, or unsupported format.
    #[error("parse error: {0}")]
    Parse(String),
}

impl LsError {
    /// Returns `true` if this error is the sole paper-incompatible signal —
    /// an `ApiError` carrying the LS Paper "unsupported work" code `01900`
    /// (모의투자에서는 해당업무가 제공되지 않습니다).
    ///
    /// `01900` is preserved verbatim in `ApiError::code` rather than collapsed
    /// into a generic failure, so callers can defer paper-incompatible TRs
    /// specifically. See [`crate::is_paper_incompatible`].
    pub fn is_paper_incompatible(&self) -> bool {
        matches!(self, LsError::ApiError { code, .. } if crate::inner::is_paper_incompatible(code))
    }
}

/// Canonical result alias used throughout the crate.
pub type LsResult<T> = Result<T, LsError>;
