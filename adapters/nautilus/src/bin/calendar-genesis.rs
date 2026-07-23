//! `calendar-genesis` — the maintainer genesis-candidate CLI (U5, KTD4).
//!
//! `calendar-genesis --inputs <inputs.json> --out <candidate> --as-of <RFC3339> \
//!     --authority <label> --granted <RFC3339> --expires <RFC3339> \
//!     --krx-through <YYYY-MM-DD> [--horizon-through <YYYY-MM-DD>] \
//!     [--consumer-from <YYYY-MM-DD>] [--state-root <dir>]`
//!
//! Turns an owner-local normalized-inputs artifact into a predecessor-less genesis candidate
//! plus its reviewable genesis description artifact (both written 0o600 beneath the owner-local
//! state root). Refuses in code — never a partial build — when a source's coverage falls short
//! of the genesis window (KTD2/AE9) or a consumer-window weekday remains Unknown (R12/KTD6),
//! printing `refused:` with the offending dates/ranges and exiting non-zero without writing a
//! candidate.
//!
//! The build reuses the production candidate machinery ([`build_genesis`]); only the
//! predecessor-absence is genesis-specific. No credential ever enters here — genesis is a pure
//! offline transform of a reviewed inputs artifact.

use std::path::PathBuf;
use std::process::ExitCode;

use chrono::{DateTime, NaiveDate, Utc};
use nautilus_ls::calendar_refresh::{
    build_genesis, confine, default_operating_horizon, describe_genesis, write_genesis, DateRange,
    GenesisParams, RefreshInputs, CONSUMER_WINDOW_START, HISTORY_FLOOR,
};
use nautilus_ls::scrub;
use nautilus_ls_calendar::schema::{Authorization, CalendarScope, SourceAvailabilityBound};

const STATE_ROOT_ENV: &str = "LS_CALENDAR_STATE_ROOT";
const DEFAULT_STATE_ROOT: &str = "state";

fn main() -> ExitCode {
    scrub::install();
    // Emit-before-fallible-parse (KTD4).
    eprintln!("calendar-genesis: starting");
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

    // Confine the candidate output beneath the owner-local state root.
    std::fs::create_dir_all(&args.state_root)?;
    let out = confine(&args.state_root, &args.out)?;

    // Read the reviewed normalized-inputs artifact.
    let inputs_bytes = std::fs::read(&args.inputs)?;
    let inputs: RefreshInputs = serde_json::from_slice(&inputs_bytes)?;

    let floor = NaiveDate::from_ymd_opt(HISTORY_FLOOR.0, HISTORY_FLOOR.1, HISTORY_FLOOR.2)
        .expect("history floor is valid");
    let horizon_through = args
        .horizon_through
        .unwrap_or_else(|| default_operating_horizon(args.as_of).1);
    let window = DateRange::new(floor, horizon_through);
    let consumer_from = args.consumer_from.unwrap_or_else(|| {
        NaiveDate::from_ymd_opt(CONSUMER_WINDOW_START.0, CONSUMER_WINDOW_START.1, CONSUMER_WINDOW_START.2)
            .expect("consumer window start is valid")
    });
    let consumer_window = DateRange::new(consumer_from, args.krx_through);

    let params = GenesisParams {
        scope: CalendarScope {
            calendar_name: "KRX domestic equity regular session".to_string(),
            venue: "XKRX".to_string(),
            instrument_class: "domestic-equity".to_string(),
            timezone: "Asia/Seoul".to_string(),
            synthetic: false,
        },
        authorization: Authorization {
            authorized: true,
            authority: args.authority,
            granted_at: args.granted,
            expires_at: Some(args.expires),
            terminated_at: None,
        },
        source_availability: vec![SourceAvailabilityBound {
            source_id: "krx-daily".to_string(),
            available_from: Some(floor),
            available_through: None,
        }],
        window,
        krx_through: args.krx_through,
        consumer_window,
    };

    let candidate = match build_genesis(&params, &inputs, args.as_of) {
        Ok(candidate) => candidate,
        Err(refusal) => {
            eprintln!("refused: {refusal}");
            return Ok(ExitCode::FAILURE);
        }
    };

    let description = describe_genesis(&candidate, consumer_window);
    let artifacts = write_genesis(&out, &candidate, &description)?;

    println!("genesis candidate written: {}", artifacts.candidate_path.display());
    println!("genesis description written: {}", artifacts.description_path.display());
    println!("candidate artifact_id: {}", candidate.artifact_id);
    println!(
        "rows: trading_session={} closed={} unknown={} (coverage {}..{})",
        description.trading_session_rows,
        description.closed_rows,
        description.unknown_rows,
        description.coverage_from,
        description.coverage_through,
    );
    println!(
        "consumer-window Unknown weekdays: {} (R12 requires 0)",
        description.consumer_window_unknown_weekdays
    );
    Ok(ExitCode::SUCCESS)
}

/// Parsed CLI arguments.
#[derive(Debug)]
struct Args {
    inputs: PathBuf,
    out: PathBuf,
    as_of: DateTime<Utc>,
    authority: String,
    granted: DateTime<Utc>,
    expires: DateTime<Utc>,
    krx_through: NaiveDate,
    horizon_through: Option<NaiveDate>,
    consumer_from: Option<NaiveDate>,
    state_root: PathBuf,
}

impl Args {
    fn parse(mut args: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut inputs: Option<PathBuf> = None;
        let mut out: Option<PathBuf> = None;
        let mut as_of: Option<DateTime<Utc>> = None;
        let mut authority: Option<String> = None;
        let mut granted: Option<DateTime<Utc>> = None;
        let mut expires: Option<DateTime<Utc>> = None;
        let mut krx_through: Option<NaiveDate> = None;
        let mut horizon_through: Option<NaiveDate> = None;
        let mut consumer_from: Option<NaiveDate> = None;
        let mut state_root: Option<PathBuf> = None;

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--inputs" => inputs = Some(PathBuf::from(args.next().ok_or("--inputs needs a path")?)),
                "--out" => out = Some(PathBuf::from(args.next().ok_or("--out needs a path")?)),
                "--as-of" => as_of = Some(parse_instant(&args.next().ok_or("--as-of needs an instant")?)?),
                "--authority" => authority = Some(args.next().ok_or("--authority needs a label")?),
                "--granted" => granted = Some(parse_instant(&args.next().ok_or("--granted needs an instant")?)?),
                "--expires" => expires = Some(parse_instant(&args.next().ok_or("--expires needs an instant")?)?),
                "--krx-through" => krx_through = Some(parse_date(&args.next().ok_or("--krx-through needs a date")?)?),
                "--horizon-through" => horizon_through = Some(parse_date(&args.next().ok_or("--horizon-through needs a date")?)?),
                "--consumer-from" => consumer_from = Some(parse_date(&args.next().ok_or("--consumer-from needs a date")?)?),
                "--state-root" => state_root = Some(PathBuf::from(args.next().ok_or("--state-root needs a path")?)),
                other => {
                    return Err(format!(
                        "unknown argument {other:?} (want --inputs / --out / --as-of / --authority / --granted / --expires / --krx-through / --horizon-through / --consumer-from / --state-root)"
                    ))
                }
            }
        }

        Ok(Args {
            inputs: inputs.ok_or("missing required --inputs <path>")?,
            out: out.ok_or("missing required --out <path>")?,
            as_of: as_of.ok_or("missing required --as-of <RFC3339>")?,
            authority: authority.ok_or("missing required --authority <label>")?,
            granted: granted.ok_or("missing required --granted <RFC3339>")?,
            expires: expires.ok_or("missing required --expires <RFC3339>")?,
            krx_through: krx_through.ok_or("missing required --krx-through <YYYY-MM-DD>")?,
            horizon_through,
            consumer_from,
            state_root: state_root
                .or_else(|| std::env::var(STATE_ROOT_ENV).ok().map(PathBuf::from))
                .unwrap_or_else(|| PathBuf::from(DEFAULT_STATE_ROOT)),
        })
    }
}

fn parse_date(raw: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(raw.trim(), "%Y-%m-%d").map_err(|e| format!("bad date {raw:?}: {e}"))
}

fn parse_instant(raw: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(raw.trim())
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| format!("bad instant {raw:?}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_required_args() {
        let args = Args::parse(
            [
                "--inputs", "state/i.json", "--out", "state/cal.json.candidate",
                "--as-of", "2026-09-06T00:00:00Z", "--authority", "KRX Open API Agreement",
                "--granted", "2026-01-01T00:00:00Z", "--expires", "2027-01-01T00:00:00Z",
                "--krx-through", "2026-09-04",
            ]
            .into_iter()
            .map(String::from),
        )
        .expect("parses");
        assert_eq!(args.authority, "KRX Open API Agreement");
        assert_eq!(args.krx_through, NaiveDate::from_ymd_opt(2026, 9, 4).unwrap());
        assert!(args.horizon_through.is_none());
    }

    #[test]
    fn missing_krx_through_is_an_error() {
        let err = Args::parse(
            [
                "--inputs", "i", "--out", "o", "--as-of", "2026-09-06T00:00:00Z",
                "--authority", "A", "--granted", "2026-01-01T00:00:00Z", "--expires", "2027-01-01T00:00:00Z",
            ]
            .into_iter()
            .map(String::from),
        )
        .unwrap_err();
        assert!(err.contains("--krx-through"), "{err}");
    }
}
