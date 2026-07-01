# TR Transition Lifecycle

**Audience:** maintainers and contributors. This document explains how a
transaction request (**TR**) climbs the support ladder, and the concrete
**requirement/gate** that each transition must clear. It is the narrative
companion to the frozen recipes under [`.agents/skills/`](.agents/skills/), which
own the mechanical step-by-step.

**What this document does not own:**

- **The recipe steps themselves** — [`.agents/skills/track-tr/`](.agents/skills/track-tr/),
  [`implement-tr/`](.agents/skills/implement-tr/),
  [`implement-realtime-tr/`](.agents/skills/implement-realtime-tr/),
  [`promote-tr/`](.agents/skills/promote-tr/), and their order variants. Read a
  recipe's `SKILL.md` before running it.
- **Term-precise definitions** — [`CONCEPTS.md`](CONCEPTS.md) is the authoritative
  glossary for every bolded term here.
- **The runtime/crate structure** — [`ARCHITECTURE.md`](ARCHITECTURE.md).

---

## The ladder

A TR is one LS-securities API operation, identified by a transaction code (e.g.
`t1102`, `CSPAQ12200`). Every TR occupies exactly one rung of a four-state
ladder. **Each rung up is a deliberate, separately-gated promotion** — a TR never
skips a rung, and being on one rung makes no claim about the next.

```
Raw ──track──► Tracked ──implement──► Implemented ──promote──► Recommended
                 │                        │                        │
            metadata +               callable Rust +          Focused Evidence +
            pinned baseline          Paper Live Smoke         recommendation block
```

A crucial invariant runs through the whole ladder: **Implemented ≠ Recommended.**
A TR can be fully wired and tested without being recommended for use. This is the
project's selective-by-design stance ([ADR 0004](docs/adr/0004-complete-tracking-selective-sdk-implementation.md)):
tracking is broad, implementation is chosen, recommendation is earned.

---

## Raw

**State.** The TR's wire shape exists in the captured OpenAPI universe (the raw
capture at `crates/ls-trackers/baselines/api-drift/raw/ls-openapi-full.json` and
`code-set.json`) but it has **no** committed `metadata/trs/<tr>.yaml` and **no**
normalized baseline. It is not yet observed for drift, and the `implement-tr`
recipe cannot derive structs for it.

Raw is *below* the maintained surface — a TR here is known to exist upstream and
nothing more.

---

## Tracked

The lowest **maintained** rung: committed metadata and a maintained baseline, but
no callable code. The TR is observed for drift — nothing else.

**Requirement to reach Tracked (Raw → Tracked):**

1. Author `metadata/trs/<tr>.yaml` — the per-TR maintenance record: owning
   dependency class, facets (session scope, instrument domain, rate bucket,
   pagination, caller-supplied identifiers), and any provisional facets.
2. Add its `tr-index.yaml` entry.
3. Project the normalized baseline with `make api-drift-renormalize`. The
   baseline is **projected from the raw capture, never hand-authored** — wire
   field names, types, and array-vs-single shapes come from it, not guesswork.

**Driving recipe:** [`.agents/skills/track-tr/`](.agents/skills/track-tr/)
(`track-realtime-tr` for WebSocket TRs). The recipe is state-driven: it TRACKS
when the raw shape is complete, or **HELDs** the TR with a recorded reason when
the raw shape is incomplete and a baseline cannot be pinned without live probing.

Bringing a raw TR to Tracked is a genuine prerequisite step — it authors metadata
and pins a baseline, and flips **no** support state past `tracked`.

---

## Implemented

The middle rung: hand-authored callable Rust that has passed a **Paper Live
Smoke**. An Implemented TR is callable but carries **no recommendation and no
recorded evidence** — explicitly *not* endorsed for production use.

**Requirement to reach Implemented (Tracked → Implemented):**

1. Author callable Rust in the owning dependency-class module of `ls-sdk`
   (request/response structs bound to the baseline's block/field shape, plus the
   facade method). Request numeric fields must serialize as JSON numbers
   (`string_as_number` / `string_as_decimal`) or the gateway returns `IGW40011`.
2. Register the TR's `{TR}_POLICY` runtime policy. A **REST** policy registers in
   **both** cross-check lists; a **WebSocket** policy (`owner_class: realtime`)
   registers in the crosscheck list **only**, never the REST-only list.
3. Pass a **Paper Live Smoke** (the gate — see below) and flip
   `support.implemented: true` in metadata.
4. Regenerate docs (`make docs`) and keep the full gate green.

**The gate — Paper Live Smoke.** A credential-gated integration test that hits
the real LS **paper** gateway with real credentials to prove the TR is genuinely
callable: it constructs the request, returns a success code, yields a **non-empty**
result, and deserializes. A smoke that returns an *empty* result leaves the TR
callable-but-unconfirmed (**Pending**), not Implemented. The non-empty-assert-
before-record rule means a wrong guess dispositions to Pending, never a false flip.

**Realtime variant.** For `realtime`/WebSocket TRs the gate is instead **lifecycle
(Transport) reachability** — a clean connect → subscribe → unsubscribe. A pushed
row that does or does not arrive is bonus, not the gate, because many realtime
feeds are session- or event-dependent. Row *contents* stay provisional until a
separate FrameDecode pass. Where the subscribe path is fire-and-forget and the
gateway emits no observable rejection for an invalid `tr_cd`, the claim recorded is
**connection-reachable-only** (the connection works, not that the specific channel
is individually reachable) — calibrated by the **WebSocket negative control**.

**Driving recipe:** [`.agents/skills/implement-tr/`](.agents/skills/implement-tr/)
— [`implement-realtime-tr/`](.agents/skills/implement-realtime-tr/) for WebSocket,
[`implement-order-tr/`](.agents/skills/implement-order-tr/) for order TRs (whose
gate is the *Guarded paper order* matrix, not a read smoke).

---

## Recommended

The top rung: an Implemented TR additionally cleared for production use.

**Requirement to reach Recommended (Implemented → Recommended):**

1. Run the TR's Paper Live Smoke and capture its result as **Focused Evidence** —
   a recorded, credential-free proof backing the recommendation. (A smoke run
   *gates implementation*; it only *becomes* Focused Evidence when the TR is
   deliberately promoted.)
2. Add the recommendation block to the TR's metadata and regenerate docs.
3. Stay within the **90-day evidence-freshness backstop** (`make freshness-check`),
   which flags any Recommended TR whose evidence has gone stale.

Promotion to Recommended is a separate, deliberate act beyond Implemented — never
automatic.

**Driving recipe:** [`.agents/skills/promote-tr/`](.agents/skills/promote-tr/)
(and `promote-trs` for batches).

---

## Off-ladder states

Not every transition succeeds cleanly. These states record *why* a TR did not
advance, so nothing is silently treated as confirmed:

- **Pending** — the Paper Live Smoke ran but did not open the Implemented gate:
  callable yet shape-unconfirmed (empty result), or blocked by an unresolved
  input or an environmental gateway rejection. A Pending TR ships without flipping
  and keeps its provisional ledger rows. It is *expected to flip* on a recovering
  re-run.

- **Paper-incompatible** — a TR the paper gateway will **never** serve, recorded
  as a permanent non-flip (the `paper_incompatible` facet) rather than a
  re-runnable Pending. Distinct from Pending: it never flips on paper.

- **Finish-the-flip** — a Tracked TR whose callable Rust is **already fully wired**
  (carrier, `{TR}_POLICY` in the crosscheck list(s), a smoke + Makefile target) but
  still sits at `implemented: false` because its certifying smoke has not yet
  returned a non-empty in-window witness. The remaining work is **metadata +
  docgen only** — re-run the existing smoke and flip on a non-empty result. Grep
  for an existing carrier/smoke before treating a candidate as needing a fresh
  `implement-tr` pass, or you collide on already-defined symbols.

### The Provisionality Ledger

[`metadata/PROVISIONALITY-LEDGER.md`](metadata/PROVISIONALITY-LEDGER.md) records,
per TR, which authored facets are still **provisional** (best-effort, pending live
confirmation) and what must be re-verified before promotion. Rows retire as a TR
is implemented and each facet is confirmed against a live call; a Pending or HELD
TR keeps its rows so nothing is silently trusted.

---

## Live counts

The number of Tracked / Implemented / Recommended TRs changes as this ladder is
worked, so this document deliberately states **no counts**. For current numbers
read the generated pages under [`docs/reference/`](docs/reference/) and
[`docs/tr-dependencies/`](docs/tr-dependencies/), which are projected from
metadata by `make docs`.
