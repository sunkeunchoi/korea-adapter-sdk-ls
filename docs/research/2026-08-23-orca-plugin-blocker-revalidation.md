# Orca plugin blocker revalidation at 1.4.188

**Research date:** 2026-08-23 (Asia/Seoul)

**Previous Orca source pin:** [`cb42b60849d81ff58976200baa6b89dc5df99fb7`](https://github.com/stablyai/orca/commit/cb42b60849d81ff58976200baa6b89dc5df99fb7), package version `1.4.178-rc.2`

**Public `v1.4.188` release pin:** [`f32ce859047a85a3ea4f507f633604dfbf596a0e`](https://github.com/stablyai/orca/commit/f32ce859047a85a3ea4f507f633604dfbf596a0e)

**Installed `1.4.188` build pin:** [`2b1254d68192676e04674c2826e5f8f63992f1ad`](https://github.com/stablyai/orca/commit/2b1254d68192676e04674c2826e5f8f63992f1ad)

**Scope:** first-party Orca source, release metadata, installed-runtime evidence, and this repository's source-grounded architecture decisions. No plugin or runtime was installed or activated.

## Question

Orca advanced from the package version inspected by [Determine Orca's custom
extension, ACP, and authority surface](https://github.com/sunkeunchoi/korea-adapter-sdk-ls/issues/280)
through several releases. Did those releases remove the blocker that kept this
repository's Repository Engineering Layer from becoming an Orca plugin, and is
the optional Orca Surface Plugin now a better integration point?

## Verdict

**The pure-native-plugin blocker remains.** Orca `1.4.188` still exposes the
same experimental, default-off, narrow plugin host API assessed on 2026-08-14.
Across both current histories—the 323 commits from the old pin to the public
release tag and the 343 commits from the old pin to the installed build—the
relevant plugin trees and entry points have **no source changes**
([old pin to public release](https://github.com/stablyai/orca/compare/cb42b60849d81ff58976200baa6b89dc5df99fb7...f32ce859047a85a3ea4f507f633604dfbf596a0e),
[old pin to installed build](https://github.com/stablyai/orca/compare/cb42b60849d81ff58976200baa6b89dc5df99fb7...2b1254d68192676e04674c2826e5f8f63992f1ad)).
The API still does not provide repository paths or file operations, worktree or
terminal lifecycle operations, long-running jobs, cancellation, worker-to-panel
messages, per-operation approval enforcement, a functional contributed-agent
registry, or ACP sessions.

The earlier architecture therefore remains correct:

- an **Orca Surface Plugin is feasible only as an optional, authority-free UI**
  that launches an already-installed external Runner by sending text to a
  visible terminal and displays sanitized status;
- the **Runner over Orca CLI/orchestration remains the real integration seam**;
  and
- ACP, if desired, still belongs behind a terminal-compatible adapter rather
  than inside Orca.

There is an important correction to the premise that “the plugin was blocked.”
The architecture did not identify an upstream blocker to a thin launcher/status
plugin. It deliberately deferred that UI until the Runner/PTY path was stable.
What Orca could not safely host was the **durable improvement loop and its
authority boundary**. That remains true.

Orca's public CLI is now a stronger control plane for the external Runner. It can
project worktree/card/terminal state and supervise workers, questions, gates,
cancellation, failure, and recovery. Those capabilities make a Runner vertical
slice more attractive, but they do not widen the native plugin API. The current
repository package remains explicitly inert: `activation_eligibility = "none"`,
with `orca_ui` and `worker_adapter` disabled
([package manifest](../../.repository-engineering/package.toml),
[package status](../../.repository-engineering/README.md)). The remaining path
is implementation and certification of the Runner/adapter boundary, not waiting
for another Orca version.

## Version evidence

The installed `/Applications/Orca.app/Contents/Info.plist` and
`orca status --json` both reported `1.4.188` on the research date. The installed
app's first-party `Resources/orca-local-build.json` records build ID
`1.4.188-2b1254d68192676e04674c2826e5f8f63992f1ad-arm64` and commit
`2b1254d68192676e04674c2826e5f8f63992f1ad`. The public
[`v1.4.188` release](https://github.com/stablyai/orca/releases/tag/v1.4.188)
instead points to `f32ce859047a85a3ea4f507f633604dfbf596a0e`
([`package.json` at the release pin](https://github.com/stablyai/orca/blob/f32ce859047a85a3ea4f507f633604dfbf596a0e/package.json)).

These are divergent release-cut histories, not one source identity. GitHub's
compare API reports the installed build 26 commits ahead and six commits behind
the public tag
([public tag versus installed build](https://github.com/stablyai/orca/compare/f32ce859047a85a3ea4f507f633604dfbf596a0e...2b1254d68192676e04674c2826e5f8f63992f1ad)).
The old source pin is an ancestor of both. Path-restricted comparisons against
both current pins found no changed file under `src/shared/plugins/`,
`src/main/plugins/`, or the inspected plugin IPC/RPC/renderer entry points.
Thus the strongest evidence is not merely that one current API looks similar:
the native plugin implementation relevant to the old decision is unchanged in
both the public release and the installed build.

## The exact old blockers and their current status

The 2026-08-14 research concluded with eight upstream requirements for a safe
pure-native solution
([original research record](https://github.com/sunkeunchoi/korea-adapter-sdk-ls/blob/1c73cee3354f2382b35bcda305a7a199730cb123/docs/research/2026-08-14-orca-custom-extension-acp-authority.md),
[resolution commit](https://github.com/sunkeunchoi/korea-adapter-sdk-ls/commit/1c73cee3354f2382b35bcda305a7a199730cb123)).

| Required upstream capability | Status at Orca 1.4.188 | Evidence |
|---|---|---|
| Stable plugin API compatibility guarantees | **Unresolved.** The manifest and host API still say every surface is experimental and has no compatibility promise before API v1 freezes. The master feature flag still defaults off. | [manifest](https://github.com/stablyai/orca/blob/f32ce859047a85a3ea4f507f633604dfbf596a0e/src/shared/plugins/plugin-manifest.ts), [host API](https://github.com/stablyai/orca/blob/f32ce859047a85a3ea4f507f633604dfbf596a0e/src/shared/plugins/plugin-host-api.ts), [default settings](https://github.com/stablyai/orca/blob/f32ce859047a85a3ea4f507f633604dfbf596a0e/src/shared/constants.ts) |
| Selected worktree identity and canonical path, explicitly repository/path scoped | **Unresolved.** `workspace.readContext` returns branch, display name, and terminal IDs. The host binding deliberately removes the internal worktree ID and path. | [public result schema](https://github.com/stablyai/orca/blob/f32ce859047a85a3ea4f507f633604dfbf596a0e/src/shared/plugins/plugin-host-api.ts), [redacting binding](https://github.com/stablyai/orca/blob/f32ce859047a85a3ea4f507f633604dfbf596a0e/src/main/plugins/plugin-host-service-bindings.ts) |
| Worktree and terminal create/list/read/wait/close operations | **Unresolved in the plugin API; available in the external CLI.** The plugin can only send text to an existing terminal. | [plugin capabilities](https://github.com/stablyai/orca/blob/f32ce859047a85a3ea4f507f633604dfbf596a0e/src/shared/plugins/plugin-capabilities.ts), [plugin host methods](https://github.com/stablyai/orca/blob/f32ce859047a85a3ea4f507f633604dfbf596a0e/src/shared/plugins/plugin-host-api.ts), [CLI worktree/terminal contract](https://github.com/stablyai/orca/blob/f32ce859047a85a3ea4f507f633604dfbf596a0e/src/cli/specs/core.ts) |
| Long-running job handles, progress, cancellation, reconnect, and crash recovery | **Unresolved in the plugin API; available for external supervised workers.** Plugin command calls still time out after 30 seconds and idle workers are reaped after five minutes. Orca orchestration separately has worker start/show/read/stop/abandon/release/retain/list. | [plugin worker limits](https://github.com/stablyai/orca/blob/f32ce859047a85a3ea4f507f633604dfbf596a0e/src/shared/plugins/plugin-host-protocol.ts), [orchestration worker contract](https://github.com/stablyai/orca/blob/f32ce859047a85a3ea4f507f633604dfbf596a0e/src/cli/specs/orchestration-worker-specs.ts) |
| Worker-to-panel messages | **Unresolved.** Panels can call only the panel-safe subset of host methods; command invocation supplies only plugin and command IDs. There is no public panel-to-worker status channel. | [panel-safe method derivation](https://github.com/stablyai/orca/blob/f32ce859047a85a3ea4f507f633604dfbf596a0e/src/shared/plugins/plugin-host-api.ts), [command invocation](https://github.com/stablyai/orca/blob/f32ce859047a85a3ea4f507f633604dfbf596a0e/src/renderer/src/lib/plugin-command-execution.ts) |
| Per-operation approval objects tied to enforceable host calls | **Unresolved.** Install consent covers a capability/worker-trust fingerprint. It is not a per-file or per-mutation grant. Orca gates are durable orchestration decisions, but an external Runner must stop, await, and enforce them. | [capability model](https://github.com/stablyai/orca/blob/f32ce859047a85a3ea4f507f633604dfbf596a0e/src/shared/plugins/plugin-capabilities.ts), [orchestration gates](https://github.com/stablyai/orca/blob/f32ce859047a85a3ea4f507f633604dfbf596a0e/src/cli/specs/orchestration.ts) |
| Functional agent-profile registry | **Unresolved.** `contributes.agents` remains a path-only manifest artifact. Artifact validation sees it, but the content-pack registry reconciles language packs, VM recipes, and commands/keybindings—not agents. Launchable agents remain a source-code union. | [agent contribution schema](https://github.com/stablyai/orca/blob/f32ce859047a85a3ea4f507f633604dfbf596a0e/src/shared/plugins/plugin-content-pack-contributions.ts), [content-pack registry](https://github.com/stablyai/orca/blob/f32ce859047a85a3ea4f507f633604dfbf596a0e/src/main/plugins/plugin-content-pack-registry.ts), [hard-coded agent catalog](https://github.com/stablyai/orca/blob/f32ce859047a85a3ea4f507f633604dfbf596a0e/src/shared/tui-agent.ts) |
| ACP client/session API, or a stable explicit PTY-host contract | **Unresolved for ACP.** Orca still has no ACP client/session dependency or initialize/session/prompt/update/cancel implementation. The sole current `acp` source reference in an agent launch context classifies Prime Agent's own `--mode acp` as non-interactive; Orca does not speak that protocol. | [Prime Agent headless classifier](https://github.com/stablyai/orca/blob/f32ce859047a85a3ea4f507f633604dfbf596a0e/src/shared/prime-agent-headless-command.ts), [Orca dependencies](https://github.com/stablyai/orca/blob/f32ce859047a85a3ea4f507f633604dfbf596a0e/package.json), [terminal-bound agent session contract](https://github.com/stablyai/orca/blob/f32ce859047a85a3ea4f507f633604dfbf596a0e/src/shared/agent-session-host-authority.ts) |

All eight requirements remain unresolved **inside the native plugin API**.
Several have strong equivalents in the public CLI, which is exactly why the old
architecture put the Runner outside the plugin.

## Current extension and authority surface

### Plugin lifecycle and API stability

The manifest remains `orca-plugin.json`, `manifestVersion: 1`, and
`pluginApi: 1`, but its source comments still call the whole surface
experimental. The default global setting remains `pluginSystemEnabled: false`.
The supported capability set is unchanged:

- `workspace:read`
- `terminal:send`
- `notifications:show`
- `storage`
- `secrets`
- `events:subscribe`
- `settings:own`

The host method table correspondingly exposes a redacted focused-worktree
context, terminal text injection, notifications, plugin-private storage and
secrets, plugin-owned settings, and subscriptions to three host events. It has
no filesystem, repository, subprocess, worktree lifecycle, terminal lifecycle,
agent lifecycle, orchestration, or generic runtime-RPC capability
([capabilities](https://github.com/stablyai/orca/blob/f32ce859047a85a3ea4f507f633604dfbf596a0e/src/shared/plugins/plugin-capabilities.ts),
[host method table](https://github.com/stablyai/orca/blob/f32ce859047a85a3ea4f507f633604dfbf596a0e/src/shared/plugins/plugin-host-api.ts)).

Plugin workers remain trusted Node processes rather than an OS sandbox. They can
use Node authority outside the supported host API, but that bypasses rather than
extends Orca's capability model. The process imports the plugin entry point as
ordinary Node code after environment scrubbing
([worker process](https://github.com/stablyai/orca/blob/f32ce859047a85a3ea4f507f633604dfbf596a0e/src/main/plugins/plugin-host-process.ts),
[environment scrubber](https://github.com/stablyai/orca/blob/f32ce859047a85a3ea4f507f633604dfbf596a0e/src/main/plugins/plugin-worker-env.ts)).
Putting repository mutation in `node:fs` or starting the Runner with
`node:child_process` would therefore make the bundle a trusted arbitrary local
program; Orca would not enforce repository confinement or per-operation
approval.

### Agent registry and ACP

The launch catalog remains a hard-coded `TuiAgent` union with per-agent launch
configuration. Command overrides can replace a known profile's executable, but
the wrapper inherits that profile's prompt injection and lifecycle assumptions
([agent catalog](https://github.com/stablyai/orca/blob/f32ce859047a85a3ea4f507f633604dfbf596a0e/src/shared/tui-agent.ts),
[launch builder](https://github.com/stablyai/orca/blob/f32ce859047a85a3ea4f507f633604dfbf596a0e/src/shared/tui-agent-launch-command.ts)).
The dormant `contributes.agents` field still does not create a launchable agent.

No native ACP transport appeared. The current source contains the word `acp`
for Prime Agent's own headless mode, but Orca treats that as a non-interactive
child-process invocation. That is evidence that Orca can launch a program which
has an ACP mode, not that Orca implements an ACP client. Prompt delivery,
session ownership, output, and cancellation remain terminal/PTY and
provider-hook shaped.

### Prompt and tool interception, permissions, and user interaction

Orca normalizes known providers' hooks into status, tool, question, and
permission projections. This is observability, not generic middleware that can
intercept every prompt or tool call. Native Chat answers known questions and
approvals by sending selector keystrokes, option `1`, or Escape into the PTY
([hook normalization](https://github.com/stablyai/orca/blob/f32ce859047a85a3ea4f507f633604dfbf596a0e/src/shared/agent-hook-listener.ts),
[interactive card parser](https://github.com/stablyai/orca/blob/f32ce859047a85a3ea4f507f633604dfbf596a0e/src/renderer/src/components/native-chat/native-chat-interactive-prompt.ts),
[PTY answer/cancel sender](https://github.com/stablyai/orca/blob/f32ce859047a85a3ea4f507f633604dfbf596a0e/src/renderer/src/components/native-chat/use-native-chat-interactive-send.ts)).
The downstream agent remains the authorizer and tool executor.

Orca's manual/yolo setting changes the flags or environment passed to each
known CLI. For example, yolo mode adds vendor-specific bypass flags, while
manual mode removes those defaults. This does not create an Orca-owned tool
permission engine or filesystem sandbox
([permission launch policy](https://github.com/stablyai/orca/blob/f32ce859047a85a3ea4f507f633604dfbf596a0e/src/shared/tui-agent-permissions.ts)).
Plugin install consent is likewise capability-bundle consent, not an attended
approval for one file mutation.

### Worktree, card, terminal, cancellation, and failure projection

The public CLI is materially richer than the native plugin facade:

- `worktree create/show/current/set/rm/ps` can bind issue identity, parent
  lineage, comments, and board status;
- `terminal create/list/show/read/send/wait/stop/close` exposes visible PTY
  lifecycle and bounded output; and
- orchestration Runs, Tasks, Dispatches, ask/reply, and gates provide durable
  coordination records
  ([CLI worktree and terminal specification](https://github.com/stablyai/orca/blob/f32ce859047a85a3ea4f507f633604dfbf596a0e/src/cli/specs/core.ts),
  [orchestration specification](https://github.com/stablyai/orca/blob/f32ce859047a85a3ea4f507f633604dfbf596a0e/src/cli/specs/orchestration.ts)).

Supervised workers have explicit start, inspect, bounded read, fence-and-stop,
abandon-without-false-stop, post-settlement release, retain, and resource-list
operations. Current `worker-show` can also distinguish a human-answerable wait
from a failure when it has evidence. Cleanup acts on an exact owned terminal and
preserves an output archive; uncertain ownership stays retained rather than
being destructively guessed
([worker CLI contract](https://github.com/stablyai/orca/blob/f32ce859047a85a3ea4f507f633604dfbf596a0e/src/cli/specs/orchestration-worker-specs.ts),
[proof-based worker stop](https://github.com/stablyai/orca/blob/f32ce859047a85a3ea4f507f633604dfbf596a0e/src/main/runtime/rpc/methods/orchestration-worker-stop.ts),
[worker observation](https://github.com/stablyai/orca/blob/f32ce859047a85a3ea4f507f633604dfbf596a0e/src/main/runtime/rpc/methods/orchestration-worker-observation.ts),
[version-matched orchestration guide](https://github.com/stablyai/orca/blob/f32ce859047a85a3ea4f507f633604dfbf596a0e/skill-guides/orchestration.md)).

These are appropriate Runner building blocks. They do not enforce the semantic
meaning of a gate, sandbox an agent to one worktree, or make the plugin the
owner of a long-running attempt. A Runner must still freeze its own attempt
identity and policy, verify the selected worktree, stop before gated mutations,
interpret cancellation/failure conservatively, and keep authoritative recovery
state outside plugin storage.

## What actually changed since the old assessment

The changes relevant to this decision are material safety and observability
improvements to the already-selected **CLI/orchestration route**, not removal of
the plugin blocker. Orca's `1.4.187` release notes group them explicitly under
Orchestration ([first-party release notes](https://github.com/stablyai/orca/releases/tag/v1.4.187)):

- orchestration mutations now expose broader retry-request recovery support;
- `worker-show` has an evidence-qualified `agentWait` observation so a worker
  blocked on a human prompt is not automatically treated as failed; and
- low-level operator-created dispatches are more explicitly represented as
  unsupervised, preventing stop/release commands from claiming process authority
  they do not own;
- worker stop now reports an unknown result unless exact dispatch ownership and
  PTY termination are proven, while process loss carries a reason rather than
  being treated as a death certificate.

The core supervised-worker surface—start, read, stop, abandon, retain, release,
and resource accounting—already existed at the old source pin. The current
release refines its recovery semantics rather than introducing the missing
native plugin authority.

## Consequence for this repository

Do not wait for Orca to become ACP-native or for the plugin API to become a
repository runtime before continuing the Repository Engineering Layer. Also do
not collapse the external Runner into trusted Node plugin code.

The next defensible proof remains a narrow Runner/PTY slice:

1. keep the repository package inert and use one low-risk capability;
2. have an external Runner resolve and verify the exact Orca worktree;
3. create a Run, Task, and supervised worker through the public CLI;
4. exercise one human question/gate, one deliberate cancellation, one retained
   failure, and one exact resume/retry;
5. preserve the attempt record outside plugin storage; and
6. only after that path is stable, add a development Orca Surface Plugin whose
   sole actions are terminal text launch, sanitized status display, and opening
   the relevant terminal.

That plugin should contain no `node:fs`, `node:child_process`, credentials,
checkpoint authority, or mutation policy. Disabling or timing out the plugin
must not strand the Runner. Because the plugin API is still explicitly
experimental, pin the exact tested Orca/plugin version and treat each Orca
upgrade as a compatibility test, not an assumed semver guarantee.
