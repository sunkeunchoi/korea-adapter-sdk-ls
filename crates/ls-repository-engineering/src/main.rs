use std::path::Path;

use ls_repository_engineering::bounded_evidence::import_bounded_evidence;
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
            if error.findings.is_empty() {
                eprintln!(
                    "path=.repository-engineering field=package code={} remediation=repair_authored_package",
                    error.code
                );
            } else {
                for finding in &error.findings {
                    print_finding(finding, true);
                }
            }
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
                print_finding(finding, false);
            }
            if !findings.is_empty() {
                std::process::exit(1);
            }
        }
        Command::ImportBoundedEvidence(path) => match import_bounded_evidence(root, &path) {
            Ok(reference) => println!("path={} sha256={}", reference.path.0, reference.sha256.0),
            Err(error) => {
                eprintln!(
                        "path=.repository-engineering/evidence/bounded code={} remediation=repair_bounded_evidence",
                        error
                    );
                std::process::exit(2);
            }
        },
    }
}

fn print_finding(finding: &ls_repository_engineering::validator::Finding, stderr: bool) {
    let logical_id = finding.logical_id.as_deref().unwrap_or("-");
    let message = format!(
        "path={} logical_id={} field={} code={} remediation={}",
        finding.path, logical_id, finding.field, finding.code, finding.remediation
    );
    if stderr {
        eprintln!("{message}");
    } else {
        println!("{message}");
    }
}
