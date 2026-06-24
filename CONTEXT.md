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

**Upstream Change Signal**:
An observed LS API, documentation, or gateway behavior change that may need SDK maintenance review.
_Avoid_: tracker finding, SDK task, generated patch

**Tracker Finding**:
A severity-classified observation emitted by a **Change Tracker** before it becomes SDK work.
_Avoid_: task, patch, generated change

**Manual Maintenance Input**:
A maintainer-observed LS API, documentation, or gateway behavior change that may need review but was not emitted by a **Change Tracker**.
_Avoid_: tracker finding, ad hoc task, generated change

**Support-Aware Severity**:
The classification of a tracker finding according to both upstream change risk and whether the affected TR is tracked, implemented, or recommended.
_Avoid_: raw diff severity, generator failure, CI severity

**Staged Snapshot**:
A captured upstream LS API or documentation artifact that can be normalized and diffed before any project baseline is updated.
_Avoid_: live fetch, generated source, pinned baseline

**Reviewed Baseline**:
The accepted upstream API or documentation state that a **Change Tracker** compares new **Staged Snapshots** against after human review.
_Avoid_: generated source, fixture, live snapshot

**Baseline Promotion**:
The reviewed act of replacing or extending a **Reviewed Baseline** after deciding the staged upstream state should become the future comparison point.
_Avoid_: SDK patch, automatic update, tracker write

**Structural API Shape**:
The normalized request and response structure of an LS transaction request, including block identity, field position, field name, field attributes, protocol, and endpoint facts.
_Avoid_: sample payload, generated struct, leaf path

**Clean Self-Diff**:
The property that comparing a **Reviewed Baseline** against its own source yields zero **Tracker Findings**; it proves the baseline is internally consistent and deterministically projected, not that every drift path the trackers gate on is covered.
_Avoid_: regression test, idempotent build, no-op diff

**Provisionality**:
A recorded caveat that a specific **Reviewed Baseline** facet is not yet trustworthy ground truth — for example a field type derived from a fallback rather than a live authoritative source — carried per facet in the provisionality ledger until a clean source resolves it.
_Avoid_: bug, tech debt, open question

**SDK Maintenance Work Item**:
A reviewed unit of work derived from a **Tracker Finding** that asks an agent or maintainer to update, create, or remove SDK behavior.
_Avoid_: generated diff, regeneration task, tracker result

**SDK Expansion Work Item**:
A reviewed unit of work that asks maintainers to start owning additional SDK behavior that is not already part of the **Maintained SDK Surface**.
_Avoid_: migration task, generated port, backlog item

**Tracked-Only Expansion Work Item**:
An **SDK Expansion Work Item** that adds TR maintenance ownership without adding callable SDK behavior.
_Avoid_: implemented expansion, generated coverage, full SDK port

**Implemented Expansion Work Item**:
An **SDK Expansion Work Item** that adds callable SDK behavior for one or more TRs.
_Avoid_: tracked-only expansion, generated coverage, full SDK port

**Implementation Lane**:
A planning boundary for sequencing TR implementation work by shared safety, transport, or evidence prerequisites. It is not a support tier and does not by itself make a TR tracked, implemented, or recommended.
_Avoid_: support tier, blanket campaign, backlog bucket

**Mixed Expansion Wave**:
An SDK expansion effort that may bring raw TRs to tracked and then implemented within one coordinated batch while still applying each TR's rung-specific gate. It is a planning scope, not a shortcut around tracking, implementation, smoke, or recommendation requirements.
_Avoid_: bulk implementation, generated port, support shortcut

**Completed Maintenance Work Item**:
An **SDK Maintenance Work Item** whose affected maintained artifacts have been updated and whose selected **Change-Scoped Gate** has passed.
_Avoid_: merged patch, closed issue, generated update

**Maintenance Work Queue**:
The durable collection of accepted SDK work items waiting for completion or already completed with review history.
_Avoid_: tracker output, sprint board, generated task list

**Maintenance Review Decision**:
The accepted, deferred, or rejected decision made after reviewing a **Tracker Finding** or **Manual Maintenance Input**.
_Avoid_: severity, tracker status, test result

**Maintenance Flow**:
The reviewed path from an upstream change signal to accepted SDK maintenance work, verification, and any necessary baseline update.
_Avoid_: tracker flow, migration flow, automatic regeneration

**Foundation Complete**:
The state where ordinary SDK maintenance or expansion can move through review, queued work, artifact updates, verification, and any needed baseline decision without inventing a new process. It is proven in two stages: first that the queue plumbing is self-consistent, then that the flow carries weight on a real SDK-facing work item. It is claimed only after the second stage.
_Avoid_: all TRs ported, tracker perfection, migration done

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

**Credentialed Live Smoke**:
A narrow LS gateway check that uses real credentials to prove an existing implemented slice still reaches live LS behavior. It is smaller than broad integration coverage and may become **Focused Evidence** only when its target, inputs, and result are recorded.
_Avoid_: real gate test, live integration suite, production test

**Paper Live Smoke**:
A **Credentialed Live Smoke** that targets LS paper credentials only. It is the default live check before any real or order-capable evidence is considered.
_Avoid_: simulation smoke, real-money smoke, sandbox test

**WebSocket Lifecycle Smoke**:
A **Paper Live Smoke** for realtime TRs that proves the paper WebSocket endpoint can connect, subscribe, and unsubscribe for a TR. A delivered row can strengthen evidence, but lifecycle reachability is the implementation gate because many realtime feeds are session- or event-dependent.
_Avoid_: row-delivery proof, realtime recommendation evidence, REST smoke

**Bootstrap Tool**:
A temporary migration utility used to create initial project data from old specs or documents.
_Avoid_: permanent tracker, maintained tooling, SDK runtime

**Migration Source**:
An old repository, document, or artifact used to seed the new maintained SDK architecture without becoming a permanent dependency.
_Avoid_: upstream dependency, source of truth, generated owner

**Decommissioned Migration Source**:
A former **Migration Source** whose gateway, TR, runtime, and operational knowledge has been extracted or deliberately rejected so the maintained SDK no longer needs it even as read-only reference material.
_Avoid_: archived repo, read-only source, obsolete active SDK

**Decommission Audit Verdict**:
The durable outcome of reconciling the migration-source extraction ledger against retained audit records. It is not the same as a test-run count or other execution-check output.
_Avoid_: validator count, test count, check output

**Decommission Closeout Record**:
Documentation that narrates decommission operations after the authorizing decommission posture is already established. It improves historical legibility but does not authorize or enforce the decommission.
_Avoid_: authorizing verdict, enforcement gate, decommission precondition

**Archived Historical Copy**:
A read-only retained copy of a former migration source kept for provenance and history after decommission. It is not a source of truth for maintained SDK behavior or ordinary maintenance.
_Avoid_: canonical historical copy, source of truth, reference repo

**Decommission Follow-up Gap**:
A tracked improvement opportunity around a decommission guard or closeout process that is not an unmet decommission precondition. It may describe a known residual gap without weakening the current decommission posture.
_Avoid_: failed gate, unresolved precondition, untrusted decommission

**Instrument Domain**:
The LS market or product area a TR belongs to, such as domestic stock, futures/options, overseas stock, or overseas futures.
_Avoid_: ownership module, generated category

**Standalone TR**:
A TR with no prerequisite beyond OAuth authentication.
_Avoid_: simple TR, caller-supplied identifier TR, easy endpoint

**Caller-Supplied Identifier**:
A request identifier normally provided by the SDK caller, such as an instrument, account, market, country, or currency code.
_Avoid_: dependency, standalone prerequisite

**Prerequisite Producer TR**:
A TR whose response supplies a required value for another TR's request, such as an order number used by a modify, cancel, inquiry, or reconciliation flow.
_Avoid_: weak lookup TR, identifier source, nice-to-have producer

**Read-Only TR**:
A TR whose successful call observes broker, market, account, or reference state without placing, modifying, canceling, registering, deregistering, or otherwise creating broker-side state.
_Avoid_: safe TR, simple TR, query TR

**Side-Effectful TR**:
A TR whose successful call can create, change, remove, or subscribe to broker-side state, including order operations and registration lifecycles.
_Avoid_: read TR, ordinary implemented TR, harmless control

**Side-Effect-Adjacent Realtime Feed**:
A realtime feed that observes broker-side lifecycle state such as order receipt, execution, correction, cancellation, or rejection without itself submitting, modifying, or canceling an order. It requires stricter evidence wording than market quote feeds, but it is not the same as REST order runtime.
_Avoid_: order runtime, harmless realtime feed, order TR

## Relationships

- The **Maintained SDK Surface** is the source of truth for SDK behavior.
- An **API Drift Tracker** and a **Specification Document Tracker** are both **Change Trackers**.
- **SDK Reference Docs** are not generated directly from upstream LS documentation.
- **TR Dependency Docs** are derived from maintained metadata, not raw tracker output.
- An **Upstream Change Signal** is reviewed before it is treated as SDK work.
- A **Change Tracker** emits advisory **Tracker Findings**.
- A **Manual Maintenance Input** is not a **Tracker Finding**.
- A **Tracker Finding** uses **Support-Aware Severity**.
- A **Change Tracker** compares **Staged Snapshots** to **Reviewed Baselines**.
- A **Baseline Promotion** is a separate review act from ordinary SDK maintenance unless the work item is specifically about tracker baseline state.
- An **API Drift Tracker** normalizes upstream API data into **Structural API Shape** before diffing.
- A **Maintenance Review Decision** determines whether a **Tracker Finding** or **Manual Maintenance Input** becomes SDK work.
- A **Tracker Finding** can be promoted into an **SDK Maintenance Work Item**.
- A **SDK Expansion Work Item** is a decision to own additional SDK behavior, not a reaction to behavior already owned by the **Maintained SDK Surface**.
- A **Tracked-Only Expansion Work Item** expands maintenance ownership but does not make a TR an **Implemented TR**.
- An **Implemented Expansion Work Item** makes a TR an **Implemented TR** but not automatically a **Recommended TR**.
- An **Implementation Lane** groups candidate TRs before support-rung promotion; it does not override the gate for any individual TR.
- A **Mixed Expansion Wave** can contain both **Tracked-Only Expansion Work Items** and **Implemented Expansion Work Items**, but each TR still climbs the support ladder one rung at a time.
- Accepted **SDK Maintenance Work Items** and **SDK Expansion Work Items** live in the **Maintenance Work Queue**.
- A **Completed Maintenance Work Item** is not complete from code changes alone.
- The **Maintenance Flow** begins with an upstream change signal and does not change the **Maintained SDK Surface** until reviewed work is accepted.
- **Foundation Complete** means the **Maintenance Flow** can be repeated without creating a new process for each work item, and is claimed only after a real SDK-facing work item has proven the flow, not from queue plumbing alone.
- An **SDK Maintenance Work Item** changes the **Maintained SDK Surface** only after review.
- A **Dependency Class** owns SDK code organization.
- **Facet Metadata** routes tests, evidence, documentation, and operator scheduling.
- **TR Maintenance Metadata** records one owning **Dependency Class** and multiple **Facet Metadata** values.
- **TR Maintenance Metadata** records strong **Prerequisite Producer TR** relationships separately from order identity fields.
- A **TR Metadata Index** supports fast selection but does not replace the per-TR **TR Maintenance Metadata** record.
- A **Tracked TR** does not have to be an **Implemented TR**.
- An **Implemented TR** does not have to be a **Recommended TR**.
- A **Change-Scoped Gate** is the default verification path for an **SDK Maintenance Work Item**.
- A **Full Baseline** is not the default verification path for ordinary SDK maintenance.
- **Focused Evidence** replaces old certification vocabulary for implemented or recommended behavior.
- **Credentialed Live Smoke** can support **Focused Evidence** but is not automatically **Focused Evidence**.
- **Paper Live Smoke** is a **Credentialed Live Smoke**.
- A **WebSocket Lifecycle Smoke** can make a realtime TR an **Implemented TR** without proving a non-empty pushed row.
- **Focused Evidence** is required for **Recommended TR** claims, not for every **Tracked TR** or every **Completed Maintenance Work Item**.
- A **Bootstrap Tool** is not part of the permanent maintenance architecture.
- A **Migration Source** does not remain a dependency after migration.
- A **Decommissioned Migration Source** is stronger than a read-only or obsolete source: maintainers should not need to consult it for ordinary SDK maintenance or expansion.
- A **Decommission Audit Verdict** is a durable ledger-audit result, not the number of validator tests that happened to pass in a run.
- A **Decommission Closeout Record** is historical documentation, not an authorization gate.
- An **Archived Historical Copy** may be retained for provenance and history, but it does not make a **Decommissioned Migration Source** ordinary reference material again.
- A **Decommission Follow-up Gap** is tracked work, not evidence that the decommission gate failed.
- An **Instrument Domain** is **Facet Metadata**, not a code ownership boundary.
- A **Standalone TR** does not include TRs that need market/session timing, account state, date handling, pagination, order coupling, or WebSocket lifecycle.
- A **Caller-Supplied Identifier** does not make a TR standalone by itself.
- A **Prerequisite Producer TR** is limited to strong cross-TR prerequisites; a TR that merely helps discover a **Caller-Supplied Identifier** is not a prerequisite producer.
- A **Side-Effectful TR** is not treated as a **Read-Only TR** merely because it uses REST or returns a response body.
- A **Side-Effect-Adjacent Realtime Feed** can be included in a realtime implementation lane without implementing broker-state-mutating REST order runtime.
