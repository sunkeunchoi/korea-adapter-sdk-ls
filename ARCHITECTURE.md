# Architecture

**Audience:** maintainers and contributors. This document maps the workspace's
structure and the data-flow that connects the runtime, the metadata, the change
trackers, and the generated docs. It explains *shape and why*.

**What this document does not own:**

- **Per-TR contracts** (what a specific TR claims, what evidence backs it) — the
  generated pages under [`docs/reference/`](docs/reference/) and
  [`docs/tr-dependencies/`](docs/tr-dependencies/), projected by `make docs`.
- **The authoritative vocabulary** — [`CONCEPTS.md`](CONCEPTS.md) (glossary) and
  [`CONTEXT.md`](CONTEXT.md) (language + relationships).
- **The maintenance flow's operational steps** —
  [`docs/MAINTENANCE_RUNBOOK.md`](docs/MAINTENANCE_RUNBOOK.md) and the frozen
  recipes under [`.agents/skills/`](.agents/skills/).
- **The TR support ladder and its gates** — [`TR_LIFECYCLE.md`](TR_LIFECYCLE.md).
- **Design rationale** — the ADRs under [`docs/adr/`](docs/adr/).

The authoritative source of truth for SDK behavior is always the maintained Rust
code plus the per-TR metadata under [`metadata/`](metadata/) — not this document.

---

## The workspace

A Cargo workspace (`resolver = "2"`) of six crates. They stack from transport at
the bottom to projected documentation at the top:

```
ls-docgen        projects docs/reference/ + docs/tr-dependencies/ from metadata
ls-trackers      advisory API-drift + spec-doc trackers; normalized baselines
ls-metadata      metadata schema + validator over metadata/trs/*.yaml, tr-index.yaml
ls-sdk           the public SDK: per-TR request/response structs + facade handles
ls-core          the runtime: dispatch, endpoint policies, auth, dedup, rate limiting
ls-sdk-test-support   wiremock helpers for offline SDK tests
```

Each crate depends only on other crates in this workspace — the repository is
**standalone**, with no build or runtime dependency on any other SDK repo.

### `ls-core` — the runtime

The shared transport and safety machinery, independent of any single TR.

- `inner.rs` — `Inner`, the shared runtime core. Its dispatch entry points are
  the load-bearing seam of the whole SDK:
  - `Inner::post` — a single non-paginated request/response round trip.
  - `Inner::post_paginated` — a request that follows the gateway's
    continuation-key pagination to assemble a full result.
  - `post_order` — the **no-retry** order path that routes through the
    deduplicator and kill switch (see *Order safety* below).
- `endpoint_policy/` — the per-TR runtime policies (`{TR}_POLICY`): endpoint
  path, HTTP method, rate bucket, pagination behavior, and REST-vs-WebSocket
  routing. A policy index is cross-checked against the metadata at test time.
- `auth.rs` — OAuth2 token acquisition and caching. Tokens are fetched lazily
  on first use and shared across every dependency-class handle.
- `order_dedup.rs` — the order deduplicator (a bounded window keyed on order
  identity) that makes a [double fill](CONCEPTS.md) structurally hard.
- `rate_limiter.rs` — the shared rate limiter, bucketed per rate class.
- `config.rs` / `config_resolve.rs` — credential + URL-scheme validation and the
  per-lane credential resolution.
- `parse.rs` — the serde helpers TRs bind through: `string_as_number` /
  `string_as_decimal` for **request** fields that must serialize as JSON numbers
  (or the gateway returns `IGW40011`), and `string_or_number` (tolerant) for
  **response** fields.

### `ls-sdk` — the public surface

The maintained public SDK. Its entry point is `LsSdk` (`src/lib.rs`), a thin
`Arc<Inner>`-backed wrapper. Dependency classes are **modules within this crate**,
not separate crates, and each is reached through an accessor that vends a small
handle sharing the one token cache and rate limiter:

| Accessor | Handle | Dependency class |
|----------|--------|------------------|
| `LsSdk::standalone()` | `standalone::Standalone` | OAuth-only reads (`token`, `revoke`) |
| `LsSdk::market_session()` | `market_session::MarketSession` | non-paginated session reads (e.g. `t1102`) |
| `LsSdk::paginated()` | `paginated::Paginated` | self-paginated / single-page reads (e.g. `t8412`) |
| `LsSdk::account()` | `account::Account` | account-state reads (e.g. `CSPAQ12200`); account sourced from config, not the caller |
| `LsSdk::orders()` | `orders::Orders` | domestic cash-equity order submit/modify/cancel + `t0425` reconcile |
| `LsSdk::fo_orders()` | `orders::FoOrders` | domestic futures/options order chain (`CFOAT00100/00200/00300`) |
| `LsSdk::realtime()` | `Arc<realtime::WsManager>` | realtime WebSocket subscriptions (e.g. `S3_`) |

A TR's **`owner_class`** facet names which of these handles exposes it and which
runtime machinery it uses. The `src/` tree mirrors the classes: `market_session/`,
`paginated/`, `account/`, `orders/`, `realtime/`, `standalone/`.

### `ls-metadata` — schema + validator

The metadata layer. `metadata/trs/<tr>.yaml` is the per-TR maintenance record
(owning dependency class, facets, provisional facets, support tier);
`tr-index.yaml` is the routing summary. This crate validates that record set:

- `schema.rs` / `validator.rs` — schema and cross-consistency validation.
- `shape.rs` — structural shape checks.
- `freshness.rs` — the evidence-freshness accounting (the 90-day backstop for
  Recommended TRs).
- `planner.rs` — dependency/lane planning helpers.

`cargo test -p ls-core` runs metadata validation and the policy-index
cross-check — the guard that every `{TR}_POLICY` matches a metadata record.

### `ls-trackers` — the advisory change trackers

Two **advisory** trackers watch upstream and emit severity-classified findings.
They **never mutate SDK code, metadata, docs, evidence, or baselines** — a run
surfaces findings for human review only.

- `api_drift.rs` — the **API Drift Tracker**: upstream API *shape* changes (TR
  additions/removals, request/response field changes). It normalizes upstream
  data into a **Structural API Shape** before diffing.
- `spec_doc.rs` — the **Specification Document Tracker**: documentation *example*
  drift.
- `fetch.rs` / `stages.rs` — snapshot staging and normalization.
- `freshness.rs` — freshness reporting across tracked TRs.
- `baselines/api-drift/normalized/trs/<tr>.json` — the **normalized baseline**,
  the wire-shape source of truth. Field names, types, and array-vs-single shapes
  come from here, never from guesswork. The baseline is *projected*
  (`make api-drift-renormalize`), never hand-authored.

Runs are operator-invoked: `make api-drift-check`, `make spec-doc-check`.

### `ls-docgen` — projected documentation

Projects [`docs/reference/`](docs/reference/) and
[`docs/tr-dependencies/`](docs/tr-dependencies/) from metadata (`make docs`).
`make docs-check` asserts the committed docs match a fresh projection, so the
generated docs can never silently drift from metadata. Because these counts and
contracts are projected, prose elsewhere should link to them rather than
restate live counts.

### `ls-sdk-test-support`

Wiremock helpers that let SDK tests run offline against a mocked gateway,
separating the fast offline suite from the credential-gated live smokes.

---

## Data flow

Two flows run through this workspace; they meet at the metadata.

**Maintenance flow (upstream change → reviewed SDK change).**

```
upstream LS API/docs change
   │
   ▼
staged snapshot ──► ls-trackers diff vs reviewed baseline ──► advisory finding
                                                                   │  (human review)
                                                                   ▼
                              SDK maintenance work item ──► maintained Rust + metadata edit
                                                                   │
                                                                   ▼
                                          change-scoped gate (cargo test) ──► commit
```

**Projection flow (metadata → docs).**

```
metadata/trs/*.yaml + tr-index.yaml
   │  ls-metadata validates
   ▼
ls-docgen projects  ──►  docs/reference/ + docs/tr-dependencies/
   │  make docs-check asserts committed == projected
   ▼
generated pages are the authoritative per-TR contract
```

A TR climbs the support ladder (Raw → Tracked → Implemented → Recommended)
*across* both flows; that lifecycle and its per-rung gates are documented in
[`TR_LIFECYCLE.md`](TR_LIFECYCLE.md).

---

## Order safety (why the order path is different)

The order class is the one place where a bug is a real, irreversible market
action rather than a stale read. So `post_order` is deliberately **no-retry**
and routes through the deduplicator and a kill switch, and the design is
governed by an explicit safety asymmetry — a false "already done" is harmless,
a false "safe to retry" causes an irreversible second fill, so every order guard
fails toward *not-safe*. The full vocabulary (double fill, ambiguous order
outcome, order reconciliation, inverted cancel risk, account-flat assertion)
lives in [`CONCEPTS.md`](CONCEPTS.md); the deferral rationale is
[ADR 0008](docs/adr/0008-defer-order-runtime-until-safety-package-is-complete.md).

---

## Design decisions

Architecture-shaping decisions are recorded as ADRs under
[`docs/adr/`](docs/adr/) — among them the maintained-SDK-surface stance (0001),
dependency-class ownership with facet routing (0002), complete-tracking /
selective-implementation (0004), and the order-runtime deferral (0008). Read the
relevant ADR before changing the boundary it establishes.
