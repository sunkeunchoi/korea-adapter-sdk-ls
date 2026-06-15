# korea-adapter-sdk-ls Context

`korea-adapter-sdk-ls` maintains a Rust SDK for the LS Securities Open API by tracking upstream change and applying reviewed SDK changes.

## Language

**Maintained SDK Surface**:
The Rust SDK surface that maintainers and agents update directly when LS API behavior or documentation changes.
_Avoid_: Generated API Surface, generated stubs, raw bindings

**API Drift Tracker**:
A change tracker that detects LS Open API shape changes such as TR additions, removals, and request or response field changes.
_Avoid_: Spec Drift Review, schema check, generator trigger

**Specification Document Tracker**:
A change tracker that detects documentation changes relevant to SDK behavior, examples, certification, or operator guidance.
_Avoid_: docs generator, documentation scrape, API drift

**SDK Reference Docs**:
User-facing documentation generated from maintained SDK behavior, TR metadata, and verified examples.
_Avoid_: upstream documentation mirror, specification tracker output

**TR Dependency Docs**:
Maintainer-facing and operator-facing documentation that explains TR prerequisites, coupling, venue/session constraints, and support state from maintained metadata.
_Avoid_: generated SDK reference, upstream documentation mirror

**Change Tracker**:
A mechanism that records upstream LS API or documentation changes so SDK maintenance work can be reviewed and assigned.
_Avoid_: code generator, release gate, CI job

**Tracker Finding**:
A severity-classified observation emitted by a **Change Tracker** before it becomes SDK work.
_Avoid_: task, patch, generated change

**Support-Aware Severity**:
The classification of a tracker finding according to both upstream change risk and whether the affected TR is tracked, implemented, or recommended.
_Avoid_: raw diff severity, generator failure, CI severity

**Staged Snapshot**:
A captured upstream LS API or documentation artifact that can be normalized and diffed before any project baseline is updated.
_Avoid_: live fetch, generated source, pinned baseline

**SDK Maintenance Work Item**:
A reviewed unit of work derived from a **Tracker Finding** that asks an agent or maintainer to update, create, or remove SDK behavior.
_Avoid_: generated diff, regeneration task, tracker result

**Dependency Class**:
The primary ownership boundary for SDK behavior, grouping TRs by the prerequisite pattern that maintainers must understand.
_Avoid_: module, generated category, shard

**Facet Metadata**:
The tags attached to a maintained TR for test routing, evidence routing, documentation, and operator scheduling.
_Avoid_: ownership boundary, module hierarchy, support tier

**TR Maintenance Metadata**:
The persisted record for a maintained TR that names its owning dependency class and all debugging, test-routing, evidence-routing, and documentation facets.
_Avoid_: generated inventory, support inventory, certification artifact

**TR Metadata Index**:
The routing summary that points each TR code to its full **TR Maintenance Metadata** record and duplicates only fields needed for fast selection.
_Avoid_: source of truth, generated docs, certification matrix

**Implemented TR**:
A TR whose behavior is present in the maintained Rust SDK.
_Avoid_: tracked TR, documented TR, generated TR

**Tracked TR**:
A TR represented in **TR Maintenance Metadata** so upstream changes can be detected and reasoned about.
_Avoid_: implemented TR, supported TR, certified TR

**Recommended TR**:
An **Implemented TR** with current focused tests or evidence strong enough for user-facing recommendation.
_Avoid_: implemented TR, tracked TR, callable TR

**Change-Scoped Gate**:
The default verification set selected from the changed TRs, owning dependency classes, and facet metadata for a maintenance work item.
_Avoid_: full baseline, release gate, all tests

**Full Baseline**:
The broad verification run across all implemented automated SDK behavior used for release or periodic confidence, not the default for every maintenance work item.
_Avoid_: default test gate, change-scoped gate

**Focused Evidence**:
Targeted proof for a specific implemented behavior, dependency class, or TR selected through maintained metadata.
_Avoid_: TR Certification, Certified TR Coverage, Evidence Accountability

**Bootstrap Tool**:
A temporary migration utility used to create initial project data from old specs or documents.
_Avoid_: permanent tracker, maintained tooling, SDK runtime

**Migration Source**:
An old repository, document, or artifact used to seed the new maintained SDK architecture without becoming a permanent dependency.
_Avoid_: upstream dependency, source of truth, generated owner

**Instrument Domain**:
The LS market or product area a TR belongs to, such as domestic stock, futures/options, overseas stock, or overseas futures.
_Avoid_: ownership module, generated category

**Standalone TR**:
A TR with no prerequisite beyond OAuth authentication.
_Avoid_: simple TR, caller-supplied identifier TR, easy endpoint

**Caller-Supplied Identifier**:
A request identifier normally provided by the SDK caller, such as an instrument, account, market, country, or currency code.
_Avoid_: dependency, standalone prerequisite

## Relationships

- The **Maintained SDK Surface** is the source of truth for SDK behavior.
- An **API Drift Tracker** and a **Specification Document Tracker** are both **Change Trackers**.
- **SDK Reference Docs** are not generated directly from upstream LS documentation.
- **TR Dependency Docs** are derived from maintained metadata, not raw tracker output.
- A **Change Tracker** emits advisory **Tracker Findings**.
- A **Tracker Finding** uses **Support-Aware Severity**.
- A **Change Tracker** compares **Staged Snapshots** to reviewed baselines.
- A **Tracker Finding** can be promoted into an **SDK Maintenance Work Item**.
- An **SDK Maintenance Work Item** changes the **Maintained SDK Surface** only after review.
- A **Dependency Class** owns SDK code organization.
- **Facet Metadata** routes tests, evidence, documentation, and operator scheduling.
- **TR Maintenance Metadata** records one owning **Dependency Class** and multiple **Facet Metadata** values.
- A **TR Metadata Index** supports fast selection but does not replace the per-TR **TR Maintenance Metadata** record.
- A **Tracked TR** does not have to be an **Implemented TR**.
- An **Implemented TR** does not have to be a **Recommended TR**.
- A **Change-Scoped Gate** is the default verification path for an **SDK Maintenance Work Item**.
- A **Full Baseline** is not the default verification path for ordinary SDK maintenance.
- **Focused Evidence** replaces old certification vocabulary for implemented or recommended behavior.
- A **Bootstrap Tool** is not part of the permanent maintenance architecture.
- A **Migration Source** does not remain a dependency after migration.
- An **Instrument Domain** is **Facet Metadata**, not a code ownership boundary.
- A **Standalone TR** does not include TRs that need market/session timing, account state, date handling, pagination, order coupling, or WebSocket lifecycle.
- A **Caller-Supplied Identifier** does not make a TR standalone by itself.
