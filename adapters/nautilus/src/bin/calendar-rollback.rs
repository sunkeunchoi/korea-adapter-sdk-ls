//! `calendar-rollback` — the maintainer rollback CLI (U2, KTD5).
//!
//! `calendar-rollback --active <path> --prior <path> --approval <approval.json> \
//!     --as-of <RFC3339>`
//!
//! Restores an earlier `--prior` snapshot over the `--active` path: it recomputes the
//! superseded identity, revalidates the prior snapshot through the real loader at `--as-of`
//! (authorization + integrity), refuses a prior whose materialized coverage no longer includes
//! the `--as-of` operating date (a lapsed-coverage rollback would leave every Enforced consumer
//! refusing on `OutOfRange`), records the supersession, and ATOMICALLY installs the prior bytes
//! (owner-readable `0o600`). Like activation, the install requires a process restart to take
//! effect — the restart-identity proof is the operator's (RUNBOOK-calendar-rollback).
//!
//! The `--approval` file is a reviewed, signed-off [`ActivationApproval`] JSON whose
//! `reviewed_artifact_id` names the exact prior snapshot being restored. Credentials never
//! enter here — rollback is a pure filesystem operation over the gitignored `/state` tree.

use std::path::PathBuf;
use std::process::ExitCode;

use chrono::{DateTime, Utc};
use nautilus_ls::calendar_refresh::{rollback, ActivationApproval};
use nautilus_ls::scrub;

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

    let approval_bytes = std::fs::read(&args.approval)?;
    let approval: ActivationApproval = serde_json::from_slice(&approval_bytes)?;

    match rollback(&args.active, &args.prior, &approval, args.as_of) {
        Ok(record) => {
            // The record carries only approval provenance + restored/superseded identities —
            // no KRX rows, no credentials — safe for the non-persisted diagnostic channel.
            println!("{}", serde_json::to_string_pretty(&record)?);
            println!("rolled back: {}", args.active.display());
            Ok(ExitCode::SUCCESS)
        }
        Err(e) => {
            eprintln!("refused: {}", scrub::scrub_secrets(&e.to_string()));
            Ok(ExitCode::FAILURE)
        }
    }
}

/// Parsed CLI arguments.
#[derive(Debug)]
struct Args {
    active: PathBuf,
    prior: PathBuf,
    approval: PathBuf,
    as_of: DateTime<Utc>,
}

impl Args {
    fn parse(mut args: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut active: Option<PathBuf> = None;
        let mut prior: Option<PathBuf> = None;
        let mut approval: Option<PathBuf> = None;
        let mut as_of: Option<DateTime<Utc>> = None;

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--active" => {
                    active = Some(PathBuf::from(args.next().ok_or("--active needs a path")?));
                }
                "--prior" => {
                    prior = Some(PathBuf::from(args.next().ok_or("--prior needs a path")?));
                }
                "--approval" => {
                    approval = Some(PathBuf::from(args.next().ok_or("--approval needs a path")?));
                }
                "--as-of" => {
                    let raw = args.next().ok_or("--as-of needs an RFC3339 instant")?;
                    let parsed = DateTime::parse_from_rfc3339(raw.trim())
                        .map_err(|e| format!("bad --as-of {raw:?}: {e}"))?;
                    as_of = Some(parsed.with_timezone(&Utc));
                }
                other => {
                    return Err(format!(
                        "unknown argument {other:?} (want --active / --prior / --approval / --as-of)"
                    ))
                }
            }
        }

        Ok(Args {
            active: active.ok_or("missing required --active <path>")?,
            prior: prior.ok_or("missing required --prior <path>")?,
            approval: approval.ok_or("missing required --approval <path>")?,
            as_of: as_of.ok_or("missing required --as-of <RFC3339 instant>")?,
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
                "--active", "/state/cal.json", "--prior", "/state/cal.json.prior",
                "--approval", "/tmp/approval.json", "--as-of", "2012-06-06T00:00:00Z",
            ]
            .into_iter()
            .map(String::from),
        )
        .expect("parses");
        assert_eq!(args.active, PathBuf::from("/state/cal.json"));
        assert_eq!(args.prior, PathBuf::from("/state/cal.json.prior"));
        assert_eq!(args.approval, PathBuf::from("/tmp/approval.json"));
    }

    #[test]
    fn missing_prior_is_an_error_not_a_panic() {
        let err = Args::parse(
            ["--active", "/state/cal.json", "--approval", "/tmp/a.json", "--as-of", "2012-06-06T00:00:00Z"]
                .into_iter()
                .map(String::from),
        )
        .unwrap_err();
        assert!(err.contains("--prior"), "{err}");
    }
}
