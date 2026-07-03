//! `lab-live` — operator-gated live paper session (U6). Never runs in the gate.
fn main() -> anyhow::Result<()> {
    nautilus_ls_lab::runner::live::main_cli()
}
