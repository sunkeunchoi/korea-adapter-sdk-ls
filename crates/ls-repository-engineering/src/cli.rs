//! Thin command parsing for deterministic package maintenance.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Generate,
    Check,
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
        _ => return Err(CliError),
    };
    if args.next().is_some() {
        return Err(CliError);
    }
    Ok(command)
}

pub const HELP: &str = "Usage: ls-repository-engineering <generate|check>\n";
