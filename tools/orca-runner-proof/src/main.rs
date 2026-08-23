use std::path::PathBuf;

use orca_runner_proof::{ProcessOrca, ProofRunner, RunnerError};
use serde_json::json;

struct Arguments {
    action: String,
    repository_root: PathBuf,
    state_root: PathBuf,
    orca: PathBuf,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), RunnerError> {
    let arguments = parse_arguments(std::env::args().skip(1))?;
    let mut runner = ProofRunner::new(
        arguments.repository_root,
        arguments.state_root,
        ProcessOrca::new(arguments.orca),
    )?;
    let output = match arguments.action.as_str() {
        "prepare" => serde_json::to_value(runner.prepare()?)?,
        "resume" => serde_json::to_value(runner.resume()?)?,
        "status" => serde_json::to_value(runner.status()?)?,
        "cancel" => serde_json::to_value(runner.cancel()?)?,
        "retry" => serde_json::to_value(runner.retry()?)?,
        action => {
            return Err(RunnerError::InvalidArguments(format!(
                "unknown action `{action}`; expected prepare, resume, status, cancel, or retry"
            )))
        }
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({"ok": true, "result": output}))?
    );
    Ok(())
}

fn parse_arguments(arguments: impl Iterator<Item = String>) -> Result<Arguments, RunnerError> {
    let mut arguments = arguments;
    let action = arguments.next().ok_or_else(usage)?;
    if action == "--help" || action == "-h" {
        println!("{}", usage());
        std::process::exit(0);
    }
    let mut repository_root = None;
    let mut state_root = None;
    let mut orca = PathBuf::from("orca");
    while let Some(flag) = arguments.next() {
        let value = arguments.next().ok_or_else(|| {
            RunnerError::InvalidArguments(format!("{flag} requires a value\n{}", usage()))
        })?;
        match flag.as_str() {
            "--repository-root" => repository_root = Some(PathBuf::from(value)),
            "--state-root" => state_root = Some(PathBuf::from(value)),
            "--orca" => orca = PathBuf::from(value),
            _ => {
                return Err(RunnerError::InvalidArguments(format!(
                    "unknown flag `{flag}`\n{}",
                    usage()
                )))
            }
        }
    }
    Ok(Arguments {
        action,
        repository_root: repository_root.ok_or_else(usage)?,
        state_root: state_root.ok_or_else(usage)?,
        orca,
    })
}

fn usage() -> RunnerError {
    RunnerError::InvalidArguments(
        "usage: orca-runner-proof <prepare|resume|status|cancel|retry> \
         --repository-root <absolute-path> --state-root <absolute-external-path> \
         [--orca <executable>]"
            .into(),
    )
}
