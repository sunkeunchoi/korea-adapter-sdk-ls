//! Thin CLI shell over [`ls_docgen::run_cli`].
//!
//! Default (no flag) writes the docs; `--check` compares the committed docs
//! against current metadata and exits non-zero on drift. All logic lives in the
//! library so it is unit-testable; this binary only maps the result to an exit
//! code and prints located errors to stderr.

use std::process::ExitCode;

fn main() -> ExitCode {
    match ls_docgen::run_cli(std::env::args().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
