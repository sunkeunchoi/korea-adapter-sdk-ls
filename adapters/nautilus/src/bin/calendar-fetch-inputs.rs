//! `calendar-fetch-inputs` — the maintainer bulk-fetch CLI (U4, KTD4/KTD9).
//!
//! `calendar-fetch-inputs --window <from..through> [--krx-through <YYYY-MM-DD>] \
//!     --inputs-out <path> --state <checkpoint> [--state-root <dir>] [--pace-ms <n>]`
//!
//! Fetches KRX daily-market witnesses (per weekday through the witness horizon) and KASI
//! holiday facts (per year), generates the weekend + fixed-closure rule evidence locally, and
//! writes the owner-local normalized-inputs artifact. Resumable: an interrupted or quota-bounded
//! run continues from the 0o600 checkpoint instead of restarting.
//!
//! Credentials come SOLELY from the gitignored maintainer env (`LS_KRX_APPKEY` /
//! `LS_KASI_SERVICE_KEY`) — never arguments — and are stripped from every persisted reason. All
//! output paths are confined beneath the owner-local state root (the publication boundary is
//! tool-enforced). This binary is the ONLY place a real HTTP client is constructed; the fetch
//! transport is hardened by construction (timeouts, redirects disabled, HTTPS-only) and injected
//! behind the [`fetch_inputs`] seam.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use chrono::NaiveDate;
use nautilus_ls::calendar_refresh::{
    atomic_write_owner_only, confine, fetch_inputs, DateRange, FetchConfig, MaintainerCredentials,
};
use nautilus_ls::scrub;

/// The env var naming the owner-local state root all outputs are confined beneath.
const STATE_ROOT_ENV: &str = "LS_CALENDAR_STATE_ROOT";
/// The default state root (relative to CWD) when `--state-root` / the env var are absent.
const DEFAULT_STATE_ROOT: &str = "state";
/// The default inter-call pacing (ms) — a conservative bulk-fetch cadence.
const DEFAULT_PACE_MS: u64 = 250;
/// A hard response-size ceiling: KRX/KASI responses are small; anything larger is rejected.
const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

fn main() -> ExitCode {
    scrub::install();
    // Emit-before-fallible-parse (KTD4): a startup marker precedes the fallible arg parse so a
    // parse/credential failure is still diagnosable on stderr.
    eprintln!("calendar-fetch-inputs: starting");
    match run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {}", scrub::scrub_secrets(&e.to_string()));
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<ExitCode, Box<dyn std::error::Error>> {
    let args = Args::parse(std::env::args().skip(1))?;

    // Confine every output beneath the owner-local state root BEFORE any network call — the
    // publication boundary is tool-enforced, not discipline-enforced.
    std::fs::create_dir_all(&args.state_root)?;
    let inputs_out = confine(&args.state_root, &args.inputs_out)?;
    let state_path = confine(&args.state_root, &args.state)?;

    // Credentials from the gitignored maintainer env only — refuse before any network if absent.
    let creds = MaintainerCredentials::from_env();
    if creds.krx_appkey.is_none() || creds.kasi_service_key.is_none() {
        eprintln!(
            "refused: both LS_KRX_APPKEY and LS_KASI_SERVICE_KEY must be set (no credential ever rides in an argument)"
        );
        return Ok(ExitCode::FAILURE);
    }

    let cfg = FetchConfig {
        window: args.window,
        krx_through: args.krx_through,
        pace: Duration::from_millis(args.pace_ms),
    };

    // The ONLY real HTTP client in the process, hardened by construction (KTD9): connect/read
    // timeouts, redirects disabled (both credentials ride as query params and must not forward),
    // HTTPS-only. Injected behind the fetch seam so every test runs offline.
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .https_only(true)
        .build()?;
    let fetch = |url: &str| -> Result<String, String> {
        let resp = client.get(url).send().map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        let bytes = resp.bytes().map_err(|e| e.to_string())?;
        if bytes.len() > MAX_RESPONSE_BYTES {
            return Err(format!("response exceeds the {MAX_RESPONSE_BYTES}-byte ceiling"));
        }
        String::from_utf8(bytes.to_vec()).map_err(|e| e.to_string())
    };

    let inputs = fetch_inputs(&cfg, &creds, &state_path, fetch, |d| std::thread::sleep(d))?;

    // Write the owner-local inputs artifact (0o600 atomic). No credential, no raw body, no
    // KRX/KASI row beyond normalized evidence ever reaches disk here.
    let bytes = serde_json::to_vec_pretty(&inputs)?;
    atomic_write_owner_only(&inputs_out, &bytes)?;

    // Summary to the non-persisted diagnostic channel — counts + per-source outcome only.
    println!("inputs written: {}", inputs_out.display());
    println!("evidence records: {}", inputs.evidence.len());
    for outcome in &inputs.outcomes {
        let covered = outcome
            .covered()
            .map(|ranges| {
                ranges
                    .iter()
                    .map(|r| format!("{}..{}", r.from, r.through))
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .unwrap_or_else(|| "<none>".to_string());
        println!(
            "source {} ok={} covered=[{}]{}",
            outcome.source_id,
            outcome.is_ok(),
            covered,
            outcome
                .failure_reason()
                .map(|r| format!(" failed={r}"))
                .unwrap_or_default()
        );
    }
    let any_failed = inputs.outcomes.iter().any(|o| !o.is_ok());
    if any_failed {
        eprintln!("note: at least one source is partial — re-run to resume from the checkpoint");
    }
    Ok(ExitCode::SUCCESS)
}

/// Parsed CLI arguments.
#[derive(Debug)]
struct Args {
    window: DateRange,
    krx_through: NaiveDate,
    inputs_out: PathBuf,
    state: PathBuf,
    state_root: PathBuf,
    pace_ms: u64,
}

impl Args {
    fn parse(mut args: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut window: Option<DateRange> = None;
        let mut krx_through: Option<NaiveDate> = None;
        let mut inputs_out: Option<PathBuf> = None;
        let mut state: Option<PathBuf> = None;
        let mut state_root: Option<PathBuf> = None;
        let mut pace_ms: Option<u64> = None;

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--window" => {
                    let raw = args.next().ok_or("--window needs <from..through>")?;
                    window = Some(parse_window(&raw)?);
                }
                "--krx-through" => {
                    let raw = args.next().ok_or("--krx-through needs a date")?;
                    krx_through = Some(parse_date(&raw)?);
                }
                "--inputs-out" => {
                    inputs_out = Some(PathBuf::from(args.next().ok_or("--inputs-out needs a path")?));
                }
                "--state" => {
                    state = Some(PathBuf::from(args.next().ok_or("--state needs a path")?));
                }
                "--state-root" => {
                    state_root = Some(PathBuf::from(args.next().ok_or("--state-root needs a path")?));
                }
                "--pace-ms" => {
                    let raw = args.next().ok_or("--pace-ms needs a number")?;
                    pace_ms = Some(raw.trim().parse().map_err(|e| format!("bad --pace-ms {raw:?}: {e}"))?);
                }
                other => {
                    return Err(format!(
                        "unknown argument {other:?} (want --window / --krx-through / --inputs-out / --state / --state-root / --pace-ms)"
                    ))
                }
            }
        }

        let window = window.ok_or("missing required --window <from..through>")?;
        Ok(Args {
            window,
            // Default the KRX witness horizon to the window end (a bounded probe run); a genesis
            // fetch passes the real last-closed-session via --krx-through.
            krx_through: krx_through.unwrap_or(window.through),
            inputs_out: inputs_out.ok_or("missing required --inputs-out <path>")?,
            state: state.ok_or("missing required --state <path>")?,
            state_root: state_root
                .or_else(|| std::env::var(STATE_ROOT_ENV).ok().map(PathBuf::from))
                .unwrap_or_else(|| PathBuf::from(DEFAULT_STATE_ROOT)),
            pace_ms: pace_ms.unwrap_or(DEFAULT_PACE_MS),
        })
    }
}

fn parse_date(raw: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(raw.trim(), "%Y-%m-%d").map_err(|e| format!("bad date {raw:?}: {e}"))
}

fn parse_window(raw: &str) -> Result<DateRange, String> {
    let (from, through) = raw
        .split_once("..")
        .ok_or_else(|| format!("--window must be <from..through>, got {raw:?}"))?;
    let from = parse_date(from)?;
    let through = parse_date(through)?;
    if from > through {
        return Err(format!("--window from {from} is after through {through}"));
    }
    Ok(DateRange::new(from, through))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn parses_required_args_with_defaults() {
        let args = Args::parse(
            [
                "--window", "2010-01-04..2026-09-06", "--inputs-out", "state/inputs.json",
                "--state", "state/fetch.ckpt",
            ]
            .into_iter()
            .map(String::from),
        )
        .expect("parses");
        assert_eq!(args.window, DateRange::new(d(2010, 1, 4), d(2026, 9, 6)));
        assert_eq!(args.krx_through, d(2026, 9, 6), "krx-through defaults to the window end");
        assert_eq!(args.pace_ms, DEFAULT_PACE_MS);
    }

    #[test]
    fn krx_through_and_pace_override() {
        let args = Args::parse(
            [
                "--window", "2010-01-04..2026-09-06", "--krx-through", "2026-07-22",
                "--inputs-out", "state/i.json", "--state", "state/s.ckpt", "--pace-ms", "50",
            ]
            .into_iter()
            .map(String::from),
        )
        .expect("parses");
        assert_eq!(args.krx_through, d(2026, 7, 22));
        assert_eq!(args.pace_ms, 50);
    }

    #[test]
    fn a_reversed_window_is_an_error_not_a_panic() {
        let err = Args::parse(
            ["--window", "2026-09-06..2010-01-04", "--inputs-out", "i", "--state", "s"]
                .into_iter()
                .map(String::from),
        )
        .unwrap_err();
        assert!(err.contains("after"), "{err}");
    }

    #[test]
    fn missing_window_is_an_error() {
        let err = Args::parse(
            ["--inputs-out", "i", "--state", "s"].into_iter().map(String::from),
        )
        .unwrap_err();
        assert!(err.contains("--window"), "{err}");
    }
}
