//! `lab-backtest-daily` — run the daily-resolution, multi-session-hold path over the
//! catalog and land a registry run (P7, U5).
//!
//! The sibling of `lab-backtest`, not a mode of it (KTD2): a different strategy, params,
//! mount, OMS, and hold semantics. Config comes from `LS_DATA_HOME` plus `LS_BTD_*` — see
//! [`nautilus_ls_lab::runner::backtest_daily::main_cli`].
fn main() -> anyhow::Result<()> {
    nautilus_ls_lab::runner::backtest_daily::main_cli()
}
