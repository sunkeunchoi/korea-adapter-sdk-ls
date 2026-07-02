//! Adapter configuration → [`ls_core::LsConfig`], with the paper-only interlock.
//!
//! The adapter owns no credentials. Its config carries a [`CredentialSource`] — a
//! lane env-file path, the process environment, or an already-constructed
//! (redacted) [`LsConfig`] for tests — and resolves it to an `LsConfig` at build
//! time, **refusing the production (real-money) environment** (R11). The config
//! type deliberately keeps no raw appkey/secret/account of its own; the only
//! credential-bearing variant, [`CredentialSource::Explicit`], holds an `LsConfig`
//! whose own `Debug` redacts, and [`LsAdapterConfig`]'s hand-written `Debug`
//! (below) never prints credential material — nautilus node internals may log or
//! serialize client configs and this repo does not control those surfaces (U1).

use std::any::Any;
use std::fmt;
use std::path::PathBuf;

use ls_core::{Environment, LsConfig};
use ls_sdk::LsSdk;
use nautilus_common::factories::ClientConfig;

use crate::error::{AdapterError, AdapterResult};

/// Where the adapter sources LS credentials from.
///
/// None of these are stored in [`LsAdapterConfig`] as raw strings the adapter
/// prints; the resolved `LsConfig` is built lazily by [`LsAdapterConfig::build_config`].
#[derive(Clone)]
pub enum CredentialSource {
    /// Resolve from the process environment via [`LsConfig::from_env`] (honours
    /// `LS_TRADING_ENV` + the paper/real interlock). The default for the tester
    /// binaries when a lane file has already been sourced by the shell/Makefile.
    Env,
    /// Load `KEY=VALUE` lines from a lane env-file (e.g. `.env.domestic`) into the
    /// process environment, then resolve via [`LsConfig::from_env`]. Used when the
    /// adapter itself owns lane selection rather than the shell.
    LaneFile(PathBuf),
    /// An already-constructed config (test injection or advanced use). The wrapped
    /// `LsConfig` redacts its own credential fields in `Debug`.
    Explicit(Box<LsConfig>),
}

impl fmt::Debug for CredentialSource {
    /// Redacting: never prints credential material. Env-file *paths* are safe to
    /// show (they name a file, not a secret); the `Explicit` config's own `Debug`
    /// redacts, but we still summarise it as `<explicit>` to avoid depending on
    /// that.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CredentialSource::Env => write!(f, "Env"),
            CredentialSource::LaneFile(p) => write!(f, "LaneFile({})", p.display()),
            CredentialSource::Explicit(_) => write!(f, "Explicit(<redacted LsConfig>)"),
        }
    }
}

/// The adapter's client configuration, shared by the data and execution clients.
///
/// Implements [`ClientConfig`] so the factories (U7) can downcast it out of the
/// `&dyn ClientConfig` the `LiveNode` builder hands them.
#[derive(Clone)]
pub struct LsAdapterConfig {
    /// How to resolve LS credentials.
    pub credentials: CredentialSource,
    /// The nautilus `ClientId` string for both clients (default `LS-KRX`).
    pub client_id: String,
    /// The nautilus `TraderId` string (default `LS-TRADER-001`).
    pub trader_id: String,
    /// Optional override of the WS per-subscription channel capacity (KTD8 raises
    /// this above the SDK default of 64 so both lanes share one manager on
    /// `DropNewest`). `None` keeps the SDK default.
    pub ws_channel_capacity: Option<usize>,
}

impl Default for LsAdapterConfig {
    fn default() -> Self {
        LsAdapterConfig {
            credentials: CredentialSource::Env,
            client_id: "LS-KRX".to_string(),
            trader_id: "LS-TRADER-001".to_string(),
            ws_channel_capacity: Some(1024),
        }
    }
}

impl fmt::Debug for LsAdapterConfig {
    /// Hand-written to guarantee no credential material is ever printed, even as
    /// nautilus internals `Debug`-log the config. Mirrors `LsConfig`'s manual
    /// redacting `Debug`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LsAdapterConfig")
            .field("credentials", &self.credentials)
            .field("client_id", &self.client_id)
            .field("trader_id", &self.trader_id)
            .field("ws_channel_capacity", &self.ws_channel_capacity)
            .finish()
    }
}

impl ClientConfig for LsAdapterConfig {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl LsAdapterConfig {
    /// Build a config sourced from the process environment.
    pub fn from_env() -> Self {
        LsAdapterConfig {
            credentials: CredentialSource::Env,
            ..Default::default()
        }
    }

    /// Build a config sourced from a lane env-file path.
    pub fn from_lane_file(path: impl Into<PathBuf>) -> Self {
        LsAdapterConfig {
            credentials: CredentialSource::LaneFile(path.into()),
            ..Default::default()
        }
    }

    /// Build a config wrapping an already-constructed [`LsConfig`] (tests).
    pub fn explicit(config: LsConfig) -> Self {
        LsAdapterConfig {
            credentials: CredentialSource::Explicit(Box::new(config)),
            ..Default::default()
        }
    }

    /// Resolve the credential source into an [`LsConfig`], applying adapter
    /// overrides and enforcing the **paper-only** interlock (R11).
    ///
    /// # Errors
    ///
    /// - [`AdapterError::ProductionRefused`] if the resolved environment is
    ///   [`Environment::Real`].
    /// - [`AdapterError::Sdk`] if `from_env` fails (missing var, interlock trip).
    /// - [`AdapterError::Config`] if a lane file cannot be read.
    pub fn build_config(&self) -> AdapterResult<LsConfig> {
        let mut config = match &self.credentials {
            CredentialSource::Env => LsConfig::from_env()?,
            CredentialSource::LaneFile(path) => {
                load_env_file(path)?;
                LsConfig::from_env()?
            }
            CredentialSource::Explicit(cfg) => (**cfg).clone(),
        };

        // Paper-only interlock as an ALLOWLIST (R11): refuse anything that is not
        // explicitly Paper. LS routes paper vs real by CREDENTIAL, not domain, so
        // overwriting `environment` cannot neutralize a real-money credential — the
        // refusal is the only real protection, and an allowlist also catches any
        // future non-`Real` real-capable variant (which `is_production()` might miss).
        if !config.environment.is_paper() {
            return Err(AdapterError::ProductionRefused);
        }
        config.environment = Environment::Paper;

        if let Some(cap) = self.ws_channel_capacity {
            config.ws_channel_capacity = Some(cap);
        }
        Ok(config)
    }

    /// Build a validated [`LsSdk`] from this config (synchronous; no network I/O).
    ///
    /// # Errors
    ///
    /// Propagates [`Self::build_config`] failures plus any `LsSdk::new` validation
    /// error.
    pub fn build_sdk(&self) -> AdapterResult<LsSdk> {
        let config = self.build_config()?;
        Ok(LsSdk::new(config)?)
    }
}

/// Load `KEY=VALUE` lines from a lane env-file into the process environment.
///
/// Minimal dotenv semantics: blank lines and `#` comments are skipped, an
/// optional leading `export ` is stripped, and surrounding single/double quotes on
/// the value are removed. Existing process-env vars are **overwritten** so the
/// lane file is authoritative (mirroring the Makefile's `set -a; . .env.<lane>`).
/// This mutates process-global state and is only used by the single-purpose tester
/// binaries (never in-library on a shared runtime).
fn load_env_file(path: &std::path::Path) -> AdapterResult<()> {
    let contents = std::fs::read_to_string(path)
        .map_err(|e| AdapterError::Config(format!("cannot read lane env-file {}: {e}", path.display())))?;
    for raw in contents.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        let value = value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
            .unwrap_or(value);
        if !key.is_empty() {
            // SAFETY: single-threaded tester-binary startup, before any SDK client
            // or async runtime is constructed.
            std::env::set_var(key, value);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal explicit paper config for tests.
    fn paper_config() -> LsConfig {
        LsConfig {
            appkey: "test-appkey".into(),
            appsecretkey: "test-secret".into(),
            account_no: "00000000-01".into(),
            environment: Environment::Paper,
            rate_limits: None,
            base_url: None,
            ws_base_url: None,
            max_pages: None,
            connect_timeout_secs: None,
            request_timeout_secs: None,
            ws_connect_timeout_secs: None,
            allow_insecure_localhost: false,
            ws_channel_capacity: None,
            ws_overflow_policy: None,
        }
    }

    #[test]
    fn explicit_paper_config_resolves() {
        let cfg = LsAdapterConfig::explicit(paper_config());
        let resolved = cfg.build_config().expect("paper config resolves");
        assert!(resolved.environment.is_paper());
        // The adapter's raised WS capacity default is applied.
        assert_eq!(resolved.ws_channel_capacity, Some(1024));
    }

    #[test]
    fn production_environment_is_refused() {
        let mut real = paper_config();
        real.environment = Environment::Real;
        let cfg = LsAdapterConfig::explicit(real);
        let err = cfg.build_config().expect_err("production must be refused");
        // The error names the paper-only constraint.
        let msg = err.to_string();
        assert!(matches!(err, AdapterError::ProductionRefused), "got {msg}");
        assert!(msg.contains("paper-only"), "message should name paper-only: {msg}");
    }

    #[test]
    fn debug_output_contains_no_credential_material() {
        let cfg = LsAdapterConfig::explicit(paper_config());
        let dbg = format!("{cfg:?}");
        assert!(!dbg.contains("test-appkey"), "appkey leaked: {dbg}");
        assert!(!dbg.contains("test-secret"), "secret leaked: {dbg}");
        assert!(!dbg.contains("00000000-01"), "account leaked: {dbg}");
        assert!(dbg.contains("redacted"), "should mark the config redacted: {dbg}");
    }

    #[test]
    fn build_sdk_succeeds_on_paper() {
        let cfg = LsAdapterConfig::explicit(paper_config());
        cfg.build_sdk().expect("sdk builds from a valid paper config");
    }
}
