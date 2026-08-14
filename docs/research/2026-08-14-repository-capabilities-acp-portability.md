# Repository engineering capabilities and ACP portability

**Research date:** 2026-08-14  
**Decision scope:** what must be preserved before this repository removes its current Compound Engineering-facing files and replaces them with an Orca-oriented, agent-independent engineering layer. This is a migration-obligation assessment, not a deletion or implementation plan.

## Verdict

The repository can make its engineering workflows ACP-portable, but **ACP is only one layer of the replacement**. The clean boundary is:

1. repository-owned, host-neutral capability contracts and prompt/knowledge assets;
2. existing deterministic executors (Rust CLIs, `make` targets, scripts, queue and evidence files), optionally exposed as narrow MCP tools;
3. a Rust ACP proxy/conductor for explicit workflow activation, prompt/context mediation, session workflow state, permission forwarding, progress, cancellation and failure handling; and
4. an Orca/host adapter for process launch, worktree selection, credentials, user interaction and fresh worker processes.

ACP does not standardize skills, plugin installation, system-prompt injection, arbitrary internal tool interception, worktree creation, durable middleware state or a subagent lifecycle. A proxy can rewrite `session/prompt` and observe ACP messages, but it cannot reliably see or govern tools an underlying agent runs directly. MCP can expose deterministic operations, but it cannot intercept prompts or orchestrate the agent. Therefore neither “one ACP proxy” nor “turn every skill into an MCP tool” is a complete representation.

Use the **Rust SDK** for the first portability experiment. Its pinned 2.0.0 source contains first-class proxy traits, proxy chaining and the conductor. The pinned TypeScript 1.3.0 SDK exposes agent/client builders and protocol-version routing, but no equivalent proxy/conductor API; choosing it would mean rebuilding lifecycle and chain semantics first.

DeepSeek Harness is not required to preserve any capability inventoried here. It should remain optional until a separate experiment demonstrates a material benefit under the same capability contracts. Adding Cordis now would create a second plugin/state/permission boundary without closing ACP's actual gaps: host-side workers, worktrees, durable resume, approval authority and direct-tool visibility.

Retirement is safe only after every repository-owned row below has a replacement with equivalent authority, state, evidence and human gates. Removing repository files will **not** uninstall the globally cached EveryInc plugin or remove the global Codex compatibility instructions; those are separate operator-level cleanup obligations.

## Reproducible source snapshot

| Source | Pinned revision/version | What it establishes |
|---|---|---|
| This repository | [`53e59c92f6bb3454d4c1e9a619e71000802a2a14`](https://github.com/sunkeunchoi/korea-adapter-sdk-ls/tree/53e59c92f6bb3454d4c1e9a619e71000802a2a14) | The tracked inventory and repository contracts assessed here. |
| EveryInc Compound Engineering | [`421a33781b86ada0f4e7599792cf45a0363ae9ef`](https://github.com/EveryInc/compound-engineering-plugin/tree/421a33781b86ada0f4e7599792cf45a0363ae9ef), manifest 3.21.4 | Current 33-skill multi-host package and its adapters. |
| Matt Pocock skills | [`8b78b531ab965735c5dc74f6f7a219e1e37326df`](https://github.com/mattpocock/skills/tree/8b78b531ab965735c5dc74f6f7a219e1e37326df) | Current upstream content used only for comparison; the local lock does not record an upstream revision. |
| ACP protocol | [`16879d6217e5a213099e540b1ddc2088fdcbfe35`](https://github.com/agentclientprotocol/agent-client-protocol/tree/16879d6217e5a213099e540b1ddc2088fdcbfe35) | Stable client/agent messages, capabilities, sessions, permissions and extension rules. |
| ACP Rust SDK | [`7d8291d42236023c683bfc52f13d27746cda59ea`](https://github.com/agentclientprotocol/rust-sdk/tree/7d8291d42236023c683bfc52f13d27746cda59ea), crates 2.0.0, schema 1.6.0 | Proxy/conductor implementation and feature stability. |
| ACP TypeScript SDK | [`01010146a731212fbbb677d6055e0b7bf183b288`](https://github.com/agentclientprotocol/typescript-sdk/tree/01010146a731212fbbb677d6055e0b7bf183b288), package 1.3.0 | Agent/client APIs and the absence of a first-class proxy/conductor surface. |

The repository has 36 tracked `.agents/skills/*/SKILL.md` files: 26 entries in `skills-lock.json` sourced from `mattpocock/skills`, plus 10 repository-native LS workflows. The lock records a source URL, path and content hash, but not an upstream commit. At the pinned Matt revision, 24 local files match byte-for-byte; `domain-modeling` and `writing-great-skills` do not. The content hashes identify the installed corpus, but the pinned current Matt commit must not be misrepresented as its installation revision.

## What the current stack actually is

The current surface is several independent layers, not one Compound Engineering runtime:

| Layer / exact paths | Origin | Runtime and state | Migration obligation |
|---|---|---|---|
| `.agents/skills/*` and `skills-lock.json` | 26 Matt Pocock skills; 10 repo-native LS skills | Markdown protocols interpreted by the active coding-agent host; skill-local scripts/references supplement them. | Normalize each repository-owned capability and preserve the precise behavior below. Do not assume an ACP connection makes Markdown skills portable. |
| `.claude/skills/*` | Repository host adapter | 25 tracked symlinks into `.agents/skills`; no independent behavior. | Replace with the Orca/ACP invocation adapter, then remove only after every alias has a reachable equivalent. |
| `.claude/agents/decommission-row-auditor.md`, `.claude/agents/tr-promoter.md` | Repository host adapter | Fresh-context Claude worker definitions used by sweep coordinators. | Replace with a host-neutral worker role plus an Orca/ACP process-spawn and typed-result contract. ACP has no standard subagent-spawn method. |
| `.compound-engineering/config.local.example.yaml` | EveryInc-shaped repo configuration | Example settings for Codex work delegation, product signals, output formats and promotion behavior. Local variants are ignored. | Disposition each setting into the new manifest/Orca config or explicitly declare it unsupported. Do not silently delete potentially used local settings. |
| `.compound-engineering/runs/` | Repo-native skills borrowing an EveryInc path convention | Ignored resumable ledgers for `promote-trs` and migration audits. | Define a versioned durable state schema and resume/import rule before renaming or retiring the path. The directory name is not evidence that EveryInc executes these workflows. |
| `AGENTS.md`, `CLAUDE.md`, `ARCHITECTURE.md`, `TR_LIFECYCLE.md`, `USER_GUIDE.md`, `README.md` | Repository instruction/configuration | `CLAUDE.md` imports `AGENTS.md`; the others route operators and agents to lifecycle recipes, gates and sources of truth. | Rewrite references only after replacement entry points exist. Preserve lifecycle, source-of-truth and gate rules as repository policy, not prompt folklore. |
| `docs/agents/issue-tracker.md`, `docs/agents/triage-labels.md` | Repository-native collaboration contract | GitHub issues, operation maps, dependency/frontier rules, claiming and resolution comments. External mutations use `gh`. | Keep as explicit capability/authority contracts. A proxy may route them, but GitHub writes need scoped tools and auditable operator policy. |
| `queue/items.jsonl`, `lab-next`, `make next` | Repository-native deterministic scheduler | The queue is the sole staging location. `lab-next add/done/supersede` owns mutation; reporting reconciles KRX window and resumable sequence state. | Keep the data and CLI authoritative. Expose narrow commands/tools; do not reimplement queue logic in prompts or proxy state. |
| `scripts/gate-run.sh`, `.gate-run/`, Makefile gate targets | Repository-native deterministic gate | Resumable ordered gates, tree fingerprint and local state; includes docs, workspace, policy, lane, adapter, script and TODO checks. | Keep as executor/evidence substrate. The workflow layer invokes and reports it; it must not infer success from prose. |
| `docs/solutions/**`, `CONCEPTS.md`, `CONTEXT.md`, ADRs, plans, generated reference docs, metadata/evidence ledgers | Repository-native knowledge | Durable domain language, prior solutions, sources of truth and evidence. | Preserve as indexed context with provenance and bounded retrieval. These are knowledge assets, not plugins. |
| `/Users/mini/.codex/AGENTS.md` compatibility block and `/Users/mini/.codex/plugins/cache/compound-engineering-plugin/compound-engineering/3.21.4` | Global Codex/EveryInc installation, not tracked by this repository | Global tool-name translations and all 33 EveryInc skills remain available to the host independently of this branch. | Separate operator cleanup. Repository deletion cannot remove or prove absence of global behavior. |

The repository's policy is visible in permanent source: [`AGENTS.md`](https://github.com/sunkeunchoi/korea-adapter-sdk-ls/blob/53e59c92f6bb3454d4c1e9a619e71000802a2a14/AGENTS.md) names the queue, gates, TR lifecycle, live-smoke boundary and normalized baselines; [`docs/agents/issue-tracker.md`](https://github.com/sunkeunchoi/korea-adapter-sdk-ls/blob/53e59c92f6bb3454d4c1e9a619e71000802a2a14/docs/agents/issue-tracker.md) defines the GitHub coordination protocol. These policies remain authoritative regardless of the host used to read them.

## Migration-obligation matrix: Matt Pocock-derived skills

All 26 are repository-tracked capabilities today. “Prompt” below means a host-neutral capability asset, not that side effects may be left to unconstrained prose.

| Capability | Behavior, authority and evidence to preserve | Best-fit representation |
|---|---|---|
| `ask-matt` | Route an ambiguous request to the appropriate capability without mutating state. | Manifest router + prompt asset. |
| `wait-what` | Re-explain confusing material at a more useful level; no side effects. | Prompt asset. |
| `teach` | Produce an interactive explanation and check understanding. | Prompt asset + ACP elicitation for check-ins. |
| `grilling`, `grill-me`, `grill-with-docs` | Iterative adversarial questioning; document variant reads/writes an artifact only after user decisions. Human answers are state. | Prompt assets + explicit ACP elicitation + versioned artifact writer. |
| `domain-modeling` | Sharpen vocabulary and optionally update `CONTEXT.md`/ADRs with user-approved decisions. | Prompt/knowledge asset + bounded file writer; approval checkpoint. |
| `codebase-design`, `improve-codebase-architecture` | Analyze deep-module seams; the latter may edit code after a selected opportunity. Evidence is source analysis, diff and gates. | Prompt asset for analysis; normal implementation executor for mutations. |
| `prototype` | Build a deliberately disposable experiment to answer one design question; user decides disposition. | Workflow state machine + isolated worktree executor + elicitation. |
| `to-questionnaire` | Convert incomplete requirements into questions and wait for answers. | Prompt asset + typed ACP elicitation. |
| `diagnosing-bugs` | Evidence-first diagnosis loop; may implement only when the invoking contract authorizes it. Preserve hypotheses, tests and failure evidence. | Prompt/workflow asset + deterministic test tools. |
| `implement` | Execute an accepted plan, edit scoped files, verify and hand off. | Workflow contract + worktree/terminal tools; not an MCP method named “implement.” |
| `tdd` | Red-green-refactor sequence with test evidence at each transition. | Workflow state machine + deterministic test executor. |
| `resolving-merge-conflicts` | Inspect and resolve an existing conflict without destructive broad resets; preserve user changes and verification. | High-authority workflow + git tools + explicit scope. |
| `code-review` | Run independent Standards and Spec reviews and report findings; currently expects parallel fresh reviewers. | Two worker-role assets + host worker orchestration + typed findings. |
| `research` | Read primary sources, write one durable Markdown note, and report provenance. | Research role asset + web/repo tools + bounded writer. |
| `setup-matt-pocock-skills` | Inspect/install/update the Matt corpus and lock state. This is a repository mutation and upstream-network operation. | Separate package-management command; exclude from ordinary session middleware. |
| `triage` | Classify work and mutate issue labels/status according to repo policy. | GitHub tool capability + explicit external-write authority. |
| `to-spec` | Convert accepted decisions into a durable specification. | Prompt asset + bounded artifact writer. |
| `to-tickets` | Decompose a spec into dependency-aware issue operations; external writes require confirmation/authority. | Prompt asset + GitHub operation tool. |
| `wayfinder` | Navigate operation maps, claim one frontier ticket, maintain resolution evidence and serialize map updates. | Host-neutral coordination protocol + GitHub tools; durable issue state remains source of truth. |
| `handoff` | Serialize/resume session continuity without relying on hidden chat history. | Versioned handoff schema + bounded artifact/messages. |
| `writing-for-agents`, `writing-great-skills` | Author instructions/skills with activation and safety contracts; changes alter future agent behavior. | Authoring prompt assets + schema/lint checks + review gate. |
| `wizard` | Generate a human-run script for steps the agent cannot perform, especially credentials/UI setup. | Script generator + explicit secret boundary; never proxy-held secrets. |

The local lock file, rather than the current Matt repository head, is the installed-content authority: [`skills-lock.json`](https://github.com/sunkeunchoi/korea-adapter-sdk-ls/blob/53e59c92f6bb3454d4c1e9a619e71000802a2a14/skills-lock.json).

## Migration-obligation matrix: repository-native LS workflows

These ten workflows are the retirement blockers with the highest fidelity requirement. They encode domain safety, lifecycle transitions and evidence semantics that a generic EveryInc or ACP layer does not know.

| Capability | State, side effects, tools and human gate | Required replacement contract |
|---|---|---|
| `track-tr` | One raw REST TR; writes metadata/index, projects (never hand-authors) its normalized baseline, runs offline docs/tests and commits. It must stop as HELD when prerequisites are incomplete and must not author callable Rust. | Typed Raw→Tracked state machine; exact touched-path allowlist; normalizer/gate executors; machine-readable outcome and commit evidence. |
| `track-realtime-tr` | WebSocket sibling of `track-tr`; records realtime facets and projected baseline, no implementation. | Separate realtime contract, not a flag hidden in a generic tracker; same outcome/evidence rules. |
| `implement-tr` | One non-order Tracked TR; writes callable Rust, policy registration, metadata and a smoke. A real LS **paper** live smoke with named lane credentials and non-empty deserialization gates Implemented; no recommendation evidence is written. | Typed Tracked→Implemented contract; secret-safe live executor; capability/window preflight; outcomes `IMPLEMENTED`, `PENDING`, `DROPPED`, `HELD`; exact redaction and gates. |
| `implement-realtime-tr` | Writes the realtime quartet/policy/row/smoke and gates on fresh connect→subscribe→unsubscribe; WebSocket registration differs from REST. | Dedicated realtime executor/contract with lifecycle cleanup and row-evidence handling. |
| `implement-order-tr` | High-risk real paper order path: no retry, dedup, kill switch, redaction and reconciliation. Requires attended PTY, explicit paper environment/opt-in, fresh nonce and flat teardown assertions. | Keep as an explicitly invoked, human-attended command/workflow. ACP permission UX supplements but cannot replace nonce, environment, reconciliation or teardown safeguards. Never auto-route from a generic “implement” prompt. |
| `promote-tr` | One Implemented TR; noninteractive/state-driven when its live paper gate is available; captures credential-free Focused Evidence, flips Recommended, regenerates docs, gates and commits. Otherwise HELD. | Typed Implemented→Recommended state machine; evidence schema; live executor; idempotent resume and scoped commit. |
| `promote-trs` | Repository-wide sweep; discovers candidates, dispatches serial fresh `tr-promoter` workers because files overlap, stores resumable ledgers under `.compound-engineering/runs`, may use credentials, writes evidence/docs, opens/merges/synchronizes a PR. | Durable coordinator with per-item typed results, serialized mutation lane, explicit GitHub-write/merge authority, crash resume and a host worker adapter. ACP alone has no worker contract. |
| `run-strategy-turn` | Executes one pre-authored ORB lever through Phase-A diagnosis, build/fingerprint check, guarded flip and KEEP-rule verdict; appends every reading to `TRIALS`; requires human before/after judgment and adapter gates. | Deterministic governed command remains authority; workflow routes inputs, preserves append-only evidence and elicits the human verdict. |
| `audit-row` | Fresh-context audit of one carried/discard ledger row; applies behavioral/knowledge/discard evidence bars; writes one credential-free record and exactly one typed verdict. Behavioral audit may require a live smoke. It does not edit the roll-up ledger. | Worker-role asset + per-row input/output schema + scoped record writer + optional secret-safe executor. |
| `audit-carried-rows` | Resumable sweep of all carried/discard rows; knowledge/discard audits may run concurrently, behavioral audits are throttled; serial roll-up can change dispositions and escalation/report artifacts. | Durable DAG/coordinator, concurrency classes, typed row verdicts, serial roll-up, resume/import of old run ledgers, gate and audit trail. |

The current source protocols are pinned under [`.agents/skills`](https://github.com/sunkeunchoi/korea-adapter-sdk-ls/tree/53e59c92f6bb3454d4c1e9a619e71000802a2a14/.agents/skills). Their referenced CLIs, metadata, baselines and evidence files are independent project assets and should remain authoritative rather than being translated into free-form proxy logic.

## EveryInc Compound Engineering and global-host behavior

EveryInc's current package is multi-host, but its portability mechanism is **adapter generation around skill text**, not ACP middleware. The manifest exposes a skills directory; OpenCode registers generated commands and skill paths; Pi returns skill paths; the converter rewrites Claude-oriented agents, commands, tools, paths, hooks and MCP configuration for other hosts. See the pinned [Codex plugin manifest](https://github.com/EveryInc/compound-engineering-plugin/blob/421a33781b86ada0f4e7599792cf45a0363ae9ef/.codex-plugin/plugin.json), [OpenCode adapter](https://github.com/EveryInc/compound-engineering-plugin/blob/421a33781b86ada0f4e7599792cf45a0363ae9ef/.opencode/plugins/compound-engineering.js), [Claude parser](https://github.com/EveryInc/compound-engineering-plugin/blob/421a33781b86ada0f4e7599792cf45a0363ae9ef/src/parsers/claude.ts), [Codex converter](https://github.com/EveryInc/compound-engineering-plugin/blob/421a33781b86ada0f4e7599792cf45a0363ae9ef/src/converters/claude-to-codex.ts) and [content transformations](https://github.com/EveryInc/compound-engineering-plugin/blob/421a33781b86ada0f4e7599792cf45a0363ae9ef/src/utils/codex-content.ts).

The globally installed 3.21.4 package exposes 33 skills:

`ce-babysit-pr`, `ce-brainstorm`, `ce-code-review`, `ce-commit-push-pr`, `ce-commit`, `ce-compound-refresh`, `ce-compound`, `ce-debug`, `ce-doc-review`, `ce-dogfood`, `ce-explain`, `ce-handoff`, `ce-ideate`, `ce-optimize`, `ce-plan`, `ce-polish`, `ce-pov`, `ce-product-pulse`, `ce-promote`, `ce-proof`, `ce-prototype`, `ce-resolve-pr-feedback`, `ce-retune`, `ce-riffrec-feedback-analysis`, `ce-setup`, `ce-simplify-code`, `ce-strategy`, `ce-sweep`, `ce-test-browser`, `ce-test-xcode`, `ce-work`, `ce-worktree`, and `lfg`.

These are **not repository-owned migration blockers** unless the new Orca experience promises equivalent global UX. They are still an operator cleanup obligation if “remove current Compound Engineering” means eliminating it from the active Codex environment. The relevant capability families are:

| Global family | Skills | If parity is desired |
|---|---|---|
| Framing/planning | `ce-strategy`, `ce-ideate`, `ce-brainstorm`, `ce-plan`, `ce-pov`, `ce-doc-review`, `ce-prototype` | Portable prompt/role assets and decision checkpoints. |
| Work/debug/review | `ce-work`, `ce-debug`, `ce-code-review`, `ce-simplify-code`, `ce-resolve-pr-feedback`, `ce-optimize`, `ce-retune` | Workflow contracts, deterministic tools, fresh reviewers and eval evidence. |
| Git/shipping | `ce-commit`, `ce-commit-push-pr`, `ce-worktree`, `lfg`, `ce-babysit-pr` | Host/worktree and GitHub adapters with explicit push/merge authority, CI monitoring and cancellation. |
| Knowledge/continuity | `ce-compound`, `ce-compound-refresh`, `ce-explain`, `ce-handoff`, `ce-setup` | Versioned knowledge, handoff and health schemas. |
| Product/external systems | `ce-product-pulse`, `ce-sweep`, `ce-proof`, `ce-promote`, `ce-riffrec-feedback-analysis` | Optional connector-specific capabilities with separate credentials and external-write policies. |
| UI/platform QA | `ce-dogfood`, `ce-polish`, `ce-test-browser`, `ce-test-xcode` | Optional browser/Xcode host adapters; irrelevant to LS workflow retirement unless explicitly adopted. |

EveryInc's own portability guidance usefully separates outcome, hard protocol, workflow, context and adapters, and tells authors to express capabilities before host tools; that is consistent with the normalized representation recommended here ([portable skill authoring](https://github.com/EveryInc/compound-engineering-plugin/blob/421a33781b86ada0f4e7599792cf45a0363ae9ef/docs/solutions/skill-design/portable-agent-skill-authoring.md)). It does not supply the missing ACP runtime semantics.

## ACP control and interception matrix

ACP's base architecture is a bidirectional JSON-RPC connection between an editor/client and an agent process. Sessions own context/history/state; the client supplies a working directory and optional MCP servers. Capabilities are negotiated during initialization and unsupported fields are omitted ([architecture](https://github.com/agentclientprotocol/agent-client-protocol/blob/16879d6217e5a213099e540b1ddc2088fdcbfe35/docs/get-started/architecture.mdx), [initialization](https://github.com/agentclientprotocol/agent-client-protocol/blob/16879d6217e5a213099e540b1ddc2088fdcbfe35/docs/protocol/v1/initialization.mdx), [session setup](https://github.com/agentclientprotocol/agent-client-protocol/blob/16879d6217e5a213099e540b1ddc2088fdcbfe35/docs/protocol/v1/session-setup.mdx)).

| Concern | What an ACP proxy can do | Boundary / migration consequence |
|---|---|---|
| Initialization | Intercept capability negotiation and add a namespaced `_meta` or custom underscore method/capability. | Must not advertise support the chain cannot honor. Custom workflow activation is not portable to clients that do not know the extension; retain an explicit text/command fallback. |
| Session setup | Observe/transform `session/new`, load/resume/fork parameters and map ACP session IDs to workflow state. | Persistence belongs to the implementing agent/proxy; conductor does not define a durable store. Resume/fork/import semantics must be designed and versioned. |
| Prompt/context | Rewrite `session/prompt` content, route explicit workflow invocations and inject user-context messages. | Stable ACP has no standard system-prompt setter. Rewriting every ordinary “implement” prompt would be surprising and unsafe; activation should be explicit. |
| Tool calls/results | Observe `session/update` tool-call reports, including optional raw input/output, and transform/suppress what reaches the client. | The underlying agent executes its tools. Reporting is not guaranteed to expose every direct subprocess/API call, and raw fields are optional. A proxy is not a complete tool policy or audit boundary ([tool calls](https://github.com/agentclientprotocol/agent-client-protocol/blob/16879d6217e5a213099e540b1ddc2088fdcbfe35/docs/protocol/v1/tool-calls.mdx)). |
| Permissions/HITL | Intercept and forward ACP permission requests; enforce a stricter policy; use ACP elicitation for typed choices/forms. | A base agent may have inherited OS access or tools that never request ACP permission. The proxy must preserve user authority and fail closed; it cannot replace order nonces, attended PTYs or reconciliation ([elicitation](https://github.com/agentclientprotocol/agent-client-protocol/blob/16879d6217e5a213099e540b1ddc2088fdcbfe35/docs/protocol/v1/elicitation.mdx)). |
| Filesystem/terminal | Observe client-side ACP filesystem and terminal requests when the agent uses them. | The session `cwd` selects a root but is not a sandbox. Direct process filesystem access is outside those messages. Orca/host must create/select the worktree and constrain process permissions ([filesystem](https://github.com/agentclientprotocol/agent-client-protocol/blob/16879d6217e5a213099e540b1ddc2088fdcbfe35/docs/protocol/v1/file-system.mdx), [terminals](https://github.com/agentclientprotocol/agent-client-protocol/blob/16879d6217e5a213099e540b1ddc2088fdcbfe35/docs/protocol/v1/terminals.mdx)). |
| Subagents/workers | Route to separately launched ACP agents if the conductor/host knows them; session fork can clone context where supported. | The schema has no standard spawn-worker/task/result lifecycle. The fork proposal mentions summaries and possible subagents, but it is not a worker orchestration contract ([session fork RFD](https://github.com/agentclientprotocol/agent-client-protocol/blob/16879d6217e5a213099e540b1ddc2088fdcbfe35/docs/rfds/session-fork.mdx)). |
| MCP | Supply deterministic domain operations to the agent or, with experimental SDK support, bridge MCP through a proxy. | MCP is a tool surface, not prompt interception, orchestration, approval policy or a skill package format. Keep mutations narrow and validated. |
| Cancellation | Forward `$/cancel_request`/session cancellation and cancel the proxy's own downstream/worker activities. | Cancellation support is capability-dependent; the coordinator must define cleanup, partial evidence and resumability ([cancellation](https://github.com/agentclientprotocol/agent-client-protocol/blob/16879d6217e5a213099e540b1ddc2088fdcbfe35/docs/protocol/v1/cancellation.mdx)). |
| Failure | Detect downstream closure/error and return a structured terminal workflow state. | The reference conductor terminates the whole chain when one component fails. Durable state and safe recovery must live above it. |
| Chaining | Compose multiple typed proxies and present one agent to the client. | Ordering changes semantics; each proxy adds capability-negotiation, failure, cancellation and observability responsibilities. Begin with one CE/LS workflow proxy, not a proxy per verb. |

Custom underscore methods and `_meta` are the sanctioned extension route ([extensibility](https://github.com/agentclientprotocol/agent-client-protocol/blob/16879d6217e5a213099e540b1ddc2088fdcbfe35/docs/protocol/v1/extensibility.mdx)). The proxy-chain design is an RFD rather than part of the stable core protocol, although the Rust SDK already implements its own proxy/conductor architecture ([proxy-chain RFD](https://github.com/agentclientprotocol/agent-client-protocol/blob/16879d6217e5a213099e540b1ddc2088fdcbfe35/docs/rfds/proxy-chains.mdx)). Consumers should therefore pin SDK versions and treat P/ACP/conductor interoperability as implementation-specific until standardized.

## Rust versus TypeScript

| Criterion | Rust SDK 2.0.0 | TypeScript SDK 1.3.0 |
|---|---|---|
| Agent/client | Supported. | Supported with versioned agent/client builders. |
| First-class proxy | Stable v1 proxy trait/builder; typed interception in both directions. v2 proxy work remains unstable. | No exported proxy abstraction found in the pinned package. Custom request/notification APIs exist, so a hand-built dual endpoint is possible. |
| Conductor/chain | Dedicated `agent-client-protocol-conductor` crate/binary; launches proxy chain and base agent, presenting one agent to the client. | No conductor export. `AgentProtocolRouter` selects protocol version; it is not an agent/proxy chain. |
| MCP through ACP | Feature-gated/unstable; conductor's earlier built-in bridge has been removed in favor of a polyfill pattern. | MCP configs are represented at the agent/client layer; no equivalent conductor bridge. |
| Failure/lifecycle burden | Reference process lifecycle and chain behavior already exist; one component failure tears down the chain. | Must build process ownership, bidirectional correlation, capability aggregation, cancellation and teardown. |
| Recommendation | **Use for the proof of portability.** Pin exact crates/features and isolate unstable MCP/v2 features. | Reconsider after proxy/conductor parity lands or when the team accepts owning the missing runtime. |

Primary implementation references: Rust [proxy concepts](https://github.com/agentclientprotocol/rust-sdk/blob/7d8291d42236023c683bfc52f13d27746cda59ea/src/agent-client-protocol/src/concepts/proxies.rs), [conductor README](https://github.com/agentclientprotocol/rust-sdk/blob/7d8291d42236023c683bfc52f13d27746cda59ea/src/agent-client-protocol-conductor/README.md), [conductor implementation](https://github.com/agentclientprotocol/rust-sdk/blob/7d8291d42236023c683bfc52f13d27746cda59ea/src/agent-client-protocol-conductor/src/conductor.rs), [core crate features](https://github.com/agentclientprotocol/rust-sdk/blob/7d8291d42236023c683bfc52f13d27746cda59ea/src/agent-client-protocol/Cargo.toml); TypeScript [package exports](https://github.com/agentclientprotocol/typescript-sdk/blob/01010146a731212fbbb677d6055e0b7bf183b288/package.json), [method surface](https://github.com/agentclientprotocol/typescript-sdk/blob/01010146a731212fbbb677d6055e0b7bf183b288/src/acp.ts), and [protocol-version router](https://github.com/agentclientprotocol/typescript-sdk/blob/01010146a731212fbbb677d6055e0b7bf183b288/src/protocol-router.ts).

## Recommended representation boundaries

| Representation | Owns | Must not own |
|---|---|---|
| Capability manifest | ID/version, activation, inputs/outcomes, risk/authority, human gates, required tools/capabilities, state schema, touched paths, evidence/gates and worker roles. | Host-specific command names, model prose or executable secrets. |
| Prompt/knowledge assets | Outcome spine, hard protocol, workflow guidance, repository vocabulary and source routing. | Side-effect enforcement, durable state or claims that a command succeeded. |
| Deterministic command/MCP executors | Queue mutations, normalization, tests/gates, evidence validation, live-smoke preflight and narrowly scoped external operations. | Open-ended orchestration or user-decision substitution. |
| ACP workflow proxy | Explicit activation, capability checks, prompt/context mediation, per-session workflow transitions, permission/elicitation forwarding, progress, cancellation and structured failure. | OS sandboxing, direct-tool omniscience, worktree creation, secrets, or implicit high-risk invocation. |
| Orca/host adapter | Launch/monitor conductor and base agent, choose isolated worktree/cwd, pass constrained environment, provide UI/HITL, spawn fresh workers and collect typed results. | Repository domain rules duplicated from manifests or deterministic executors. |

A single workflow proxy is enough for a first experiment. Split proxies only when a capability has an independently useful, order-insensitive protocol boundary. Research→plan→work→review is an internal workflow state machine, not four mandatory network hops.

## Retirement gates

The existing repository-facing surface is not ready to remove until all of these are evidenced:

1. Every one of the 36 local capabilities has a disposition and, where retained, a versioned manifest plus host-neutral assets.
2. The 25 Claude symlink aliases and two fresh-agent definitions have reachable Orca equivalents, including typed worker results, context isolation, concurrency rules and cancellation.
3. The two `.compound-engineering/runs` consumers have a durable state schema, crash-resume/idempotency tests and an import/disposition rule for existing ignored runs.
4. Queue scheduling, gate execution, metadata/baseline projection, evidence ledgers and `docs/solutions` remain authoritative deterministic assets; parity tests prove the new layer invokes rather than reinterprets them.
5. Permission and elicitation behavior is capability-gated and fail-closed. Live credentials never enter prompts or durable proxy state. Order workflows remain explicitly attended and preserve nonce, no-retry, kill-switch, redaction, reconciliation and flat teardown.
6. Worktree/cwd/process environment ownership is documented and enforced by the Orca adapter. A session path is not treated as a filesystem sandbox.
7. GitHub issue/PR/merge operations have explicit external-write scopes, machine-readable outcomes and recovery after partial success.
8. Cancellation and component failure produce a durable terminal/paused state with cleanup and exact resume instructions.
9. A cross-agent evaluation corpus proves activation, outcome, touched-file, evidence, gate and refusal/HELD parity for at least one supported base agent before adding more agents.
10. Repository instructions and example config are rewritten only after replacement commands are usable. Global EveryInc plugin/cache and global compatibility instructions are separately inventoried and removed by the operator if desired.

The smallest meaningful research-to-build handoff is therefore a **non-mutating or low-risk vertical slice**, not the full loop: define one normalized capability contract (for example `research` or `track-tr` in dry-run/validation mode), launch one unchanged ACP base agent behind the pinned Rust conductor and one proxy, explicitly activate the capability, verify session state/permission/cancellation behavior, and compare its artifact/evidence against the existing protocol. `implement-order-tr`, sweep/merge automation and DeepSeek/Cordis should remain outside that first experiment.
