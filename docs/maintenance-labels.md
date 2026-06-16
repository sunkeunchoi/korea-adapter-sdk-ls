# Maintenance Work Queue Labels

GitHub Issues are the **Maintenance Work Queue**. These labels make accepted SDK maintenance and expansion work items filterable without turning labels into a second issue schema.

## Required label dimensions

Every accepted work item issue should have one label from each required dimension.

### Queue type

- `queue:maintenance` — the issue changes behavior already owned by the Maintained SDK Surface.
- `queue:expansion` — the issue asks maintainers to start owning additional SDK behavior.

### Source

- `source:api-drift` — accepted from an API Drift Tracker finding.
- `source:spec-doc` — accepted from a Specification Document Tracker finding.
- `source:manual` — accepted from a Manual Maintenance Input.

### Dependency class

- `class:standalone`
- `class:market-session`
- `class:paginated`
- `class:account`
- `class:orders`
- `class:realtime`
- `class:paper-incompatible`
- `class:cross-cutting`

### Support state

- `support:tracked`
- `support:implemented`
- `support:recommended`

### Verification

- `gate:change-scoped` — the issue must name and pass the selected Change-Scoped Gate.

## Optional labels

- `baseline:promotion-needed` — completing the work may require a separate Baseline Promotion decision.
- `evidence:needed` — the work affects a Recommended TR claim or promotes an Implemented TR to Recommended.

## Avoid for now

Do not add priority labels yet. Tracker-emitted work already carries Support-Aware Severity, and a separate priority vocabulary will likely drift from that model.
