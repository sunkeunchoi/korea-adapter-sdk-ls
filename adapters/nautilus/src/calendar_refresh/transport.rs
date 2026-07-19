//! The maintainer-run live transport (U14, KTD9) — a SEPARATE impl of the input port.
//!
//! [`LiveEvidencePort`] is the only place that touches the network. It is NEVER exercised
//! by the offline gate (the gate uses [`StaticEvidencePort`](super::port::StaticEvidencePort)
//! with synthetic inputs); the tests here only prove the CREDENTIAL boundary with an
//! injected failing `fetch`. Credentials come solely from a named gitignored maintainer env
//! file / process env ([`MaintainerCredentials::from_env`]) — never hardcoded — and are
//! STRIPPED from every URL ([`strip_url_credentials`]) before it can reach an error, log, or
//! diagnostic. Raw KRX/KASI bodies are normalized to evidence and never persisted or
//! returned.

use std::fmt;

use chrono::NaiveDate;
use serde::Deserialize;

use nautilus_ls_calendar::schema::{EvidenceKind, EvidenceRecord, Source, SourceKind};
use nautilus_ls_calendar::witness::{
    default_witness_id, witness_from_response, KrxDailyMarketResponse, KrxDailyRow, WitnessOutcome,
};

use super::port::{EvidenceInputPort, RefreshInputs, RefreshScope, SourceOutcome};

/// Env var naming the KASI holiday-service key (maintainer-local, gitignored).
pub const KASI_SERVICE_KEY_ENV: &str = "LS_KASI_SERVICE_KEY";
/// Env var naming the KRX daily-market appkey (maintainer-local, gitignored).
pub const KRX_APPKEY_ENV: &str = "LS_KRX_APPKEY";

const KASI_SOURCE_ID: &str = "kasi";
const KRX_RULE_SOURCE_ID: &str = "krx-rule";
const KRX_DAILY_SOURCE_ID: &str = "krx-daily";

/// Maintainer credentials for the live transport. Resolved from the process env / a named
/// gitignored maintainer env file — NEVER hardcoded. `Debug` is hand-written to redact.
#[derive(Clone, Default)]
pub struct MaintainerCredentials {
    /// The KASI holiday-service key, if configured.
    pub kasi_service_key: Option<String>,
    /// The KRX daily-market appkey, if configured.
    pub krx_appkey: Option<String>,
}

impl MaintainerCredentials {
    /// Resolve credentials from the process environment (empty values treated as absent).
    pub fn from_env() -> Self {
        let read = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        MaintainerCredentials {
            kasi_service_key: read(KASI_SERVICE_KEY_ENV),
            krx_appkey: read(KRX_APPKEY_ENV),
        }
    }

    /// The configured secret values (for defense-in-depth masking of any surface).
    fn secrets(&self) -> Vec<&str> {
        [self.kasi_service_key.as_deref(), self.krx_appkey.as_deref()]
            .into_iter()
            .flatten()
            .collect()
    }
}

impl fmt::Debug for MaintainerCredentials {
    /// Never prints credential material.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MaintainerCredentials")
            .field("kasi_service_key", &self.kasi_service_key.as_ref().map(|_| "<redacted>"))
            .field("krx_appkey", &self.krx_appkey.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

/// Mask credential query-string parameters out of a URL, leaving structure + non-credential
/// params intact. A parameter whose key contains a credential hint (case-insensitive:
/// `serviceKey`, `appkey`, `secret`, `token`, `key`, …) has its value replaced with `***`.
///
/// `scrub.rs`'s panic-only hook and its token heuristic do NOT cover a URL-encoded
/// credential param, so this is the transport's own explicit responsibility.
pub fn strip_url_credentials(url: &str) -> String {
    let Some((base, query)) = url.split_once('?') else {
        return url.to_string();
    };
    let redacted: Vec<String> = query
        .split('&')
        .map(|pair| match pair.split_once('=') {
            Some((k, _v)) if is_credential_param(k) => format!("{k}=***"),
            _ => pair.to_string(),
        })
        .collect();
    format!("{base}?{}", redacted.join("&"))
}

fn is_credential_param(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    const HINTS: &[&str] = &[
        "servicekey", "appkey", "appsecret", "secretkey", "apikey", "authkey", "token",
        "secret", "password", "pwd", "key",
    ];
    HINTS.iter().any(|h| key.contains(h))
}

/// The live transport input port — the SEPARATE maintainer impl. Generic over a `fetch`
/// function `Fn(&str) -> Result<String, String>` so the real HTTP client is injected at the
/// composition root and tests can inject a deterministic (failing) fetch. Never persists a
/// raw body; a fetch error is credential-stripped before it reaches any [`SourceOutcome`].
pub struct LiveEvidencePort<F> {
    creds: MaintainerCredentials,
    fetch: F,
}

impl<F> fmt::Debug for LiveEvidencePort<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LiveEvidencePort")
            .field("creds", &self.creds)
            .field("fetch", &"<fn>")
            .finish()
    }
}

impl<F> LiveEvidencePort<F>
where
    F: Fn(&str) -> Result<String, String>,
{
    /// Build a live port with resolved `creds` and an injected `fetch`.
    pub fn new(creds: MaintainerCredentials, fetch: F) -> Self {
        LiveEvidencePort { creds, fetch }
    }

    /// Perform one request, returning the body on success or a CREDENTIAL-SAFE
    /// [`SourceOutcome`] on failure (the raw URL — including credentials — never reaches the
    /// reason).
    fn request(&self, source_id: &str, kind: SourceKind, url: &str) -> Result<String, SourceOutcome> {
        match (self.fetch)(url) {
            Ok(body) => Ok(body),
            Err(message) => {
                // Strip the credential params from any URL echoed in the message, then mask
                // any standalone secret value as defense-in-depth.
                let mut safe = message.replace(url, &strip_url_credentials(url));
                for secret in self.creds.secrets() {
                    safe = safe.replace(secret, "***");
                }
                Err(SourceOutcome::failed(source_id, kind, safe))
            }
        }
    }

    fn kasi_url(&self, scope: &RefreshScope, key: &str) -> String {
        format!(
            "https://apis.data.go.kr/B090041/openapi/service/SpcdeInfoService/getRestDeInfo?serviceKey={key}&solYear={}&_type=json",
            scope.from.format("%Y")
        )
    }

    fn krx_url(&self, scope: &RefreshScope, appkey: &str) -> String {
        format!(
            "https://data.krx.example/api/stk_bydd_trd?appkey={appkey}&basDd={}",
            scope.through.format("%Y%m%d")
        )
    }
}

impl<F> EvidenceInputPort for LiveEvidencePort<F>
where
    F: Fn(&str) -> Result<String, String>,
{
    fn gather(&self, scope: &RefreshScope) -> RefreshInputs {
        let mut sources: Vec<Source> = Vec::new();
        let mut evidence: Vec<EvidenceRecord> = Vec::new();
        let mut outcomes: Vec<SourceOutcome> = Vec::new();

        // KASI holiday facts (+ the connecting KRX rule).
        if let Some(key) = self.creds.kasi_service_key.as_deref() {
            let url = self.kasi_url(scope, key);
            match self.request(KASI_SOURCE_ID, SourceKind::KasiHoliday, &url) {
                Ok(body) => match parse_holidays(&body) {
                    Ok(dates) => {
                        sources.push(source(KASI_SOURCE_ID, SourceKind::KasiHoliday));
                        sources.push(source(KRX_RULE_SOURCE_ID, SourceKind::KrxRule));
                        for date in dates {
                            evidence.push(fact(
                                format!("kasi-{date}"),
                                KASI_SOURCE_ID,
                                date,
                                EvidenceKind::HolidayFact,
                            ));
                            evidence.push(fact(
                                format!("rule-{date}"),
                                KRX_RULE_SOURCE_ID,
                                date,
                                EvidenceKind::DeterministicRule,
                            ));
                        }
                        outcomes.push(SourceOutcome::ok(KASI_SOURCE_ID, SourceKind::KasiHoliday));
                    }
                    Err(message) => outcomes.push(SourceOutcome::failed(
                        KASI_SOURCE_ID,
                        SourceKind::KasiHoliday,
                        message,
                    )),
                },
                Err(outcome) => outcomes.push(outcome),
            }
        }

        // KRX daily-market positive witness for the scope's most recent date.
        if let Some(appkey) = self.creds.krx_appkey.as_deref() {
            let url = self.krx_url(scope, appkey);
            match self.request(KRX_DAILY_SOURCE_ID, SourceKind::KrxDailyMarket, &url) {
                Ok(body) => match parse_krx(&body) {
                    Ok(resp) => {
                        sources.push(source(KRX_DAILY_SOURCE_ID, SourceKind::KrxDailyMarket));
                        if let WitnessOutcome::Witness(mut w) = witness_from_response(&resp) {
                            w.id = default_witness_id(resp.requested_date);
                            w.source_id = KRX_DAILY_SOURCE_ID.to_string();
                            evidence.push(w);
                        }
                        outcomes.push(SourceOutcome::ok(
                            KRX_DAILY_SOURCE_ID,
                            SourceKind::KrxDailyMarket,
                        ));
                    }
                    Err(message) => outcomes.push(SourceOutcome::failed(
                        KRX_DAILY_SOURCE_ID,
                        SourceKind::KrxDailyMarket,
                        message,
                    )),
                },
                Err(outcome) => outcomes.push(outcome),
            }
        }

        RefreshInputs {
            sources,
            evidence,
            outcomes,
        }
    }
}

fn source(id: &str, kind: SourceKind) -> Source {
    Source {
        id: id.to_string(),
        kind,
        label: id.to_string(),
        synthetic: false,
    }
}

fn fact(id: String, source_id: &str, date: NaiveDate, kind: EvidenceKind) -> EvidenceRecord {
    EvidenceRecord {
        id,
        source_id: source_id.to_string(),
        date,
        kind,
        valid: true,
        superseded_by: None,
        citation: None,
        recorded_at: chrono::Utc::now(),
    }
}

/// A normalized-transport DTO for KASI holidays (the maintainer's real fetch adapts KASI's
/// native response to this credential-free, raw-body-free shape).
#[derive(Debug, Deserialize)]
struct HolidaysDto {
    holidays: Vec<NaiveDate>,
}

fn parse_holidays(body: &str) -> Result<Vec<NaiveDate>, String> {
    serde_json::from_str::<HolidaysDto>(body)
        .map(|d| d.holidays)
        .map_err(|e| format!("KASI response could not be normalized: {e}"))
}

/// A normalized-transport DTO for a KRX daily-market response.
#[derive(Debug, Deserialize)]
struct KrxDto {
    success: bool,
    requested_date: NaiveDate,
    rows: Vec<KrxRowDto>,
    #[serde(default)]
    error_code: Option<String>,
}

#[derive(Debug, Deserialize)]
struct KrxRowDto {
    date: NaiveDate,
    market: String,
}

fn parse_krx(body: &str) -> Result<KrxDailyMarketResponse, String> {
    let dto: KrxDto = serde_json::from_str(body)
        .map_err(|e| format!("KRX response could not be normalized: {e}"))?;
    Ok(KrxDailyMarketResponse {
        success: dto.success,
        requested_date: dto.requested_date,
        rows: dto
            .rows
            .into_iter()
            .map(|r| KrxDailyRow {
                date: r.date,
                market: r.market,
            })
            .collect(),
        error_code: dto.error_code,
    })
}
