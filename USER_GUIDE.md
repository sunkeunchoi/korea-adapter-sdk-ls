# Contributor Guide

**Audience:** maintainers and contributors working *in* this repository. This is
a thin orientation to the workflow — it points at the authoritative sources
rather than restating them.

**What this guide does not own:**

- **The recipe steps** for tracking / implementing / promoting a TR —
  [`.agents/skills/`](.agents/skills/) (read a recipe's `SKILL.md` before running it).
- **The gate details, gotchas, and live-smoke registry** —
  [`AGENTS.md`](AGENTS.md) and [`docs/MAINTENANCE_RUNBOOK.md`](docs/MAINTENANCE_RUNBOOK.md).
- **The support ladder and its gates** — [`TR_LIFECYCLE.md`](TR_LIFECYCLE.md).
- **The workspace structure** — [`ARCHITECTURE.md`](ARCHITECTURE.md).

---

## Build

A standard Cargo workspace — no external SDK repo is needed to build or test.

```bash
cargo build            # whole workspace
cargo test             # whole workspace (offline; live smokes are #[ignore]d)
```

The offline test suite runs against a mocked gateway (`ls-sdk-test-support`), so
it needs no credentials. The credential-gated **live smokes** are `#[ignore]`d
and never run as part of `cargo test`.

## The gate — run before committing TR/SDK/metadata changes

```bash
make docs              # regenerate docs/ from metadata
cargo test             # workspace
cargo test -p ls-core  # metadata validation + policy-index cross-check
make docs-check        # assert generated docs match committed
make lane-check        # smoke-harness fail-fast lane guard (offline; no gateway)
```

Keep the tree green — never commit with a red gate. `make docs-check` is the guard
that generated docs can't drift from metadata; `cargo test -p ls-core` is the
guard that every runtime `{TR}_POLICY` matches a metadata record.

## Working on a TR

TRs climb a four-rung support ladder (Raw → Tracked → Implemented → Recommended);
each rung is a separately-gated promotion described in
[`TR_LIFECYCLE.md`](TR_LIFECYCLE.md). Each transition has a **frozen recipe** —
read its `SKILL.md` first, then run it:

| Transition | Recipe |
|------------|--------|
| Raw → Tracked | [`track-tr`](.agents/skills/track-tr/) / [`track-realtime-tr`](.agents/skills/track-realtime-tr/) |
| Tracked → Implemented | [`implement-tr`](.agents/skills/implement-tr/) / [`implement-realtime-tr`](.agents/skills/implement-realtime-tr/) / [`implement-order-tr`](.agents/skills/implement-order-tr/) |
| Implemented → Recommended | [`promote-tr`](.agents/skills/promote-tr/) / [`promote-trs`](.agents/skills/promote-trs/) |

## Live smokes & credentials

Live smokes hit the **real LS paper gateway** and require `LS_TRADING_ENV=paper`.

- Credentials come from a **gitignored, named per-lane env file** — `.env.domestic`
  by default, `.env.<lane>` per instrument domain (`.env.overseas`,
  `.env.domestic_option`, `.env.overseas_option`). There is **no legacy `.env`
  fallback**: a lane whose file is absent **fails fast** rather than silently
  authenticating as the wrong account. See `.env.example` for the shape.
- Run a smoke with `make live-smoke-<tr>`; the registry of smoke targets is
  [`.agents/skills/promote-tr/references/smoke-map.md`](.agents/skills/promote-tr/references/smoke-map.md).
- `make raw-probe LS_PROBE_TR_CD=.. LS_PROBE_PATH=.. LS_PROBE_BODY=..` is the
  credential-safe failure classifier (prints only http / rsp_cd / body_len) — use
  it to A/B request-body shapes.

Why per-lane files: the LS gateway resolves the target account **entirely from the
OAuth token**, never from the account number on the wire. One appkey reaches
exactly one account, so reaching a different account requires a different appkey —
hence one credential file per lane. See the **Credential lane** entry in
[`CONCEPTS.md`](CONCEPTS.md).

## Gotchas (pointers)

The load-bearing gotchas live in [`AGENTS.md`](AGENTS.md); the two most common:

- **Numeric request-body fields** must serialize as JSON numbers
  (`#[serde(serialize_with = "ls_core::string_as_number")]`) or the gateway returns
  `IGW40011`. Response fields use the tolerant `string_or_number`.
- **Do not `cargo fmt` the whole `ls-trackers` crate** — `main` is intentionally
  unformatted there and a blanket format produces a huge spurious diff.

Documented solutions to past problems are catalogued under
[`docs/solutions/`](docs/solutions/), organized by category with YAML frontmatter
(module, tags, problem_type) — search there when debugging in a documented area.
