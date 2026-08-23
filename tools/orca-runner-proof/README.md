# Orca Runner attended proof

This standalone tool exercises the public Orca CLI as an external Runner. It does
not activate `.repository-engineering` as an Orca plugin, select the optional UI,
or select the worker adapter. `prepare` refuses to proceed unless those package
settings remain inert.

The attempt record must live outside the repository. Each mutating Orca receipt is
persisted to `<state-root>/attempt.json`, so a later invocation can inspect or
resume the exact Run, Task, gate, and Dispatch.

```sh
cargo +1.96.0 run --locked --manifest-path tools/orca-runner-proof/Cargo.toml -- \
  prepare \
  --repository-root /absolute/path/to/korea-adapter-sdk-ls \
  --state-root /absolute/external/path/orca-runner-attempt
```

`prepare` creates a Run, one read-only task, and an operator decision gate. It
does not start a worker. Resolve that gate explicitly in Orca, then use the same
paths with `resume`. Re-run `resume` to inspect and settle the worker.

- `status` performs a read-only remote inspection.
- `cancel` fences only the current supervised Dispatch.
- `retry` is allowed only after a failed or cancelled attempt and sends the exact
  previous Dispatch as `--retry-of`.
- A successful worker is released after its output is archived. A failed worker
  is retained for debugging.

The proof worker may only inspect `git status` and the inactive package manifest.
It is instructed not to edit files, commit, use credentials, or access the
network.
