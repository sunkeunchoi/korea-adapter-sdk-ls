//! `calendar-activate` — the maintainer activation CLI (U15, KTD9).
//!
//! `calendar-activate --active <path> --candidate <path> --approval <approval.json> \
//!     --as-of <RFC3339>`
//!
//! Revalidates the candidate + predecessor, refuses stale-base / invalid / unreviewed /
//! unacknowledged-high-risk candidates, records the approval, and ATOMICALLY installs the
//! candidate over the active path (owner-readable `0o600`). The active production snapshot
//! lives only under the gitignored, owner-readable `/state` tree — no KRX-derived rows are
//! ever committed.
//!
//! The `--approval` file is a reviewed, signed-off [`ActivationApproval`] JSON naming the
//! exact candidate `artifact_id` and acknowledging every required high-risk / partial key
//! (`calendar-refresh` printed the HIGH-RISK entries; this tool refuses until they are
//! acknowledged). Credentials never enter here — activation is a pure filesystem operation.

use std::path::PathBuf;
use std::process::ExitCode;

use chrono::{DateTime, Utc};
use nautilus_ls::calendar_refresh::{activate, first_install, ActivationApproval};
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

    // First-install (genesis) mode installs a predecessor-less chain root with the full ceremony
    // minus the stale-base/active-load legs, guarded by an exclusive-create install (KTD5).
    if args.first_install {
        return match first_install(&args.active, &args.candidate, &approval, args.as_of) {
            Ok(record) => {
                println!("{}", serde_json::to_string_pretty(&record)?);
                println!("genesis installed: {}", args.active.display());
                Ok(ExitCode::SUCCESS)
            }
            Err(e) => {
                eprintln!("refused: {}", scrub::scrub_secrets(&e.to_string()));
                Ok(ExitCode::FAILURE)
            }
        };
    }

    match activate(&args.active, &args.candidate, &approval, args.as_of) {
        Ok(record) => {
            // The record carries only approval provenance + identities — no KRX rows, no
            // credentials — safe for the non-persisted diagnostic channel.
            println!("{}", serde_json::to_string_pretty(&record)?);
            println!("activated: {}", args.active.display());
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
    candidate: PathBuf,
    approval: PathBuf,
    as_of: DateTime<Utc>,
    first_install: bool,
}

impl Args {
    fn parse(mut args: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut active: Option<PathBuf> = None;
        let mut candidate: Option<PathBuf> = None;
        let mut approval: Option<PathBuf> = None;
        let mut as_of: Option<DateTime<Utc>> = None;
        let mut first_install = false;

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--active" => {
                    active = Some(PathBuf::from(args.next().ok_or("--active needs a path")?));
                }
                "--candidate" => {
                    candidate = Some(PathBuf::from(args.next().ok_or("--candidate needs a path")?));
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
                "--first-install" => first_install = true,
                other => {
                    return Err(format!(
                        "unknown argument {other:?} (want --active / --candidate / --approval / --as-of / --first-install)"
                    ))
                }
            }
        }

        Ok(Args {
            active: active.ok_or("missing required --active <path>")?,
            candidate: candidate.ok_or("missing required --candidate <path>")?,
            approval: approval.ok_or("missing required --approval <path>")?,
            as_of: as_of.ok_or("missing required --as-of <RFC3339 instant>")?,
            first_install,
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
                "--active", "/state/cal.json", "--candidate", "/state/cal.json.candidate",
                "--approval", "/tmp/approval.json", "--as-of", "2012-06-06T00:00:00Z",
            ]
            .into_iter()
            .map(String::from),
        )
        .expect("parses");
        assert_eq!(args.active, PathBuf::from("/state/cal.json"));
        assert_eq!(args.candidate, PathBuf::from("/state/cal.json.candidate"));
        assert_eq!(args.approval, PathBuf::from("/tmp/approval.json"));
    }

    #[test]
    fn missing_candidate_is_an_error_not_a_panic() {
        let err = Args::parse(
            ["--active", "/state/cal.json", "--approval", "/tmp/a.json", "--as-of", "2012-06-06T00:00:00Z"]
                .into_iter()
                .map(String::from),
        )
        .unwrap_err();
        assert!(err.contains("--candidate"), "{err}");
    }
}
