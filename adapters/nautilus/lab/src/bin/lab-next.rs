//! `lab-next` — the window-aware "what now" entry point (U1+): the queue edit
//! surface (`add` / `done` / `supersede` / `list`) now; the default report (U5)
//! and resume probe (U6) in later units.
//!
//! Mirrors `lab-mount-universe`'s posture: no nonce, no TTY, no chain append —
//! its only writes are queue-file mutations (`queue/items.jsonl` at the repo
//! root; `LS_QUEUE_PATH` overrides for tests).
fn main() -> std::process::ExitCode {
    nautilus_ls_lab::runner::next::main_cli()
}
