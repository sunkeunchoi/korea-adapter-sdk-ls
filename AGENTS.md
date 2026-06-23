# AGENTS.md

Guidance for agents working in this repository — a Rust SDK + metadata/tracking
toolchain for LS-securities (Korea) market-data TRs.

## Workspace layout

A Cargo workspace (`resolver = "2"`):

```
crates/ls-core/            # runtime: dispatch (Inner::post / post_paginated), endpoint policies, serde helpers
crates/ls-sdk/             # the public SDK: per-TR request/response structs + facade handles
                           #   src/market_session/  non-paginated reads        src/paginated/  single-page reads
                           #   src/account/ src/realtime/ ...
crates/ls-metadata/        # metadata schema + validator (metadata/trs/*.yaml, tr-index.yaml)
crates/ls-trackers/        # API-drift + spec trackers; baselines/api-drift/normalized/trs/<tr>.json (the wire-shape source of truth)
crates/ls-docgen/          # projects docs/reference/ + docs/tr-dependencies/ from metadata
crates/ls-sdk-test-support/# wiremock helpers for offline SDK tests
docs/solutions/            # documented solutions to past problems (bugs, best practices, patterns), by category with YAML frontmatter (module, tags, problem_type) — relevant when implementing or debugging in a documented area
CONCEPTS.md                # shared domain vocabulary (TR, owner_class, support tiers, Paper Live Smoke, ...) — relevant when orienting or discussing domain concepts
metadata/PROVISIONALITY-LEDGER.md  # per-TR provisional-facet ledger, retired as TRs implement
.agents/skills/            # frozen workflow recipes: track-tr, implement-tr, promote-tr, ... (read the SKILL.md before running one)
```

## Gate (run before committing TR/SDK/metadata changes)

```
make docs            # regenerate docs/ from metadata
cargo test           # workspace
cargo test -p ls-core  # metadata validation + policy index cross-check
make docs-check      # assert generated docs match committed
```

Keep the tree green; never commit with a red gate.

## TR support lifecycle

TRs climb **Raw → Tracked → Implemented → Recommended** (see CONCEPTS.md). The
`track-tr` recipe (`.agents/skills/track-tr/SKILL.md`) brings a raw TR (present
only in the raw OpenAPI capture, no metadata, no baseline) to Tracked by
authoring its `metadata/trs/<tr>.yaml` + `tr-index.yaml` entry and projecting its
normalized baseline via `make api-drift-renormalize` (the baseline is projected,
never hand-authored). The `implement-tr` recipe
(`.agents/skills/implement-tr/SKILL.md`) then flips a Tracked TR to Implemented by
authoring callable Rust and gating it on a **Paper Live Smoke**; `promote-tr`
takes Implemented → Recommended. Each new `{TR}_POLICY` const must
be registered in **both** cross-check lists (see the recipe). Wire field names,
types, and array-vs-single shapes come from the normalized baseline
(`crates/ls-trackers/baselines/api-drift/normalized/trs/<tr>.json`), not guesswork.

## Live smokes & gateway

- `make live-smoke-<tr>` hits the **real LS paper gateway** with credentials from
  a gitignored `.env`; requires `LS_TRADING_ENV=paper`. The smoke registry is
  `.agents/skills/promote-tr/references/smoke-map.md`.
- `make raw-probe LS_PROBE_TR_CD=.. LS_PROBE_PATH=.. LS_PROBE_BODY=..` is the
  credential-safe failure classifier (prints only http/rsp_cd/body_len). Use it to
  A/B request-body shapes — see
  `docs/solutions/integration-issues/ls-gateway-igw40011-numeric-request-fields.md`.

## Gotchas

- Numeric **request-body** fields must serialize as JSON numbers
  (`#[serde(serialize_with = "ls_core::string_as_number")]`) or the gateway returns
  `IGW40011`. Response fields use `string_or_number` (tolerant). See docs/solutions/.
- Do not `cargo fmt` the whole `ls-trackers` crate — `main` is intentionally
  unformatted there and CI does not enforce it; a blanket format produces a huge
  spurious diff.
