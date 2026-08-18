use std::path::Path;

pub use repository_engineering_runtime::{machine, model};

#[path = "../comparison/mod.rs"]
mod comparison;

const POLICY: &str = ".repository-engineering/scenarios/audit-carried-rows/comparison-policy.toml";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let repository_root = arguments.next().ok_or("missing repository root")?;
    let output_root = arguments.next().ok_or("missing output root")?;
    if arguments.next().is_some() {
        return Err("unexpected argument".into());
    }
    let evidence = comparison::run_comparison(Path::new(&repository_root), POLICY)?;
    if !evidence.bounded_agreement {
        return Err(comparison::ComparisonError::SemanticDifference.into());
    }
    let output = comparison::write_external_evidence(
        Path::new(&repository_root),
        Path::new(&output_root),
        &evidence,
    )?;
    println!("{}", output.display());
    Ok(())
}
