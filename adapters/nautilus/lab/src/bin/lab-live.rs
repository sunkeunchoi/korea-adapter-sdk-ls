//! `lab-live` — the operator-facing live surface: the dispatch pre-flight gate
//! (`--dispatch`), chain genesis (`--genesis`), the attended mounted session
//! (`--mount`), and the ladder controls (`--head`, `--rung-report`, `--escalate`,
//! `--reregister`, `--clear-killswitch`), and the daily rehearsal lane (`--rehearse-daily`,
//! `--rehearsal-clear-trip`, `--rehearsal-book adopt`). Never runs the mounted session in the
//! commit gate: `--mount` is TTY-gated and operator-only.
fn main() -> std::process::ExitCode {
    nautilus_ls_lab::runner::live::main_cli()
}
