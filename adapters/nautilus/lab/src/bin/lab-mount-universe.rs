//! `lab-mount-universe` — resolve the live-mount universe file `--mount` trades.
//!
//! Deliberately a separate binary from `lab-live`: this produces an *input* and authorizes
//! nothing. It takes no nonce, needs no TTY, and appends to no chain, so keeping it out of the
//! attended safety tool keeps that tool's surface entirely nonce-gated.
//!
//! For a PAST session date it is fully offline. When the session date is **today (KST)** it
//! makes exactly one kind of gateway call — a `t8407` market-data quote for `today_open`,
//! which the catalog cannot supply for a session still in progress (see
//! `runner::mount_universe`). That read needs `LS_DISPATCH_LANE_ENV`; it still takes no nonce
//! and still authorizes nothing.
//!
//! ```sh
//! export LS_DATA_HOME=/ABSOLUTE/path/to/data-home
//! export LS_MOUNT_UNIVERSE_DATE=2026-07-27          # the KST session date
//! export LS_MOUNT_UNIVERSE_METADATA=/…/universe-metadata-YYYYMMDD.json   # if the head is
//!                                                                       #   metadata-driven
//! export LS_DISPATCH_LANE_ENV=/ABSOLUTE/path/to/.env.domestic            # REQUIRED when the
//!                                                                       #   date is today
//! cargo run --release -p nautilus-ls-lab --bin lab-mount-universe -- \
//!   --out /ABSOLUTE/path/to/universe.json
//! ```
//!
//! Then point `LS_MOUNT_UNIVERSE_FILE` at `--out` for the attended `--mount`.

use nautilus_ls_lab::runner::mount_universe;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    nautilus_ls::scrub::install();
    let mut args = std::env::args().skip(1);
    let mut out: Option<std::path::PathBuf> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" => {
                out = Some(
                    args.next()
                        .ok_or_else(|| anyhow::anyhow!("--out requires a path"))?
                        .into(),
                );
            }
            other => anyhow::bail!("unknown argument {other:?} (only --out <path> is accepted)"),
        }
    }
    let cfg = mount_universe::config_from_env()?;
    let rows = mount_universe::resolve(&cfg).await?;
    mount_universe::emit(&rows, out.as_deref())
}
