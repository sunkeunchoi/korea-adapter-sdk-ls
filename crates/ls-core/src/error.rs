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

/// Canonical result alias used throughout the crate.
pub type LsResult<T> = Result<T, LsError>;
