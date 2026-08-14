# DeepSeek Harness and Cordis architecture verdict

Research date: 2026-08-14 (Asia/Seoul)

## Decision

**DEFER DeepSeek Harness as an optional future ACP-agent backend. Exclude Cordis
from the first Orca Repository Engineering Layer architecture.**

DeepSeek Harness already is ACP-compatible in one precise sense: its
`@deepseek-ai/dsh-acp` package opens an ACP `AgentSideConnection` over stdio and
implements `initialize`, `session/new`, `session/prompt`, `session/cancel`, and
permission requests. It can therefore appear to Orca as an ACP Agent today. It
does **not** implement an ACP Proxy or Conductor, and a Cordis plugin is an
in-process Harness component, not independently an ACP agent or proxy.

The existing ACP bridge is intentionally automation-only and semantically
lossy. It exposes fresh sessions, baseline text prompts, committed assistant
text, one-shot permission choices, and cancellation. It omits session
load/resume/list/fork/close, additional roots, MCP servers, client filesystem and
terminal services, live tool/reasoning/plan/usage updates, and per-session
close. Thus the whole Harness composition can sit behind a thin ACP process
adapter, but preserving its full Cordis runtime semantics over ACP would require
a substantial stateful gateway, not a thin wrapper.

Cordis is a capable agent-runtime framework: typed services and events,
reversible plugin lifecycles, prompt and tool middleware, durable event-sourced
sessions, model adapters, approval policy, and interchangeable subagent
providers. Those benefits matter when Harness itself owns the agent loop. They
do not provide the missing cross-agent middleware needed to add repository
engineering behavior to unchanged Codex, Claude, OpenCode, or other ACP Agents.
Putting Cordis in the first architecture would add a second orchestration,
session, permission, and plugin model without satisfying a necessary capability
that Orca plus ACP cannot supply cleanly.

The adoption criteria therefore resolve as follows:

| Criterion | Finding |
| --- | --- |
| **ADOPT** only for a necessary or materially superior first-architecture capability | Not met. Cordis is materially richer *inside Harness*, but it is not an agent-independent ACP proxy/conductor. |
| **DEFER** for plausible later value behind a stable seam | Met. Harness can already run as an ACP Agent and may later be a useful optional local/provider-neutral backend. |
| **REJECT** if coupling or maturity destroys relevant value | Not met absolutely. Model coupling is low and the ACP Agent path is real, so a future backend remains plausible. The current developer-preview maturity does rule it out of the first architecture. |

## Source snapshot and terminology

The findings pin:

- `deepseek-ai/deepseek-harness` commit
  [`47f943859bef60e4160492346772ded9b24f765a`](https://github.com/deepseek-ai/deepseek-harness/tree/47f943859bef60e4160492346772ded9b24f765a),
  root version `0.1.0-rc.5`. Its README explicitly labels the project a
  developer preview and promises compatibility-breaking changes
  ([README.md](https://github.com/deepseek-ai/deepseek-harness/blob/47f943859bef60e4160492346772ded9b24f765a/README.md#L1-L23)).
- The Harness-pinned upstream Cordis commit
  [`56b3d4f725681cf4556c1a8695a709cc3b6eed74`](https://github.com/cordiverse/cordis/tree/56b3d4f725681cf4556c1a8695a709cc3b6eed74).
  Harness vendors that snapshot and documents numerous local runtime changes;
  the current vendored package identifies itself as `@deepseek-ai/cordis`
  `4.0.1`
  ([vendor manifest](https://github.com/deepseek-ai/deepseek-harness/blob/47f943859bef60e4160492346772ded9b24f765a/vendor/README.md#L1-L66),
  [package](https://github.com/deepseek-ai/deepseek-harness/blob/47f943859bef60e4160492346772ded9b24f765a/vendor/cordis/package.json#L1-L16)).
- ACP TypeScript SDK tag `v0.25.1`, commit
  [`cd8dc79b94a9d131687a2cdd02298820c32f5880`](https://github.com/agentclientprotocol/typescript-sdk/tree/cd8dc79b94a9d131687a2cdd02298820c32f5880).
  Harness pins `@agentclientprotocol/sdk` exactly to `0.25.1`
  ([package.json](https://github.com/deepseek-ai/deepseek-harness/blob/47f943859bef60e4160492346772ded9b24f765a/package.json#L135-L149)).

In this report:

- an **ACP Agent** terminates the client-to-agent ACP connection;
- an **ACP Proxy** stands between an ACP client and another ACP Agent and
  forwards or modifies protocol traffic;
- a **Conductor** owns an ordered chain of such proxies and presents the chain
  as one agent;
- an **MCP server/tool** exposes tool resources through MCP, not an agent loop;
- a **Cordis plugin** is trusted code mounted into a shared Harness process and
  lifecycle; and
- a coding-agent skill is instruction/workflow content interpreted by an agent,
  not one of the above protocol roles.

Those roles are not interchangeable merely because all can contribute agent
behavior.

## Exact Cordis plugin and composition model

### Plugin API and registration

A Cordis plugin is one of three TypeScript shapes: a function
`(ctx, config)`, a constructor, or an object with `apply(ctx, config)`. Static
metadata may declare a Standard Schema config validator, required services
(`inject`), provided service names (`provide`), and consumed intercept config
([`registry.ts`](https://github.com/deepseek-ai/deepseek-harness/blob/47f943859bef60e4160492346772ded9b24f765a/vendor/cordis/src/registry.ts#L91-L145)).
`ctx.plugin(plugin, config)` mounts one fiber; `ctx.inject(dependencies,
callback)` mounts a dependency-gated plugin and re-runs it when required service
availability changes
([`registry.ts`](https://github.com/deepseek-ai/deepseek-harness/blob/47f943859bef60e4160492346772ded9b24f765a/vendor/cordis/src/registry.ts#L147-L186)).

Custom plugins are supported. A profile under
`$DSH_HOME/profiles/<name>` owns ordinary npm dependencies for out-of-tree
plugins plus a `dsh.profile` bundle list and `cordis.patch.yml`. Bare package
names resolve through the Cordis loader; config rows name the package and its
config. Bundles contribute ordered patch layers, followed by profile, home, and
command-line overlays
([architecture](https://github.com/deepseek-ai/deepseek-harness/blob/47f943859bef60e4160492346772ded9b24f765a/docs/architecture.md#L9-L37),
[`app-boot` profiles](https://github.com/deepseek-ai/deepseek-harness/blob/47f943859bef60e4160492346772ded9b24f765a/packages/boot/app-boot/README.md#L36-L73)).
This is a real custom-plugin interface, but it is a Harness/Cordis distribution
mechanism, not an Orca plugin interface and not an ACP proxy interface.

### Shared context, plugin calls, and lifecycle

`Context` is a proxied service container. Child contexts can extend metadata,
isolate a named service into another scope, or add service-specific intercept
configuration without mutating the parent
([`context.ts`](https://github.com/deepseek-ai/deepseek-harness/blob/47f943859bef60e4160492346772ded9b24f765a/vendor/cordis/src/context.ts#L9-L40),
[`context.ts`](https://github.com/deepseek-ai/deepseek-harness/blob/47f943859bef60e4160492346772ded9b24f765a/vendor/cordis/src/context.ts#L70-L145)).
A plugin provides a service through the reflection/service APIs, and another
plugin calls it directly as `ctx.<service>` after declaring the injection.
Plugins can also communicate through typed event dispatch. Event modes include
synchronous emit, concurrent parallel, ordered serial, first-result bail, and
compositional waterfall
([`events.ts`](https://github.com/deepseek-ai/deepseek-harness/blob/47f943859bef60e4160492346772ded9b24f765a/vendor/cordis/src/events.ts#L8-L32)).

These are trusted, same-process calls over shared objects. Cordis provides no
wire isolation, authorization boundary, or ACP identity for an individual
plugin. A plugin may mount another plugin with `ctx.plugin`, but there is no
special remote “plugin invokes plugin” protocol.

Every plugin mount has a `Fiber` whose states are pending, loading, active,
failed, unloading, and disposed. Effects register cleanup functions owned by
that fiber. Unload drains cleanups, and dependencies can move a fiber between
pending and active states
([`fiber.ts`](https://github.com/deepseek-ai/deepseek-harness/blob/47f943859bef60e4160492346772ded9b24f765a/vendor/cordis/src/fiber.ts#L139-L154),
[`fiber.ts`](https://github.com/deepseek-ai/deepseek-harness/blob/47f943859bef60e4160492346772ded9b24f765a/vendor/cordis/src/fiber.ts#L402-L560)).
Startup/config errors mark the fiber failed and are observable when awaiting it;
unload contains and logs individual cleanup failures so one disposer does not
starve later cleanup
([`fiber.ts`](https://github.com/deepseek-ai/deepseek-harness/blob/47f943859bef60e4160492346772ded9b24f765a/vendor/cordis/src/fiber.ts#L610-L709)).
The loader/app boot layer fails startup loudly while transactional profile/HMR
updates retain the last good tree
([`app-boot`](https://github.com/deepseek-ai/deepseek-harness/blob/47f943859bef60e4160492346772ded9b24f765a/packages/boot/app-boot/README.md#L5-L35)).

### Prompt, model, and tool interception

Harness composes the system prompt from registered ordered sections, context,
variables, and the schemas of currently registered tools. The
`system-prompt/assemble` waterfall permits a plugin to modify the assembled
result; registrations may be global or agent-scoped
([`system-prompt`](https://github.com/deepseek-ai/deepseek-harness/blob/47f943859bef60e4160492346772ded9b24f765a/packages/core/system-prompt/src/index.ts#L13-L38),
[`system-prompt`](https://github.com/deepseek-ai/deepseek-harness/blob/47f943859bef60e4160492346772ded9b24f765a/packages/core/system-prompt/src/index.ts#L337-L542)).

Before a model step, `agent/pre-step` can rewrite or reject claimed messages.
`agent/request` can change route/request configuration but deliberately cannot
mutate the durable message history. `llm/stream` can delegate to the next LLM
adapter or supply its own stream. `agent/turn-stopping` can cause another step
([runtime types](https://github.com/deepseek-ai/deepseek-harness/blob/47f943859bef60e4160492346772ded9b24f765a/packages/core/agent/src/runtime-types.ts#L219-L278),
[`llm`](https://github.com/deepseek-ai/deepseek-harness/blob/47f943859bef60e4160492346772ded9b24f765a/packages/llm/llm/src/index.ts#L46-L65)).
These hooks are sufficient for planning/review loops when Harness owns the
agent. They cannot intercept prompts sent directly from Orca to some other ACP
Agent.

`ctx.tools.register()` contributes a model-visible schema and execute callback.
The execution pipeline exposes:

1. `tools/pre-execute` for allow, deny, or ask;
2. `tools/execute` as around-dispatch middleware;
3. `tools/post-execute` for result transformation or blocking; and
4. `tools/result` for observing the immutable final result.

The contracts are defined in
[`core/tools`](https://github.com/deepseek-ai/deepseek-harness/blob/47f943859bef60e4160492346772ded9b24f765a/packages/core/tools/src/index.ts#L137-L269)
and implemented in the guarded pipeline
([`core/tools`](https://github.com/deepseek-ai/deepseek-harness/blob/47f943859bef60e4160492346772ded9b24f765a/packages/core/tools/src/index.ts#L1328-L1675)).
Tool cancellation is cooperative for same-process code; a plugin must honor its
`AbortSignal`. A subprocess provider can enforce stronger process termination.

### Subagents, permissions, persistence, and cancellation

Subagents are a Harness capability seam layered on Cordis, not a primitive of
Cordis itself. A `SubagentProvider` starts a trusted run with parent identity,
maximum depth, optional persona and tool filter, output schema, and
`AbortSignal`; providers can additionally support resumable/continuable work
([subagent types](https://github.com/deepseek-ai/deepseek-harness/blob/47f943859bef60e4160492346772ded9b24f765a/packages/subagent/subagent/src/types.ts#L75-L149),
[`SubagentProvider`](https://github.com/deepseek-ai/deepseek-harness/blob/47f943859bef60e4160492346772ded9b24f765a/packages/subagent/subagent/src/types.ts#L240-L323)).
The repository supplies in-process spawn/fork and out-of-process ACP, Codex,
Claude Code, and Harness SDK providers. The ACP provider is an ACP **client**
that launches another ACP Agent; it is not an ACP proxy or conductor.

Approvals are tool-centric. The approval service emits durable asked/decided
events, accepts `ask` or `never` policy, and represents allowed-once, rejected,
cancelled, or unavailable outcomes. Missing or failing answerers fail closed
([user approval](https://github.com/deepseek-ai/deepseek-harness/blob/47f943859bef60e4160492346772ded9b24f765a/packages/interaction/user-approval/src/index.ts#L34-L102),
[`ApprovalService`](https://github.com/deepseek-ai/deepseek-harness/blob/47f943859bef60e4160492346772ded9b24f765a/packages/interaction/user-approval/src/index.ts#L149-L276)).

Sessions are append-only, event-sourced logs. Session events are the durable
source of model history; persistence is supplied by plugins such as JSONL or
SQLite. `session/flush` is a durability checkpoint, and the checkpoint policy
fails closed before model calls, top-level tools, and a next step
([session](https://github.com/deepseek-ai/deepseek-harness/blob/47f943859bef60e4160492346772ded9b24f765a/packages/core/session/src/index.ts#L1-L86),
[`checkpoint-policy`](https://github.com/deepseek-ai/deepseek-harness/blob/47f943859bef60e4160492346772ded9b24f765a/packages/session/session-checkpoint-policy/src/index.ts#L1-L82),
[`SQLite persistence`](https://github.com/deepseek-ai/deepseek-harness/blob/47f943859bef60e4160492346772ded9b24f765a/packages/session/session-persistence-sqlite/src/index.ts#L1-L198)).

An Agent exposes `cancel`, `whenIdle`, `followup`, `steer`, and `inject`.
Cancellation propagates through request/tool/subagent abort signals, but ordinary
JavaScript plugin code remains cooperatively cancellable. The agent loop records
durable turn outcomes and lets `agent/request-error` middleware own retry/error
recovery rather than silently retrying in every provider
([Agent contract](https://github.com/deepseek-ai/deepseek-harness/blob/47f943859bef60e4160492346772ded9b24f765a/packages/core/agent/src/runtime-types.ts#L63-L143),
[`agent-loop`](https://github.com/deepseek-ai/deepseek-harness/blob/47f943859bef60e4160492346772ded9b24f765a/packages/core/agent-loop/src/agent.ts#L113-L243)).

## ACP compatibility: verified, bounded, and lossy

### What exists now

The ACP server is not hypothetical. `packages/acp/acp/src/index.ts` constructs
an SDK `AgentSideConnection` and implements the ACP Agent methods
([implementation](https://github.com/deepseek-ai/deepseek-harness/blob/47f943859bef60e4160492346772ded9b24f765a/packages/acp/acp/src/index.ts#L231-L353)).
Each `session/new` creates a fresh Harness Agent. `session/prompt` accepts only
baseline text/resource-link input, permits one in-flight prompt per session,
waits for the whole Agent to become idle, and emits only committed assistant
message text. ACP cancellation calls that Agent's cancellation path. The bridge
maps Harness approval requests to allow-once/reject-once ACP choices
([bridge](https://github.com/deepseek-ai/deepseek-harness/blob/47f943859bef60e4160492346772ded9b24f765a/packages/acp/acp/src/index.ts#L212-L343)).

The package's own contract calls it an “automation-only” transport adapter and
lists the omitted surfaces
([README](https://github.com/deepseek-ai/deepseek-harness/blob/47f943859bef60e4160492346772ded9b24f765a/packages/acp/acp/README.md#L1-L44),
[limitations](https://github.com/deepseek-ai/deepseek-harness/blob/47f943859bef60e4160492346772ded9b24f765a/packages/acp/acp/README.md#L76-L81)).
No ACP Proxy, Conductor, or `agent-client-protocol-conductor` dependency or
implementation exists in the pinned Harness tree. Other files named “proxy” are
Harness's internal web/API proxies, not ACP protocol proxies.

The reverse path is also concrete: `dsh-subagent-acp` constructs an SDK
`ClientSideConnection`, starts another ACP process, creates a session, forwards
text, decides permission requests by policy, and returns assistant text
([ACP subagent client](https://github.com/deepseek-ai/deepseek-harness/blob/47f943859bef60e4160492346772ded9b24f765a/packages/subagent/subagent-acp/src/run.ts#L13-L24),
[`run`](https://github.com/deepseek-ai/deepseek-harness/blob/47f943859bef60e4160492346772ded9b24f765a/packages/subagent/subagent-acp/src/run.ts#L168-L367)).
It drops non-text output and does not surface remote thoughts, tool calls, or
plans. That reinforces the same automation boundary; it does not make Harness
ACP middleware.

### Why a lossless adapter is not thin

ACP 0.25.1 can advertise load/resume and session lifecycle, MCP transports,
additional roots, prompt modalities, client filesystem and terminal services,
and elicitation
([Agent capabilities](https://github.com/agentclientprotocol/typescript-sdk/blob/cd8dc79b94a9d131687a2cdd02298820c32f5880/src/schema/types.gen.ts#L54-L122),
[session capabilities](https://github.com/agentclientprotocol/typescript-sdk/blob/cd8dc79b94a9d131687a2cdd02298820c32f5880/src/schema/types.gen.ts#L4540-L4594),
[MCP transports](https://github.com/agentclientprotocol/typescript-sdk/blob/cd8dc79b94a9d131687a2cdd02298820c32f5880/src/schema/types.gen.ts#L2521-L2584)).
Its session updates include assistant/user/thought chunks, tool calls and
updates, plans, commands, modes, configuration, session information, and usage
([`SessionUpdate`](https://github.com/agentclientprotocol/typescript-sdk/blob/cd8dc79b94a9d131687a2cdd02298820c32f5880/src/schema/types.gen.ts#L5011-L5057)).

The current bridge maps only a small subset:

| Semantic area | Harness/Cordis truth | Current ACP bridge | Translation cost for parity |
| --- | --- | --- | --- |
| Session identity and history | Append-only event log; persistence, fork, resume, continuable lineage | Fresh sessions only; no load/list/resume/fork/close | High: durable ID mapping, lifecycle ownership, replay boundaries, and reconnect semantics |
| Prompt/input | Text, structured content, injections, agent-scoped context and tools | Baseline text plus resource links flattened into text | Medium/high: content fidelity and additional-root/MCP policy |
| Output/progress | Raw chunks, thoughts, tools, plans, usage, durable turn/step outcomes | Committed assistant text only | High: correlate each event and tool update to ACP session updates without leaking retries |
| Tools | Same-process registered schema plus four-stage middleware and durable result | Tool activity is not exposed | High: tool IDs, status updates, cancellation, result fidelity, and client presentation |
| Permissions | Tool-centric durable request with one-shot Harness outcomes | Allow-once/reject-once ACP choices | Medium/high: ACP option identity, client mediation, persistence, cancellation, and unavailable behavior |
| Cancellation/errors | Cooperative abort plus durable blocked/error/interrupted outcomes | Narrow stop-reason mapping; blocked/error/aborted collapse toward `end_turn` | Medium/high: preserve prompt correlation and distinguish terminal outcomes |
| Subagents | Provider registry, parent/depth/continuable state | Not represented on ACP wire | High if lineage and recovery must be externally visible |
| Workspace/client services | Harness providers and sandbox world | One absolute cwd; rejects extra roots and MCP; no ACP client fs/terminal | High when Orca owns filesystem, terminal, or permission mediation |

The bridge's stop-reason codec itself demonstrates loss: completed, aborted,
blocked, and error outcomes do not retain distinct ACP meanings, while richer
content is rejected or flattened
([`codec.ts`](https://github.com/deepseek-ai/deepseek-harness/blob/47f943859bef60e4160492346772ded9b24f765a/packages/acp/acp/src/codec.ts#L14-L65)).
Therefore:

- **Can Harness act as an ACP Agent?** Yes, now, within the documented narrow
  contract.
- **Can a Cordis plugin be exposed as an ACP Agent?** Only indirectly by mounting
  it into a complete Harness composition whose ACP package exposes the Agent.
  The individual plugin has no ACP lifecycle or transport.
- **Can a Cordis plugin be exposed as an ACP Proxy?** Not directly. One must
  author an ACP proxy process with upstream/downstream connections and decide
  how Cordis state participates. Harness supplies no such adapter.
- **Can Harness be wrapped by a thin ACP adapter without semantic loss?** No.
  The existing adapter is thin because it deliberately discards semantics; a
  parity adapter is a substantial gateway.

## Model coupling and Apple Silicon

Harness's core LLM service is provider-neutral. An `LlmAdapter` owns a provider
route and implements streaming; `llm/stream` is the swappable seam
([`llm`](https://github.com/deepseek-ai/deepseek-harness/blob/47f943859bef60e4160492346772ded9b24f765a/packages/llm/llm/src/index.ts#L46-L65),
[`LlmAdapter`](https://github.com/deepseek-ai/deepseek-harness/blob/47f943859bef60e4160492346772ded9b24f765a/packages/llm/llm/src/index.ts#L174-L233)).
There is a direct DeepSeek adapter, and examples/defaults are DeepSeek-oriented,
but this is composition rather than a hard core dependency.

The `dsh-llm-pi-ai` plugin supports installed OpenAI, Anthropic, DeepSeek, and
other provider catalogs as well as hand-declared OpenAI-compatible gateways and
self-hosted servers. A custom route supplies its protocol, base URL, and model
catalog
([adapter contract and examples](https://github.com/deepseek-ai/deepseek-harness/blob/47f943859bef60e4160492346772ded9b24f765a/packages/llm/llm-pi-ai/README.md#L1-L72)).
Thus Harness can use external non-DeepSeek models and a local model server. It
does not itself provide local inference; on Apple Silicon a separate compatible
server must expose the model endpoint, with memory/performance governed by that
server and model.

Native Harness operation on Apple Silicon is plausible and first-party tested:

- the npm/source runtime requires Node `^22.19.0` or `>=24.0.0` and pnpm `11.7.0`
  ([package.json](https://github.com/deepseek-ai/deepseek-harness/blob/47f943859bef60e4160492346772ded9b24f765a/package.json#L1-L15));
- the main CI has a `macos-latest` lane
  ([CI](https://github.com/deepseek-ai/deepseek-harness/blob/47f943859bef60e4160492346772ded9b24f765a/.github/workflows/ci.yml#L620-L629));
- the packaged Python runtime explicitly supports macOS 14+ arm64
  ([Python SDK](https://github.com/deepseek-ai/deepseek-harness/blob/47f943859bef60e4160492346772ded9b24f765a/docs/user/guide/python-sdk.md#L7-L27)); and
- local sandboxing uses macOS Seatbelt through deprecated `sandbox-exec` and
  fails closed if that facility disappears
  ([sandbox limitation](https://github.com/deepseek-ai/deepseek-harness/blob/47f943859bef60e4160492346772ded9b24f765a/packages/sandbox/sandbox-local/README.md#L5-L38)).

For a source-built custom ACP composition, operational requirements are Node,
pnpm, dependency installation/build, the chosen model endpoint and credentials,
and the desired sandbox/tool dependencies. A DeepSeek API key is optional when
using another adapter
([development prerequisites](https://github.com/deepseek-ai/deepseek-harness/blob/47f943859bef60e4160492346772ded9b24f765a/docs/development.md#L5-L32)).

## Fit against Orca plus an ACP Repository Engineering Layer

### Unique benefits Harness could provide later

- A coherent, hot-reloadable in-process runtime in which prompt sections,
  tools, policies, models, persistence, and subagent providers share typed
  lifecycle ownership.
- An event-sourced session model with durability checkpoints and replayable
  internal reasoning/tool events.
- A mature-looking tool middleware pipeline and agent-scoped registration
  model, useful if the repository engineering workflow itself becomes a full
  custom agent rather than middleware around existing agents.
- Provider-neutral external/local model routing despite the DeepSeek defaults.
- A real ACP Agent process boundary already available for an optional backend.

### Costs and mismatch for this repository

- The Repository Engineering Layer's goal is behavior reusable across arbitrary
  ACP Agents. Cordis extensions apply only within Harness-owned Agents.
- Harness would duplicate Orca/ACP concerns: session ownership, subagent
  orchestration, permissions, tool state, persistence, cancellation, and
  workspace policy.
- Mapping the two models losslessly is a product-sized integration, as the
  capability matrix shows. Adopting only the current bridge accepts substantial
  observability and lifecycle loss.
- Harness and its vendored Cordis fork are pre-release and rapidly changing.
  Building the repository's canonical workflow representation on their
  TypeScript types would couple migration work to an unstable runtime.
- Using Harness merely because it is plugin-oriented confuses an in-agent plugin
  system with an agent-independent protocol middleware layer.

## Recommended architecture and re-evaluation gate

Keep the first architecture:

```text
Orca
  -> ACP
Repository Engineering Layer (ACP Agent/proxy/conductor boundary + repo state)
  -> ACP
Codex | Claude | OpenCode | another ACP Agent
```

Keep workflow definitions, state transitions, evidence, permissions, and
recovery semantics independent of Cordis. If a process-level backend interface
is useful, define it only in ACP terms so Harness can later occupy the same slot
as any other ACP Agent:

```text
Orca / Engineering Conductor
  -> ACP
optional DeepSeek Harness ACP Agent
  -> Cordis composition
  -> chosen external or local model
```

Do not import Cordis services or event types into the canonical Repository
Engineering Layer, do not require DeepSeek Harness to port the existing LS SDK
skills, and do not build a Cordis-to-ACP proxy in the first migration.

Re-evaluate the deferred backend only after all of these are true:

1. Harness publishes a compatibility/stability policy beyond developer preview.
2. Its ACP Agent supports the session lifecycle needed by Orca (at least
   reconnect/load or resume and per-session close).
3. Required live tool, progress, plan, usage, and error outcomes cross ACP with
   explicit fidelity.
4. Permission semantics and cancellation have tested round-trip mappings.
5. Additional workspace roots and any required MCP/client services are
   supported or intentionally unnecessary.
6. A repository-specific benchmark shows Harness materially improves a target
   outcome over the simpler Orca + ACP implementation.

Until then, DeepSeek Harness is **not worth adding to the first architecture**.
It is worth preserving as an optional, process-isolated ACP backend candidate,
because that seam costs little now and avoids coupling the migration to Cordis.
