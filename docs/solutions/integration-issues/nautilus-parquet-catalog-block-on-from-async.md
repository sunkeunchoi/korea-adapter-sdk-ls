---
title: "nautilus ParquetDataCatalog panics from an async context — wrap every catalog call in spawn_blocking"
date: 2026-07-02
category: integration-issues
module: adapters/nautilus ingest + catalog I/O
problem_type: integration_issue
component: tooling
symptoms:
  - "Test/binary panics: 'Cannot start a runtime from within a runtime. This happens because a function (like block_on) attempted to block the current thread while the thread is being used to drive asynchronous tasks.'"
  - "Or: 'Cannot drop a runtime in a context where blocking is not allowed. This happens when a runtime is dropped from within an asynchronous context.'"
  - "Or: 'Failed to create catalog from path: ... Unable to canonicalize filesystem root: <path> ... No such file or directory (os error 2)' when the catalog dir does not exist yet"
root_cause: wrong_api
resolution_type: code_fix
severity: high
related_components:
  - tooling
tags: [nautilus, nautilus-persistence, parquet-catalog, tokio, block-on, spawn-blocking, datafusion, object-store, ingest]
---

# nautilus ParquetDataCatalog panics from an async context

## Problem

`nautilus_persistence::backend::catalog::ParquetDataCatalog` (0.60.0) drives an
internal runtime via `block_on` (datafusion / object_store). Calling any of its
methods — `new`, `write_to_parquet`, `write_instruments`, `bars`, `instruments` —
while a tokio reactor is already driving the current thread panics. This bites
immediately in the adapter's ingest path (`write_bars`) and in any `#[tokio::test]`
that reads bars back.

## Symptoms

- `Cannot start a runtime from within a runtime ... block_on ...` on a write/read.
- `Cannot drop a runtime in a context where blocking is not allowed` when the
  catalog object is even constructed-and-dropped inside an `async fn`.
- `Unable to canonicalize filesystem root: <path> ... No such file or directory`
  when the catalog directory does not exist at `ParquetDataCatalog::new` time —
  `new` canonicalizes the path and requires it to already exist.

## What Didn't Work

- Constructing the catalog once and holding it across `.await` points in an async
  ingest loop: it must be `Send`, and even holding it in the async context risks the
  drop-in-async-context panic. Also churns lifetimes.
- Relying on an outer `create_dir_all` in the ingest `run()` before the first
  `write_bars`: a *standalone* `write_instruments` (called before `run()` in
  `ls-ingest`) still hit the missing-dir panic because it constructs its own
  catalog.

## Solution

Run **every** catalog interaction inside `tokio::task::spawn_blocking`, constructing,
using, and dropping the catalog entirely inside the closure (so the internal
`block_on` runs on a blocking-pool thread where blocking is allowed), and
`create_dir_all` the catalog directory at the top of each write closure:

```rust
// BEFORE — panics from an async caller
async fn write_bars(catalog: &ParquetDataCatalog, bars: &[Bar]) -> AdapterResult<()> {
    catalog.write_to_parquet(bars, None, None, Some(true))?; // block_on -> panic
    Ok(())
}

// AFTER — construct + use + drop the catalog on the blocking pool
async fn write_bars(catalog_path: &Path, bars: Vec<Bar>) -> AdapterResult<()> {
    let path = catalog_path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&path)              // new() canonicalizes -> dir must exist
            .map_err(|e| AdapterError::Ingest(format!("mkdir {}: {e}", path.display())))?;
        let catalog = ParquetDataCatalog::new(&path, None, None, None, None);
        catalog
            .write_to_parquet(&bars, None, None, Some(true)) // Some(true) = skip disjoint check
            .map(|_| ())
            .map_err(|e| AdapterError::Ingest(format!("catalog write: {e}")))
    })
    .await
    .map_err(|e| AdapterError::Ingest(format!("catalog write task panicked: {e}")))?
}
```

The same wrapper is applied to `write_instruments`, `read_all_bars`, and
`read_all_instruments`. Move owned data (`Vec<Bar>`, `PathBuf`) into the closure so
it is `'static + Send`.

## Why This Works

`spawn_blocking` moves execution to a dedicated blocking-pool thread that is **not**
running a tokio reactor, so the catalog's internal `block_on` is legal there. Because
the catalog is created and dropped inside the closure, the runtime it owns is also
dropped on that blocking thread, avoiding the drop-in-async panic. `create_dir_all`
inside the closure makes each entry point self-sufficient rather than depending on a
caller having pre-created the directory.

Note the backtest engine (`nautilus_backtest::engine::BacktestEngine::run`) has the
same constraint — run the whole engine build+run inside `spawn_blocking` from a
`#[tokio::test]`, moving the `Vec<InstrumentAny>` / `Vec<Bar>` in.

## Prevention

- Treat **all** `nautilus-persistence` and `nautilus-backtest` entry points as
  blocking APIs: never call them directly from an `async fn`; always
  `spawn_blocking`. Expose async wrappers (`read_all_bars`, `read_all_instruments`)
  so tests never touch the raw catalog inline.
- Any code path that constructs a `ParquetDataCatalog` for writing must
  `create_dir_all` the base path first (`new` canonicalizes and fails on a missing
  dir).
- Bars must be written ascending by `ts_init`; when the disjoint check is skipped
  with `Some(true)`, the caller owns ordering (LS daily charts return newest-first —
  sort before writing). See [[ls-gateway-t0425-rate-limit-and-pagination-flat-scan]]
  for the broader "SDK metadata the client does not enforce" theme.
