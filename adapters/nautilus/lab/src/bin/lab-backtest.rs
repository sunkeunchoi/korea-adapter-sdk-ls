//! `lab-backtest` — run ORB vN over the catalog and land a registry run (U5).
fn main() -> anyhow::Result<()> {
    nautilus_ls_lab::runner::backtest::main_cli()
}
