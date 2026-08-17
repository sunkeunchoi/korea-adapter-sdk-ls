# Repository Engineering Package

This directory is an inert repository declaration. It does not install, activate,
execute, publish, retire, or transfer authority. Current skills, Claude aliases,
workers, and ignored run-state consumers remain legacy-authoritative.

Human-authored inputs are `package.toml`, `discovery-policy.toml`, and
`migration-ledger.toml`. Rust types are the schema authority. The generated
`schemas/v0/`, `schema-registry.json`, `conformance/v0/`, `package.lock.json`,
`generated-set.json`, and reference documentation are projections; do not edit
them by hand.

For an added, renamed, moved, or removed obligation:

1. Run `make repository-engineering-check` and inspect its stable candidate ID,
   locator, digest, and remediation class.
2. Author or update the ledger row and choose its disposition in review.
3. Run `cargo run --locked -q -p ls-repository-engineering -- generate`.
4. Run `make repository-engineering-check` again without writes.

Generation never creates or overwrites a Migration Ledger disposition.
The root workspace is pinned to Rust 1.96.0 by `rust-toolchain.toml`.
