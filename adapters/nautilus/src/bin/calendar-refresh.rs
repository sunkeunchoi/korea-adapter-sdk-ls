//! `calendar-refresh` — the maintainer refresh CLI (U14, KTD9).
//!
//! `calendar-refresh --active <path> --as-of <RFC3339> --mode incremental|full \
//!     --through <YYYY-MM-DD> [--inputs <normalized-inputs.json>]`
//!
//! Loads the active predecessor snapshot, gathers NORMALIZED evidence through the input
//! port, recomputes a CANDIDATE, diffs it against the exact active predecessor, and writes
//! the candidate + diff to SEPARATE paths — the active file is never touched (activation is
//! the separate `calendar-activate` tool, U15).
//!
//! Two evidence ports:
//! - `--inputs <file>`: a reviewed, credential-free normalized-inputs JSON ([`StaticEvidencePort`]).
//!   The offline / reproducible path.
//! - default: the live [`LiveEvidencePort`] with credentials from the process env
//!   (`LS_KASI_SERVICE_KEY` / `LS_KRX_APPKEY`) — maintainer-run, networked, out of the gate.
//!   This binary wires no HTTP client, so the live port requires an operator-supplied fetch;
//!   absent one, run with `--inputs`.
//!
//! Credentials never appear in arguments, logs, diffs, or the candidate: they enter only via
//! the gitignored maintainer env and are stripped from every URL before any surface.

use std::path::PathBuf;
use std::process::ExitCode;

use chrono::{DateTime, NaiveDate, Utc};
use nautilus_ls::calendar_refresh::{
    default_operating_horizon, refresh, write_candidate, RefreshMode, RefreshScope,
    StaticEvidencePort,
};
use nautilus_ls::scrub;
use nautilus_ls_calendar::schema::Snapshot;
use nautilus_ls_calendar::KrxCalendar;

fn main() -> ExitCode {
    scrub::install();
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

    // Load + validate the active predecessor at the as-of instant (typed failures propagate).
    let active = KrxCalendar::load_from_path(&args.active, args.as_of)?;
    let prior: &Snapshot = active.snapshot();

    // Resolve the evidence port. Only the reviewed-inputs path is wired offline; the live
    // transport is a maintainer-run impl requiring an injected fetch.
    let inputs_path = args
        .inputs
        .ok_or("live transport requires an operator-supplied fetch; pass --inputs <file>")?;
    let inputs_bytes = std::fs::read(&inputs_path)?;
    let inputs = serde_json::from_slice(&inputs_bytes)?;
    let port = StaticEvidencePort::new(inputs);

    let scope = RefreshScope {
        from: prior.coverage.materialized_from,
        through: args.through,
    };
    let prior_forward_horizon = prior.freshness.forward_readiness_through;
    let outcome = refresh(
        prior,
        &port,
        scope,
        args.mode,
        args.as_of,
        default_operating_horizon(args.as_of),
    );

    let artifacts = write_candidate(&args.active, &outcome)?;

    // Summary to the non-persisted diagnostic channel (redacted by construction — the diff
    // carries no credentials or raw bodies).
    println!(
        "candidate written: {}",
        artifacts.candidate_path.display()
    );
    println!("diff written: {}", artifacts.diff_path.display());
    println!(
        "requires_review={} high_risk={} partial={}",
        outcome.diff.requires_review(),
        outcome.diff.high_risk_entries().count(),
        outcome.diff.partial
    );

    // Forward-horizon outcome, reported explicitly. The candidate diff compares rows, coverage
    // and evidence but NOT freshness, so a refused horizon extension is otherwise indistinguishable
    // from "there was nothing to extend" — an operator running the runbook's forward-extension
    // procedure would see an unchanged `stale` verdict with no way to tell which happened.
    let new_forward_horizon = outcome.candidate.freshness.forward_readiness_through;
    let fmt = |d: Option<NaiveDate>| d.map(|d| d.to_string()).unwrap_or_else(|| "none".to_string());
    match (prior_forward_horizon, new_forward_horizon) {
        (before, after) if before == after && args.through > before.unwrap_or(args.through) => {
            println!(
                "forward_horizon={} REFUSED (asked for {}) — the KASI and generated-rule sources \
                 must both be present, ok, and cover every date past the current horizon; re-check \
                 the fetch covered the requested window",
                fmt(after),
                args.through
            );
        }
        (before, after) if before == after => {
            println!("forward_horizon={} unchanged", fmt(after));
        }
        (before, after) => {
            println!("forward_horizon={} -> {} advanced", fmt(before), fmt(after));
        }
    }
    for entry in outcome.diff.high_risk_entries() {
        println!(
            "  HIGH-RISK {:?} {} {}",
            entry.category,
            entry.date.map(|d| d.to_string()).unwrap_or_default(),
            scrub::scrub_secrets(&entry.detail)
        );
    }
    Ok(ExitCode::SUCCESS)
}

/// Parsed CLI arguments.
#[derive(Debug)]
struct Args {
    active: PathBuf,
    as_of: DateTime<Utc>,
    mode: RefreshMode,
    through: NaiveDate,
    inputs: Option<PathBuf>,
}

impl Args {
    fn parse(mut args: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut active: Option<PathBuf> = None;
        let mut as_of: Option<DateTime<Utc>> = None;
        let mut mode: Option<RefreshMode> = None;
        let mut through: Option<NaiveDate> = None;
        let mut inputs: Option<PathBuf> = None;

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--active" => {
                    active = Some(PathBuf::from(
                        args.next().ok_or("--active needs a path")?,
                    ));
                }
                "--as-of" => {
                    let raw = args.next().ok_or("--as-of needs an RFC3339 instant")?;
                    let parsed = DateTime::parse_from_rfc3339(raw.trim())
                        .map_err(|e| format!("bad --as-of {raw:?}: {e}"))?;
                    as_of = Some(parsed.with_timezone(&Utc));
                }
                "--mode" => {
                    let raw = args.next().ok_or("--mode needs incremental|full")?;
                    mode = Some(match raw.trim() {
                        "incremental" => RefreshMode::Incremental,
                        "full" | "full-history" => RefreshMode::FullHistory,
                        other => return Err(format!("unknown --mode {other:?} (want incremental|full)")),
                    });
                }
                "--through" => {
                    let raw = args.next().ok_or("--through needs a YYYY-MM-DD date")?;
                    through = Some(
                        NaiveDate::parse_from_str(raw.trim(), "%Y-%m-%d")
                            .map_err(|e| format!("bad --through {raw:?}: {e}"))?,
                    );
                }
                "--inputs" => {
                    inputs = Some(PathBuf::from(
                        args.next().ok_or("--inputs needs a path")?,
                    ));
                }
                other => {
                    return Err(format!(
                        "unknown argument {other:?} (want --active / --as-of / --mode / --through / --inputs)"
                    ))
                }
            }
        }

        Ok(Args {
            active: active.ok_or("missing required --active <path>")?,
            as_of: as_of.ok_or("missing required --as-of <RFC3339 instant>")?,
            mode: mode.ok_or("missing required --mode incremental|full")?,
            through: through.ok_or("missing required --through <YYYY-MM-DD>")?,
            inputs,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_required_args() {
        let args = Args::parse(
            [
                "--active", "/data/cal.json", "--as-of", "2012-06-06T00:00:00Z", "--mode",
                "incremental", "--through", "2012-06-08",
            ]
            .into_iter()
            .map(String::from),
        )
        .expect("parses");
        assert_eq!(args.active, PathBuf::from("/data/cal.json"));
        assert_eq!(args.mode, RefreshMode::Incremental);
        assert_eq!(args.through, NaiveDate::from_ymd_opt(2012, 6, 8).unwrap());
        assert!(args.inputs.is_none());
    }

    #[test]
    fn missing_active_is_an_error_not_a_panic() {
        let err = Args::parse(
            ["--as-of", "2012-06-06T00:00:00Z", "--mode", "full", "--through", "2012-06-08"]
                .into_iter()
                .map(String::from),
        )
        .unwrap_err();
        assert!(err.contains("--active"), "{err}");
    }
}
