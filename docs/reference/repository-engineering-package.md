# Repository Engineering Package

This page is generated from the inert, reviewed package declaration. It grants no runtime, credential, activation, authority-transfer, retirement, or publication authority.

- Schema version: `v0`
- Package lock identity: `sha256:82f8661328e0551c328114f4b7f3e5669fcccb805535ea7fddb3c27c516e5f7c`
- Activation eligibility: `none`
- Declared capability contracts: `1`
- Declared worker roles: `1`
- Active capability contracts: `0`
- Active worker roles: `0`
- Reviewed migration rows: `78`
- Planned migration rows: `2`

Declaration, migration planning, implementation, certification, parity, activation, authority, and retirement are independent states. Canonical lifecycle statements below come only from validated typed fields.

## Declared contracts

### Capability `audit-carried-rows`

Non-normative purpose text (not identity-bound and not lifecycle evidence): Audits every carried or discard row and produces credential-free per-row records plus a roll-up gate.

Canonical typed state: declaration `declared`, implementation `implemented`, certification `uncertified`, authority `legacy`, retirement `not_started`; activation: inactive; executor: present; scenarios: 1.

Evidence boundary: legacy evidence `available_validated` (1 artifact(s), 1 artifact set(s), 26 set member(s)); successor implementation evidence `available_validated`; parity `unproved`; certification `uncertified`; legacy evidence satisfies successor: `false`.

External source requirements:

| Requirement | Status | Locator | Digest | Unavailable outcome | Worker verdict |
|---|---|---|---|---|---|
| korea-broker-sdk-ls | unavailable_unproved | absent | absent | held | unverifiable |

Identity-bearing semantic provenance:

| Field groups | Status | Source basis |
|---|---|---|
| coordination_semantics, inputs, outcomes, worker_roles | legacy_observed | `{"source_kind":"knowledge_reference","path":".agents/skills/audit-carried-rows/SKILL.md"}; {"source_kind":"knowledge_reference","path":".agents/skills/audit-row/SKILL.md"}` |
| autonomy, evidence_obligations, human_gates, safety_overlays, touched_paths, legacy_authority_dependencies | legacy_observed | `{"source_kind":"knowledge_reference","path":".agents/skills/audit-carried-rows/SKILL.md"}; {"source_kind":"knowledge_reference","path":".agents/skills/audit-carried-rows/references/record-format.md"}; {"source_kind":"migration_ledger_rows","logical_ids":["capability--audit-row","run-state-consumer--audit-carried-rows"]}` |
| credential_boundary | successor_requirement | `{"source_kind":"successor_decision","decision_id":"credential-boundary-v0"}` |
| external_source_requirements[*].purpose | legacy_observed | `{"source_kind":"worker_knowledge_reference","role_id":"decommission-row-auditor","path":".claude/agents/decommission-row-auditor.md"}` |
| external_source_requirements[*].status, external_source_requirements[*].locator, external_source_requirements[*].digest | unavailable_unproved | `{"source_kind":"external_source_requirement","requirement_id":"korea-broker-sdk-ls"}` |
| evidence_status.legacy | legacy_observed | `{"source_kind":"legacy_artifact_set","artifact_set_id":"decommission-audit-record-corpus"}` |
| evidence_status.successor, state, executor, scenario_references | successor_requirement | `{"source_kind":"successor_decision","decision_id":"inert-migration-boundary-v0"}` |

### Worker role `decommission-row-auditor`

Non-normative purpose text (not identity-bound and not lifecycle evidence): A fresh worker audits exactly one manifest row and returns a validated credential-free result.

Canonical typed state: declaration `declared`, implementation `implemented`, certification `uncertified`, authority `legacy`, retirement `not_started`; activation: inactive; terminal correlation: required.

Identity-bearing semantic provenance:

| Field groups | Status | Source basis |
|---|---|---|
| assignment_fields, result_fields, fresh_context_required, concurrency, result_validation_required | legacy_observed | `{"source_kind":"knowledge_reference","path":".claude/agents/decommission-row-auditor.md"}; {"source_kind":"knowledge_reference","path":".agents/skills/audit-row/SKILL.md"}; {"source_kind":"knowledge_reference","path":".agents/skills/audit-carried-rows/references/record-format.md"}` |
| terminal_result_correlation, cancellation_supported, idempotency_key_required | successor_requirement | `{"source_kind":"successor_decision","decision_id":"terminal-correlation-v0"}` |
| state | successor_requirement | `{"source_kind":"successor_decision","decision_id":"inert-migration-boundary-v0"}` |

## Migration ledger

| Logical ID | Source kind | Source locator | Disposition | Migration | Absence reason | Declaration | Implementation | Certification | Authority | Retirement | Replacement |
|---|---|---|---|---|---|---|---|---|---|---|---|
| capability--ask-matt | capability | `.agents/skills/ask-matt` | PORT | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| capability--audit-carried-rows | capability | `.agents/skills/audit-carried-rows` | PORT | planned | parity_not_proven | declared | implemented | uncertified | legacy | not_started | audit-carried-rows |
| capability--audit-row | capability | `.agents/skills/audit-row` | PORT | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| capability--code-review | capability | `.agents/skills/code-review` | PORT | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| capability--codebase-design | capability | `.agents/skills/codebase-design` | PORT | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| capability--diagnosing-bugs | capability | `.agents/skills/diagnosing-bugs` | PORT | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| capability--domain-modeling | capability | `.agents/skills/domain-modeling` | PORT | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| capability--grill-me | capability | `.agents/skills/grill-me` | PORT | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| capability--grill-with-docs | capability | `.agents/skills/grill-with-docs` | PORT | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| capability--grilling | capability | `.agents/skills/grilling` | PORT | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| capability--handoff | capability | `.agents/skills/handoff` | PORT | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| capability--implement | capability | `.agents/skills/implement` | PORT | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| capability--implement-order-tr | capability | `.agents/skills/implement-order-tr` | PORT | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| capability--implement-realtime-tr | capability | `.agents/skills/implement-realtime-tr` | PORT | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| capability--implement-tr | capability | `.agents/skills/implement-tr` | PORT | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| capability--improve-codebase-architecture | capability | `.agents/skills/improve-codebase-architecture` | PORT | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| capability--promote-tr | capability | `.agents/skills/promote-tr` | PORT | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| capability--promote-trs | capability | `.agents/skills/promote-trs` | PORT | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| capability--prototype | capability | `.agents/skills/prototype` | PORT | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| capability--research | capability | `.agents/skills/research` | PORT | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| capability--resolving-merge-conflicts | capability | `.agents/skills/resolving-merge-conflicts` | PORT | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| capability--run-strategy-turn | capability | `.agents/skills/run-strategy-turn` | PORT | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| capability--setup-matt-pocock-skills | capability | `.agents/skills/setup-matt-pocock-skills` | PORT | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| capability--tdd | capability | `.agents/skills/tdd` | PORT | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| capability--teach | capability | `.agents/skills/teach` | PORT | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| capability--to-questionnaire | capability | `.agents/skills/to-questionnaire` | PORT | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| capability--to-spec | capability | `.agents/skills/to-spec` | PORT | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| capability--to-tickets | capability | `.agents/skills/to-tickets` | PORT | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| capability--track-realtime-tr | capability | `.agents/skills/track-realtime-tr` | PORT | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| capability--track-tr | capability | `.agents/skills/track-tr` | PORT | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| capability--triage | capability | `.agents/skills/triage` | PORT | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| capability--wait-what | capability | `.agents/skills/wait-what` | PORT | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| capability--wayfinder | capability | `.agents/skills/wayfinder` | PORT | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| capability--wizard | capability | `.agents/skills/wizard` | PORT | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| capability--writing-for-agents | capability | `.agents/skills/writing-for-agents` | PORT | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| capability--writing-great-skills | capability | `.agents/skills/writing-great-skills` | PORT | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| claude-alias--ask-matt | claude_alias | `.claude/skills/ask-matt` | REPLACE_WITH_EXECUTOR | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| claude-alias--code-review | claude_alias | `.claude/skills/code-review` | REPLACE_WITH_EXECUTOR | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| claude-alias--codebase-design | claude_alias | `.claude/skills/codebase-design` | REPLACE_WITH_EXECUTOR | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| claude-alias--diagnosing-bugs | claude_alias | `.claude/skills/diagnosing-bugs` | REPLACE_WITH_EXECUTOR | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| claude-alias--domain-modeling | claude_alias | `.claude/skills/domain-modeling` | REPLACE_WITH_EXECUTOR | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| claude-alias--grill-me | claude_alias | `.claude/skills/grill-me` | REPLACE_WITH_EXECUTOR | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| claude-alias--grill-with-docs | claude_alias | `.claude/skills/grill-with-docs` | REPLACE_WITH_EXECUTOR | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| claude-alias--grilling | claude_alias | `.claude/skills/grilling` | REPLACE_WITH_EXECUTOR | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| claude-alias--handoff | claude_alias | `.claude/skills/handoff` | REPLACE_WITH_EXECUTOR | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| claude-alias--implement | claude_alias | `.claude/skills/implement` | REPLACE_WITH_EXECUTOR | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| claude-alias--improve-codebase-architecture | claude_alias | `.claude/skills/improve-codebase-architecture` | REPLACE_WITH_EXECUTOR | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| claude-alias--prototype | claude_alias | `.claude/skills/prototype` | REPLACE_WITH_EXECUTOR | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| claude-alias--research | claude_alias | `.claude/skills/research` | REPLACE_WITH_EXECUTOR | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| claude-alias--resolving-merge-conflicts | claude_alias | `.claude/skills/resolving-merge-conflicts` | REPLACE_WITH_EXECUTOR | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| claude-alias--setup-matt-pocock-skills | claude_alias | `.claude/skills/setup-matt-pocock-skills` | REPLACE_WITH_EXECUTOR | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| claude-alias--tdd | claude_alias | `.claude/skills/tdd` | REPLACE_WITH_EXECUTOR | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| claude-alias--teach | claude_alias | `.claude/skills/teach` | REPLACE_WITH_EXECUTOR | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| claude-alias--to-questionnaire | claude_alias | `.claude/skills/to-questionnaire` | REPLACE_WITH_EXECUTOR | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| claude-alias--to-spec | claude_alias | `.claude/skills/to-spec` | REPLACE_WITH_EXECUTOR | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| claude-alias--to-tickets | claude_alias | `.claude/skills/to-tickets` | REPLACE_WITH_EXECUTOR | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| claude-alias--triage | claude_alias | `.claude/skills/triage` | REPLACE_WITH_EXECUTOR | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| claude-alias--wait-what | claude_alias | `.claude/skills/wait-what` | REPLACE_WITH_EXECUTOR | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| claude-alias--wayfinder | claude_alias | `.claude/skills/wayfinder` | REPLACE_WITH_EXECUTOR | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| claude-alias--wizard | claude_alias | `.claude/skills/wizard` | REPLACE_WITH_EXECUTOR | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| claude-alias--writing-for-agents | claude_alias | `.claude/skills/writing-for-agents` | REPLACE_WITH_EXECUTOR | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| global-cleanup--issue-281 | global_cleanup_assumption | `global-cleanup/issue-281` | GLOBAL_CLEANUP | unported | cleanup_not_executed | absent | unported | uncertified | legacy | not_started | absent |
| global-cleanup--issue-285 | global_cleanup_assumption | `global-cleanup/issue-285` | GLOBAL_CLEANUP | unported | cleanup_not_executed | absent | unported | uncertified | legacy | not_started | absent |
| instruction--agents-md | instruction_config | `AGENTS.md` | MERGE | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| instruction--architecture-md | instruction_config | `ARCHITECTURE.md` | MERGE | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| instruction--claude-md | instruction_config | `CLAUDE.md` | MERGE | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| instruction--compound-engineering-config-local-example-yaml | instruction_config | `.compound-engineering/config.local.example.yaml` | MERGE | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| instruction--docs-agents-domain-md | instruction_config | `docs/agents/domain.md` | MERGE | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| instruction--docs-agents-issue-tracker-md | instruction_config | `docs/agents/issue-tracker.md` | MERGE | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| instruction--docs-agents-triage-labels-md | instruction_config | `docs/agents/triage-labels.md` | MERGE | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| instruction--readme-md | instruction_config | `README.md` | MERGE | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| instruction--skills-lock-json | instruction_config | `skills-lock.json` | MERGE | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| instruction--tr-lifecycle-md | instruction_config | `TR_LIFECYCLE.md` | MERGE | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| instruction--user-guide-md | instruction_config | `USER_GUIDE.md` | MERGE | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| run-state-consumer--audit-carried-rows | ignored_state_consumer | `.compound-engineering/runs/audit-carried-rows` | PORT | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| run-state-consumer--promote-trs | ignored_state_consumer | `.compound-engineering/runs/promote-trs` | PORT | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
| worker-role--decommission-row-auditor | worker_role | `.claude/agents/decommission-row-auditor.md` | PORT | planned | parity_not_proven | declared | implemented | uncertified | legacy | not_started | decommission-row-auditor |
| worker-role--tr-promoter | worker_role | `.claude/agents/tr-promoter.md` | PORT | unported | successor_not_implemented | absent | unported | uncertified | legacy | not_started | absent |
