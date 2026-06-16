//! Thin CLI shell over [`ls_trackers::run_cli`].
//!
//! All logic lives in the library so it is unit-testable; this binary only maps
//! the resolved tiered exit (R17) to a process exit code: `0` no gating drift,
//! `1` a finding crossed the gate threshold, `2` fetch/parse/baseline/internal
//! error.

use std::process::ExitCode;

fn main() -> ExitCode {
    ExitCode::from(ls_trackers::run_cli(std::env::args().skip(1)).code())
}
