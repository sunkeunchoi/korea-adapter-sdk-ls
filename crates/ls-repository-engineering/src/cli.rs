//! Thin command parsing for deterministic package maintenance.

use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Generate,
    Check,
    ImportBoundedEvidence(PathBuf),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CliError;

pub fn parse_command<I, S>(args: I) -> Result<Command, CliError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut args = args.into_iter();
    let command = match args.next().as_ref().map(AsRef::as_ref) {
        Some("generate") => Command::Generate,
        Some("check") => Command::Check,
        Some("import-bounded-evidence") => {
            let path = args.next().ok_or(CliError)?;
            Command::ImportBoundedEvidence(PathBuf::from(path.as_ref()))
        }
        _ => return Err(CliError),
    };
    if args.next().is_some() {
        return Err(CliError);
    }
    Ok(command)
}

pub const HELP: &str =
    "Usage: ls-repository-engineering <generate|check|import-bounded-evidence PATH>\n";
