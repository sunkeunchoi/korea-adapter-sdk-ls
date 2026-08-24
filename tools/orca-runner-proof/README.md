# Orca Runner attended proof

This standalone tool exercises the public Orca CLI as an external Runner. It does
not activate `.repository-engineering` as an Orca plugin, select the optional UI,
or select the worker adapter. `prepare` refuses to proceed unless those package
settings remain inert.

The attempt record must live outside the repository. Each mutating Orca receipt is
persisted to `<state-root>/attempt.json`, so a later invocation can inspect or
resume the exact Run, Task, gate, and Dispatch.

## Operator workflow

Use a new stable attempt id for each attended run. It may contain letters,
numbers, dots, underscores, and hyphens, and must contain at least one character
other than a dot. Prefer lowercase ids so case-insensitive filesystems cannot
alias two spellings to the same attempt directory. The operator wrapper stores
attempts under:

```text
${XDG_STATE_HOME}/korea-adapter-sdk-ls/orca-runner/<attempt-id>
```

When `XDG_STATE_HOME` is unset, the base is
`${HOME}/.local/state/korea-adapter-sdk-ls/orca-runner`. Set
`ORCA_RUNNER_STATE_BASE` to an absolute external directory to override the base.
Unlike `/tmp`, this convention is intended to survive restarts and preserve the
receipts needed for status, recovery, or audit.

Prepare one attempt from the repository root:

```sh
make orca-runner-prepare ORCA_RUNNER_ATTEMPT=2026-08-24-attended-01
```

`prepare` verifies Orca 1.4.188, the exact current worktree, and the inactive
package settings. It creates a Run, read-only Task, and decision gate, then stops
without starting a worker. Read the returned IDs and resolve that exact gate:

```sh
orca orchestration gate-resolve \
  --id <gate-id> \
  --resolution approved \
  --json
make orca-runner-resume ORCA_RUNNER_ATTEMPT=2026-08-24-attended-01
```

Approval is per attempt. Never resolve a gate from a previous attempt as a
shortcut. Re-run `resume` with the same attempt id after the worker settles: a
successful worker is archived and released, while a failed worker is retained
for attended diagnosis.

The remaining operator actions use the same attempt id:

```sh
make orca-runner-status ORCA_RUNNER_ATTEMPT=2026-08-24-attended-01
make orca-runner-cancel ORCA_RUNNER_ATTEMPT=2026-08-24-attended-01
make orca-runner-retry ORCA_RUNNER_ATTEMPT=2026-08-24-attended-01
```

- `status` is read-only and is the first recovery action after interruption.
- `cancel` fences only the persisted current Dispatch.
- `retry` is accepted only after a failed or cancelled attempt and links the
  replacement to the exact prior Dispatch.
- Re-running `prepare` with an existing attempt id fails closed; use `status` or
  `resume` rather than creating overlapping state.

`attempt.lock` protects each attempt from overlapping mutating commands. If a
mutating command is interrupted and leaves that file behind, run `status` first
and confirm that no `operator.sh`, Cargo, or `orca-runner-proof` process is
still using the exact state root printed by the wrapper. Reconcile any remote
mutation shown by `status`, remove only `<state-root>/attempt.lock`, and then
run `resume` with the same attempt id. Never remove `attempt.json`; it is the
durable receipt needed to recover the attempt.

`ORCA_RUNNER_ORCA` may name a different direct Orca executable, and
`ORCA_RUNNER_CARGO` may name a rustup-compatible Cargo executable because the
wrapper selects toolchain `+1.96.0`. These variables accept an executable name
or path, not a shell command with flags.

The offline wrapper contract is part of the repository-engineering gate:

```sh
make orca-runner-operator-check
```

It uses a stub Cargo executable and does not contact Orca, start a worker, read
credentials, or access the network.

## Direct invocation

The lower-level command remains available when an explicit state root is more
appropriate than the operator convention:

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
