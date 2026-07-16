//! `lab-live` — the dispatch pre-flight gate (`--dispatch`) and chain genesis
//! (`--genesis`); the operator-gated mounted session lands in U6. Never runs the
//! mounted session in the commit gate.
fn main() -> std::process::ExitCode {
    nautilus_ls_lab::runner::live::main_cli()
}
