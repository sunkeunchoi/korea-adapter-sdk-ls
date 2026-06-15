# Four-crate workspace split by tooling concern

The maintained SDK workspace splits into four crates by tooling concern rather than by dependency class: `ls-core` (transport-agnostic runtime — auth, config, transport, rate buckets, errors, serde wire-compat), `ls-sdk` (the public facade, with dependency classes as modules), `ls-metadata` (TR metadata types, routing index, validator, and the change-scoped planner), and `ls-trackers` (API Drift and Specification Document trackers). This keeps maintenance tooling out of a user's dependency tree, while the Change-Scoped Gate routes tests by metadata facet rather than crate boundary — so crate-per-dependency-class would be boilerplate that buys nothing.

The first vertical slice builds `ls-core`, `ls-sdk`, and `ls-metadata`; `ls-trackers` is deferred until the tracker skeleton lands. A dev-only `ls-sdk-test-support` crate (`publish = false`) holds shared test mocks and is a workspace member but not one of the four shippable target crates.
