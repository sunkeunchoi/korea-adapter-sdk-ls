use std::path::Path;

use ls_repository_engineering::cli::Command;
use ls_repository_engineering::generate::{check_projection_set, generate_projection_set};
use ls_repository_engineering::repository::compose_repository;

fn main() {
    let command = match ls_repository_engineering::cli::parse_command(std::env::args().skip(1)) {
        Ok(command) => command,
        Err(_) => {
            eprint!("{}", ls_repository_engineering::cli::HELP);
            std::process::exit(64);
        }
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace crate has a repository root");
    let projections = match compose_repository(root) {
        Ok(projections) => projections,
        Err(error) => {
            eprintln!(
                "path=.repository-engineering code={} remediation=repair_authored_package",
                error.code
            );
            std::process::exit(2);
        }
    };

    match command {
        Command::Generate => {
            if let Err(error) = generate_projection_set(root, &projections) {
                eprintln!(
                    "path=.repository-engineering code={} remediation=repair_projection",
                    error.code
                );
                std::process::exit(2);
            }
        }
        Command::Check => {
            let findings = check_projection_set(root, &projections);
            for finding in &findings {
                println!(
                    "path={} code={} remediation={}",
                    finding.path, finding.code, finding.remediation
                );
            }
            if !findings.is_empty() {
                std::process::exit(1);
            }
        }
    }
}
