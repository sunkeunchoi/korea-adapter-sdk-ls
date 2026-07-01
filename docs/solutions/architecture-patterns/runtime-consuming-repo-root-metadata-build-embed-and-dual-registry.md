---
title: "Consuming repo-root metadata from a shippable runtime crate: build.rs embed + a dual type registry, and the InBlock preflight-descent pitfall"
date: 2026-07-01
category: architecture-patterns
module: crates/ls-core, crates/ls-metadata
problem_type: architecture_pattern
tags:
  - build-rs
  - metadata
  - preflight
  - serde
  - ls-core
  - error-resilience-gate
---

## Context

The error-resilience gate (plan 2026-07-01-004) needed `ls-core` — the shippable,
transport-agnostic runtime — to read two things that live at the **workspace
root**, outside the crate: `metadata/error-catalog.yaml` (gateway-code
explanations) and `metadata/constraints/<tr>.yaml` (per-TR request field-constraint
schemas, which drive a preflight validator run inside `dispatch_once`).

Two structural constraints collided:

1. **`ls-core` ships to consumers with no filesystem access**, so it cannot
   `fs::read` the metadata at runtime — the data must be embedded at build time.
2. **`ls-core` cannot depend on `ls-metadata` at runtime.** `ls-metadata` is a
   *dev-dependency* only (it exists to validate + project metadata offline), and
   pulling it into the shipped library is off-limits. So the parsed metadata types
   `ls-metadata` owns (e.g. `ConstraintSchema`) are unavailable to runtime code.

A naive `include_str!("../../../metadata/…")` would work in this checkout but is a
non-publish-safe escape out of the crate directory, and it does not scale to a
*set* of files (the constraint schemas are a growing, globbed set).

## Guidance

**1. Embed repo-root data with a `build.rs` that reads it and emits string-literal
consts into `OUT_DIR`** — not `include_str!` with a `../../../` escape.

`build.rs` runs with `CARGO_MANIFEST_DIR` = the crate dir, so it can reach the
workspace root (`../../metadata`), read each file, and generate a small module of
`&'static str` literals. A `src/embedded.rs` `include!`s the generated file. The
crate then reads embedded data with zero filesystem access, and the file set can be
**globbed** (dynamic), which `include_str!` cannot do.

```rust
// build.rs (crates/ls-core)
let metadata_dir = Path::new(&manifest_dir).join("..").join("..").join("metadata");
println!("cargo:rerun-if-changed={}", metadata_dir.join("constraints").display());
// ...read each *.yaml, then emit:
writeln!(generated, "pub(crate) const ERROR_CATALOG_YAML: &str = {catalog:?};").unwrap();
// {body:?} produces a valid escaped Rust string literal — no OUT_DIR file layout,
// no include_str path fragility.
generated.push_str("pub(crate) const CONSTRAINT_FILES: &[(&str, &str)] = &[\n");
for (tr, body) in &constraint_files { writeln!(generated, "    ({tr:?}, {body:?}),").unwrap(); }
```

```rust
// src/embedded.rs
include!(concat!(env!("OUT_DIR"), "/embedded_metadata.rs"));
```

Parse once behind a `OnceLock`; `expect`/`panic` on a malformed embedded file is
acceptable here because the data is **committed and CI-gated** (a bad file cannot
reach `main`) — this matches the repo's existing embed-and-parse convention.

**2. When the runtime can't share the offline crate's types, deliberately
duplicate the type and let the shared file format be the contract.** `ls-core` owns
its own `ConstraintSchema` (in `preflight.rs`) and `ls-metadata` owns a
structurally-identical copy (in `schema.rs`). Both deserialize the *same*
`metadata/constraints/*.yaml`. Document the duplication on **both** sides, and
protect against drift with **two independent tests that parse the same committed
files through each crate's copy** — a shape-breaking edit to either struct then
fails loudly in CI:

- `ls-core`: `embedded_constraint_schemas_all_parse` (registry parses every file).
- `ls-metadata` (via `ls-core` dev-dep): `constraint_grounding.rs` parses + grounds.

The duplication is the price of the crate boundary; the shared YAML + the two
parse-tests are what keep the copies honest.

**3. Thread new metadata through the validated report, don't re-read files
downstream.** `ls-docgen` projects only from `ValidationReport`. To surface the new
artifacts (constraints, error-coverage, error-catalog) in docs, the *validator*
parses them into new `ValidationReport` fields and docgen reads the report — never
the files. One parse, one source of truth.

## Why This Matters

- **Publish-safety + scale.** The `build.rs` embed keeps the crate self-contained
  at ship time and handles a globbed, growing file set. The `../../../ include_str`
  alternative is both an escape out of the crate and static (one hard-coded path
  per file).
- **The dual registry is a *feature*, not debt** — but only if it is guarded.
  Undocumented, it is a silent drift trap: the two `ConstraintSchema` copies could
  diverge (`type` vs `field_type` rename, a `#[serde(default)]` on one side only)
  and the same YAML would parse differently in runtime vs offline. The two
  parse-tests are load-bearing; a cross-model reviewer specifically probed this and
  it held because both tests exercise the real committed file.
- **Panic-on-committed-embedded-data is a considered choice.** It fails at
  build/CI, not in a consumer — *provided* the CI gate actually parses the file.
  The trade-off surfaces when the set grows: an eager "parse all files in one
  `OnceLock`" amplifies one bad file into a panic for *every* TR's dispatch. If the
  set grows past a couple of files, switch to lazy per-TR parsing so a bad schema
  only disables its own TR.

## When to Apply

- A **shippable** crate (no filesystem at runtime) must consume data that lives
  outside its own directory (workspace-root config, schemas, catalogs).
- The natural owner of the parsed types is a crate the shippable one **cannot
  depend on at runtime** (a dev-dep, or a heavier sibling).
- The data is a **set** of files that grows over time (globbed), not a single fixed
  file.

If the crate *can* depend on the type-owning crate at runtime, share the type
instead — the duplication is justified only by the crate-boundary constraint.

## Examples

### Pitfall: preflight must descend into the `{TR}InBlock` wrapper

The single sharpest bug in this work: LS request bodies serialize as
`{"<TR>InBlock": { field: value, ... }}`, but the first preflight implementation
validated the **top-level** object. `serde_json::to_value(req)` returned
`{"t8412InBlock": {...}}`, so `value.get("shcode")` was `None` and **every valid
SDK call false-rejected** with `Invalid { field: "shcode", reason: "required..." }`
— caught only because the SDK's own `chart` wiremock tests went red.

The fix is a `locate_fields_object` that descends into the wrapper before
validating:

```rust
// Validate the block the schema's fields actually live in, not the wrapper.
fn locate_fields_object<'a>(value: &'a Value, schema: &ConstraintSchema) -> &'a Value {
    if let Some(obj) = value.as_object() {
        if schema.fields.iter().any(|f| obj.contains_key(&f.name)) { return value; }
        for nested in obj.values() {
            if let Some(inner) = nested.as_object() {
                if schema.fields.iter().any(|f| inner.contains_key(&f.name)) { return nested; }
            }
        }
    }
    value
}
```

**Lesson:** any validation/inspection that runs against a serialized request at the
`dispatch_once` seam must account for the `{TR}InBlock` nesting — the caller-facing
fields are one level down. (Known residual: this descends into a *single* block;
a future multi-block request TR, e.g. `CSPAT00701`, needs per-field location across
blocks — fix it with that schema in hand, not speculatively.)

### Permissive-by-default preflight (avoid false-rejects)

Preflight blocks only `type`/`required` (grounded structurally against the
normalized baseline). Value-class bounds (`enum`/`range`/`format`) carry
`confirmed: false` and stay **permissive** until a live differential probe confirms
them — because a false-reject silently breaks a caller's valid request with no
detector. A schema may also declare a wire-required field *caller-optional* (the
permissive direction), which is exactly why the exemplar marks `sdate`/`edate`
`required: false`: they are range filters the gateway defaults, and requiring them
would reject a legitimate empty-range query.
