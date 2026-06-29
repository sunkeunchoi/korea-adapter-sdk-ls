# Refactor baseline — plan 2026-06-29-003 (codebase simplification / module split)

Committed, diffable behavior-preservation anchor for the module-decomposition +
serde-consolidation refactor. Every later wave diffs its count-test values against
**this file**, not an ephemeral PR description (U1 in the plan).

Captured on a clean, gate-green `main` at the start of the refactor (lull confirmed:
no open PRs, raw TR pool exhausted as of 2026-06-29 per MEMORY).

## Count-test invariant (must be byte-identical after every wave; no test edits)

| Count test | Value | Source of truth |
|---|---|---|
| `TRACKED_TRS` length | **307** | `crates/ls-docgen/src/lib.rs:677` (`[&str; 307]`) |
| `reference.len()` | **279** | `crates/ls-docgen/src/lib.rs:1386` |
| `maintained_tr_count` | **307** | `crates/ls-trackers/tests/api_drift.rs:106` + `baselines/api-drift/normalized/manifest.json:3` |
| `banner_trs` list | unchanged literal | `crates/ls-docgen/src/lib.rs:1103` (implemented-not-recommended set) |

Starting gate (all green, recorded pre-move):
`make docs && cargo test && cargo test -p ls-core && make docs-check`
→ `cargo test`: **1182 passed, 199 ignored**; `cargo test -p ls-core`: **122 passed**;
`make docs-check`: no diff.

## Pre-split monolith sizes (the diffable "before")

| File | Lines (before) | Structs / consts |
|---|---|---|
| `crates/ls-sdk/src/market_session/mod.rs` | 11,789 | 474 structs |
| `crates/ls-sdk/src/realtime/frame.rs` | 4,977 | 102 structs (+ shared helpers + ~107 inline tests) |
| `crates/ls-sdk/src/account/mod.rs` | 2,406 | 73 structs |
| `crates/ls-core/src/endpoint_policy.rs` | 5,077 | 297 `_POLICY` consts |

## KTD1 — the re-export rule (load-bearing)

Each moved cluster is brought back into its original module path via
`pub use <submod>::*;` and each submodule starts with `use super::*;`. This keeps
`ls_sdk::market_session::T1102Request` (and every other public path) resolvable, so
callers, the typed regression tests, and the docgen count tests are untouched. Moves
are **pure relocation** — no struct/policy renames, no metadata/baseline edits, no
generated-doc hand-edits.

## Cross-repo scope decision (resolved now, not deferred)

**Out-of-repo consumer drift is OUT of scope.** This is an internal workspace SDK; its
only consumers are the in-repo typed tests (`market_session_tests.rs`,
`account_tests.rs`, `realtime_tests.rs`, `policy_index_crosscheck.rs`), which import
~30+ struct/const names by path and **fail at compile time** if any re-export is
dropped. Per KTD3 the compile gate + count-test invariance IS the agreed coverage; no
`cargo public-api` step is added.

## U2 (Wave 1) TR→family map — `market_session/mod.rs` split

`mod.rs` retains only module docs + the module-level `use` block + the
`MarketSession` struct & its facade `impl` (≈1,158 lines) + `mod`/`pub use`
re-exports. Family names are directional (KTD2; mis-filing is silent + gate-green via
glob re-export). 474 structs → 473 moved + 1 (`MarketSession`) retained; all 129
`impl …Request` blocks moved with their clusters.

- **`quote.rs`** (9 TRs): t1102, t8450, t8407, t1471, t1105, t1104, t1101, t1537, t2301
- **`quote_deriv.rs`** (13 TRs): t2111, t2112, t2106, t8402, t8403, t8434, g3101, g3106, o3105, o3106, o3125, o3126, o3127
- **`investor_flow.rs`** (17 TRs): t1716, t1927, t1941, t1631, t1632, t1633, t1702, t1717, t1665, t1601, t1615, t1640, t1662, t1664, t8463, t8462, t1926
- **`charts.rs`** (14 TRs): t1308, t1621, t2545, t8406, t1449, g3102, g3103, o3104, t8427, t2210, t2424, t8428, t1302, t2216
- **`etf.rs`** (6 TRs): t1901, t1902, t1903, t1904, t1906, t1959
- **`elw.rs`** (12 TRs): t1950, t1956, t1958, t1964, t1969, t1971, t1972, t1974, t9907, t9942, t8431, t1988
- **`masters.rs`** (19 TRs): t8425, t8436, t1531, t1532, t1533, t9905, t8430, t9945, t2522, t8435, t8467, t9943, t9944, g3104, g3190, o3101, o3121, t8455, t8460
- **`reference.rs`** (8 TRs): t8401, t8426, t8433, t3202, t1764, t3102, t3320, t0167
- **`ranking.rs`** (10 TRs): t1638, t1475, t1859, t1826, t1825, t8424, t1511, t1485, t1516, t3521

Family files land at 611–1,776 lines. Three (`investor_flow` 1,776 / `charts` 1,581 /
`masters` 1,661) sit just above the ~1,500 ceiling by genuine single-concept cohesion
(DoD permits this); the rest are well under.

## U6 (Wave 4) serde-attr consolidation — VERDICT: SKIP all three types

Decided on measured evidence from the KTD5 pre-check prototype (not optimism). The
DoD explicitly permits skipping Wave 4 with recorded rationale.

**Attribute inventory (workspace):** `serialize_with = "ls_core::string_as_number"`
×214 (request numeric, serialize-only); `deserialize_with = "ls_core::string_or_number"`
×2,724 (response, deserialize-only); `serialize_with = "ls_core::string_as_decimal"`
×**1** (one F-O futures-price field, `account/capacity.rs`). There are **zero**
`Option<>`-numeric fields using these helpers (the optional case uses a separate
`option_string_or_number`, out of this scope).

**Prototype run (R1-safety + reduction measured):** built `with =` modules in
`ls-core` (`wire_str` = {serialize: serialize_str, deserialize: string_or_number};
`wire_num`/`wire_dec` = {serialize: string_as_*, deserialize: string_or_number}),
converted `paginated/chart.rs` (331 `string_or_number` + 28 `string_as_number`
fields) to `#[serde(with = "ls_core::wire_*")]`, and ran the offline suite.

- **Behavior:** `paginated_tests` 158/158 passed — `with =` is R1-safe (field type
  stays `String`; response default-serialize ≡ `serialize_str`; request structs are
  `Serialize`-only so the bundled deserialize is never emitted). The IGW40011
  direction + tolerant-decode tests passed.
- **Reduction:** attribute-string characters 20,183 → 13,135 (**−35%**) but **line
  count 2,304 → 2,304 (0%)** — the consolidation removes *string characters*, never
  the ~2,400 cited attr *lines* (still exactly one `#[serde(...)]` per field). No
  R1-safe mechanism reduces the line count (a newtype would, but retypes
  `pub field: String` → violates R1 per KTD5; a container default can't set per-field
  coercion).

**Why skip despite passing + a 35% string cut:**
1. It addresses none of the framed cost — the attr-*line* count is irreducible under
   R1; only the per-string length shrinks (cosmetic).
2. It **hides the load-bearing IGW40011 wire direction.** `serialize_with` (request →
   JSON number) vs `deserialize_with` (response → tolerant) names the direction at
   every field; a `with =` module collapses both into one opaque name, reducing
   visibility of the codebase's most important wire invariant (AGENTS.md gotcha #1).
   That is a net **R6/maintainability regression**, not a gain.
3. `string_as_number → wire_num` carries a real R1 edge-risk: any `Request` struct
   that also derives `Deserialize` would silently gain number-tolerant deserialize
   (default String parse → `string_or_number`). chart.rs requests are `Serialize`-only
   so the prototype didn't exercise this, but a workspace-wide rollout would.
4. `string_as_decimal` has a single field — nothing to consolidate.

**Per-type verdict:** `string_or_number` SKIP, `string_as_number` SKIP,
`string_as_decimal` SKIP. The prototype (`wire_*` modules + chart.rs conversion) was
reverted; no behavior or public-API change ships from Wave 4. The explicit
single-direction helpers remain the intended minimal, direction-revealing form.

## Gate sequence (every chunk)

```
make docs && cargo test && cargo test -p ls-core && make docs-check
```
Keep the tree green; never commit on a red gate.
