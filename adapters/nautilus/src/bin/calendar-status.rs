//! `calendar-status` — the offline calendar preflight diagnostic (U8, AC10).
//!
//! `calendar-status --as-of <RFC3339> --snapshot <path> [--day <YYYY-MM-DD>] [--json]`
//!
//! Resolves the EXPLICIT snapshot path (composition root — the core reads no env / picks
//! no default), loads the calendar at the supplied as-of instant (typed load failures
//! become a diagnostic, never a crash), builds an as-of view, and renders a stable human
//! (default) or JSON (`--json`) diagnostic. Every field is REDACTED by construction — no
//! credential or authorization identity is ever printed.
//!
//! Exit code: `0` for a usable factual outcome (healthy / stale / Unknown / conflict);
//! `1` for a load/use/query failure (missing, corrupt, incompatible, unauthorized,
//! expired, out-of-range) so a preflight script can gate on it.

use std::path::PathBuf;
use std::process::ExitCode;

use chrono::{DateTime, Duration, NaiveDate, Utc};
use nautilus_ls::scrub;
use nautilus_ls_calendar::diagnostics::{render_human, render_json, CalendarDiagnostic};
use nautilus_ls_calendar::KrxCalendar;

fn main() -> ExitCode {
    // Credential hygiene before any output (repo convention for the bin targets).
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

    // The as-of view re-checks authorization at this instant (KTD5); a target day
    // defaults to the KST civil date of the as-of instant.
    let target = args
        .day
        .unwrap_or_else(|| (args.as_of + Duration::hours(9)).date_naive());

    // Composition root: resolve the explicit path + load. A typed load failure becomes a
    // diagnostic, not a crash.
    let diagnostic = match KrxCalendar::load_from_path(&args.snapshot, args.as_of) {
        Ok(calendar) => match calendar.as_of(args.as_of) {
            Ok(view) => CalendarDiagnostic::from_view(&view, target),
            Err(err) => CalendarDiagnostic::from_load_error(args.as_of, &err),
        },
        Err(err) => CalendarDiagnostic::from_load_error(args.as_of, &err),
    };

    let rendered = if args.json {
        render_json(&diagnostic)
    } else {
        render_human(&diagnostic)
    };
    print!("{rendered}");
    if !rendered.ends_with('\n') {
        println!();
    }

    Ok(if diagnostic.outcome.is_usable() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

/// The parsed CLI arguments.
#[derive(Debug)]
struct Args {
    as_of: DateTime<Utc>,
    snapshot: PathBuf,
    day: Option<NaiveDate>,
    json: bool,
}

impl Args {
    fn parse(mut args: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut as_of: Option<DateTime<Utc>> = None;
        let mut snapshot: Option<PathBuf> = None;
        let mut day: Option<NaiveDate> = None;
        let mut json = false;

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--as-of" => {
                    let raw = args.next().ok_or("--as-of needs an RFC3339 instant")?;
                    let parsed = DateTime::parse_from_rfc3339(raw.trim())
                        .map_err(|e| format!("bad --as-of {raw:?}: {e}"))?;
                    as_of = Some(parsed.with_timezone(&Utc));
                }
                "--snapshot" => {
                    let raw = args.next().ok_or("--snapshot needs a path")?;
                    snapshot = Some(PathBuf::from(raw));
                }
                "--day" => {
                    let raw = args.next().ok_or("--day needs a YYYY-MM-DD date")?;
                    let parsed = NaiveDate::parse_from_str(raw.trim(), "%Y-%m-%d")
                        .map_err(|e| format!("bad --day {raw:?}: {e}"))?;
                    day = Some(parsed);
                }
                "--json" => json = true,
                other => return Err(format!("unknown argument {other:?} (want --as-of / --snapshot / --day / --json)")),
            }
        }

        Ok(Args {
            as_of: as_of.ok_or("missing required --as-of <RFC3339 instant>")?,
            snapshot: snapshot.ok_or("missing required --snapshot <path>")?,
            day,
            json,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_required_args_and_flags() {
        let args = Args::parse(
            [
                "--as-of",
                "2012-06-01T00:00:00Z",
                "--snapshot",
                "/tmp/cal.json",
                "--json",
            ]
            .into_iter()
            .map(String::from),
        )
        .expect("parses");
        assert_eq!(args.snapshot, PathBuf::from("/tmp/cal.json"));
        assert!(args.json);
        assert!(args.day.is_none());
    }

    #[test]
    fn missing_snapshot_is_an_error_not_a_panic() {
        let err = Args::parse(
            ["--as-of", "2012-06-01T00:00:00Z"].into_iter().map(String::from),
        )
        .unwrap_err();
        assert!(err.contains("--snapshot"), "{err}");
    }

    #[test]
    fn optional_day_parses() {
        let args = Args::parse(
            [
                "--as-of",
                "2012-06-01T00:00:00Z",
                "--snapshot",
                "s.json",
                "--day",
                "2010-01-04",
            ]
            .into_iter()
            .map(String::from),
        )
        .expect("parses");
        assert_eq!(args.day, NaiveDate::from_ymd_opt(2010, 1, 4));
    }
}
