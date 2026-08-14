# Orca custom extension and ACP authority

**Research date:** 2026-08-14  
**Orca source pin:** [`cb42b60849d81ff58976200baa6b89dc5df99fb7`](https://github.com/stablyai/orca/commit/cb42b60849d81ff58976200baa6b89dc5df99fb7), package version `1.4.178-rc.2`  
**Scope:** first-party Orca source and Orca documentation only. This note decides what can be integrated without modifying Orca; it does not implement an integration.

## Decision

Orca now has a real native plugin kernel, but it is an **experimental, default-off desktop/headless extension API**, not an agent protocol and not a safe authority boundary for a repository improvement engine. Its supported host API can read a deliberately redacted active-worktree context, send text to an existing terminal, show a notification, and use plugin-private settings/storage/secrets/events. It cannot, through the supported API, read or write repository files, obtain a repository path, create a worktree or terminal, start or supervise an agent, stream an agent turn, approve a tool call, or cancel a run.

Orca has **no ACP client, ACP agent server, ACP proxy, or ACP conductor implementation** at the inspected commit. Orca launches a hard-coded catalog of coding-agent CLIs in PTYs. Prompt injection, streaming, approval input, cancellation, persistence, and status are terminal- and vendor-hook-shaped rather than ACP-shaped. A process that happens to implement ACP cannot be registered as an ACP agent in Orca.

The best fit for a repository-scoped progressive-improvement loop, without changing Orca, is therefore:

1. keep the loop and ported repository skills in a repository-owned runner;
2. use the public `orca` CLI and its worktree, terminal, orchestration task, ask/reply, and decision-gate surfaces as the Orca control plane;
3. launch a supported downstream agent in manual-permission mode, or use a command override for one supported agent profile to invoke a terminal-compatible wrapper; and
4. optionally add a **thin native Orca plugin** only as an attended launcher/status affordance.

If the runner internally uses an ACP conductor/proxy chain, it must also provide a TUI/PTY adapter. Orca will own and observe the outer process and terminal; the wrapper, not Orca, will own ACP sessions, updates, permissions, cancellation, and downstream agent compatibility. That is a useful private seam but is not ACP-native Orca integration.

A pure native-plugin implementation is not recommended. It would either be too weak when confined to the documented host API, or would rely on the plugin worker's unrestricted Node.js access to the host filesystem and process APIs. The latter is technically possible but turns the plugin into trusted arbitrary code and bypasses the capability model; Orca does not constrain it to this repository or provide per-mutation approval.

## What “plugin” means in Orca today

The current manifest is `orca-plugin.json`, `manifestVersion: 1`, `engines.orca`, and `engines.pluginApi: 1`. It can contribute commands, panels, events, language packs, keybindings, VM recipes, an `agents` artifact list, and declared capabilities. The schema itself says that the entire surface is experimental and has no compatibility promise until plugin API v1 is frozen ([manifest schema](https://github.com/stablyai/orca/blob/cb42b60849d81ff58976200baa6b89dc5df99fb7/src/shared/plugins/plugin-manifest.ts)). The feature flag defaults to false ([default settings](https://github.com/stablyai/orca/blob/cb42b60849d81ff58976200baa6b89dc5df99fb7/src/shared/constants.ts)). The system was introduced on 2026-07-27 as “experimental” ([introducing commit](https://github.com/stablyai/orca/commit/97e4776dfe54ee87f3acc0e3e2d898d9e410955f)).

When enabled, Orca discovers installed bundles below its user-data plugin directory and developer bundles from configured paths, checks engine and artifact compatibility, and lets a development path override an installed copy ([discovery](https://github.com/stablyai/orca/blob/cb42b60849d81ff58976200baa6b89dc5df99fb7/src/main/plugins/plugin-discovery.ts)). Local-path and pinned-git installation, enablement and consent, refresh, removal, worker invocation, panels, and logs are exposed through desktop IPC ([plugin IPC](https://github.com/stablyai/orca/blob/cb42b60849d81ff58976200baa6b89dc5df99fb7/src/main/ipc/plugins.ts)); `orca serve` exposes the same plugin service through Orca's own runtime RPC ([headless plugin RPC](https://github.com/stablyai/orca/blob/cb42b60849d81ff58976200baa6b89dc5df99fb7/src/main/runtime/rpc/methods/plugins.ts)). This RPC is an Orca-specific transport, not ACP.

The supported capabilities are a small, closed set:

- `workspace:read`
- `terminal:send`
- `notifications:show`
- `storage`
- `secrets`
- `events:subscribe`
- `settings:own`

([capability definitions](https://github.com/stablyai/orca/blob/cb42b60849d81ff58976200baa6b89dc5df99fb7/src/shared/plugins/plugin-capabilities.ts)). The host-call contract exposes only active-worktree display/branch/terminal metadata, terminal text injection, notifications, plugin-private key/value state and secrets, settings, and event subscription ([host API contract](https://github.com/stablyai/orca/blob/cb42b60849d81ff58976200baa6b89dc5df99fb7/src/shared/plugins/plugin-host-api.ts)). The binding deliberately omits the internal worktree ID and filesystem path, and only permits `terminal.sendText` for a terminal in the active worktree ([host bindings](https://github.com/stablyai/orca/blob/cb42b60849d81ff58976200baa6b89dc5df99fb7/src/main/plugins/plugin-host-service-bindings.ts)).

Plugin commands run in lazily forked Node.js workers. Commands have a 30-second invocation timeout, workers are eligible for idle reaping after five minutes, and the manager limits concurrent active workers ([host protocol limits](https://github.com/stablyai/orca/blob/cb42b60849d81ff58976200baa6b89dc5df99fb7/src/shared/plugins/plugin-host-protocol.ts), [worker manager](https://github.com/stablyai/orca/blob/cb42b60849d81ff58976200baa6b89dc5df99fb7/src/main/plugins/plugin-worker-manager.ts)). The supervisor restarts failed workers with bounded backoff and eventually marks them errored ([supervisor](https://github.com/stablyai/orca/blob/cb42b60849d81ff58976200baa6b89dc5df99fb7/src/main/plugins/plugin-supervisor.ts)). This lifecycle is appropriate for commands and event reactions, not for owning a multi-hour agent run in worker memory.

The worker is **trusted Node.js, not an operating-system sandbox**. Orca scrubs inherited environment variables to an allowlist, but the worker imports the plugin entry point as ordinary Node code and can import Node core modules ([worker process](https://github.com/stablyai/orca/blob/cb42b60849d81ff58976200baa6b89dc5df99fb7/src/main/plugins/plugin-host-process.ts), [environment scrubber](https://github.com/stablyai/orca/blob/cb42b60849d81ff58976200baa6b89dc5df99fb7/src/main/plugins/plugin-worker-env.ts)). Consent fingerprints explicitly distinguish this trusted worker tier ([consent fingerprint](https://github.com/stablyai/orca/blob/cb42b60849d81ff58976200baa6b89dc5df99fb7/src/shared/plugins/plugin-consent-fingerprint.ts)). Consequently, host capabilities govern calls through `orca.host.call`; they are not a filesystem or subprocess security boundary.

Panels are materially narrower. Orca serves them in a sandboxed iframe with a restrictive content security policy, and the panel bridge only permits the subset of host calls marked panel-safe: workspace context, terminal text, and notifications ([panel shell](https://github.com/stablyai/orca/blob/cb42b60849d81ff58976200baa6b89dc5df99fb7/src/shared/plugins/plugin-panel-shell.ts), [panel bridge](https://github.com/stablyai/orca/blob/cb42b60849d81ff58976200baa6b89dc5df99fb7/src/main/plugins/plugin-panel-bridge.ts)). There is no public panel-to-worker message channel. A command invocation also receives no selected-worktree or terminal context payload from the renderer ([command execution](https://github.com/stablyai/orca/blob/cb42b60849d81ff58976200baa6b89dc5df99fb7/src/renderer/src/lib/plugin-command-execution.ts)).

The manifest's `contributes.agents` field is not a working agent-registration API at this commit. The schema only declares agent artifact paths ([content-pack contribution types](https://github.com/stablyai/orca/blob/cb42b60849d81ff58976200baa6b89dc5df99fb7/src/shared/plugins/plugin-content-pack-contributions.ts)), artifact validation checks that those files exist ([artifact validation](https://github.com/stablyai/orca/blob/cb42b60849d81ff58976200baa6b89dc5df99fb7/src/main/plugins/plugin-artifact-validation.ts)), but the content-pack registry registers language packs, VM recipes, and commands—not agents ([content-pack registry](https://github.com/stablyai/orca/blob/cb42b60849d81ff58976200baa6b89dc5df99fb7/src/main/plugins/plugin-content-pack-registry.ts)). No consumer turns the declared files into launchable agents.

## How Orca actually hosts agents

### Registration and launch

The launchable agent identifiers are a source-code union, and the comment says to extend that union when adding agents ([`TuiAgent`](https://github.com/stablyai/orca/blob/cb42b60849d81ff58976200baa6b89dc5df99fb7/src/shared/tui-agent.ts)). Each identifier has a hard-coded executable detector, launch command, expected process name, and prompt-injection mode ([agent configuration](https://github.com/stablyai/orca/blob/cb42b60849d81ff58976200baa6b89dc5df99fb7/src/shared/tui-agent-config.ts)). Orca's public documentation accurately describes the consequence: selecting an agent launches its CLI process in a terminal ([supported agents](https://www.onorca.dev/docs/agents/supported), [agents and sessions](https://www.onorca.dev/docs/model/agents-sessions)).

There is no arbitrary custom-agent row in the agent catalog. Settings do allow a user to replace the command of an existing catalog agent and customize its arguments/environment ([agent settings](https://github.com/stablyai/orca/blob/cb42b60849d81ff58976200baa6b89dc5df99fb7/src/renderer/src/components/settings/AgentsPane.tsx), [settings type](https://github.com/stablyai/orca/blob/cb42b60849d81ff58976200baa6b89dc5df99fb7/src/shared/global-settings-types.ts)). The launch builder tokenizes and uses that override while retaining the selected profile's other semantics ([launch-command builder](https://github.com/stablyai/orca/blob/cb42b60849d81ff58976200baa6b89dc5df99fb7/src/shared/tui-agent-launch-command.ts)). This is the available wrapper-process seam, but the wrapper must match the borrowed profile's prompt injection, resume assumptions, expected status behavior, and interactive terminal contract. It is not a custom agent registration API.

The renderer builds a shell launch command, creates a terminal tab, applies per-agent launch preferences, and injects an initial prompt by argv, flags, environment, or a readiness-gated PTY paste ([new-tab launch](https://github.com/stablyai/orca/blob/cb42b60849d81ff58976200baa6b89dc5df99fb7/src/renderer/src/lib/launch-agent-in-new-tab.ts), [startup planning](https://github.com/stablyai/orca/blob/cb42b60849d81ff58976200baa6b89dc5df99fb7/src/shared/tui-agent-startup.ts), [prompt plan](https://github.com/stablyai/orca/blob/cb42b60849d81ff58976200baa6b89dc5df99fb7/src/renderer/src/lib/launch-agent-startup-prompt-plan.ts)). Orca's structured agent-session host call still creates a terminal-bound CLI process; it is Orca runtime RPC, not an agent protocol ([agent-session host authority](https://github.com/stablyai/orca/blob/cb42b60849d81ff58976200baa6b89dc5df99fb7/src/shared/agent-session-host-authority.ts)).

### Worktree, terminal, and state ownership

Orca owns creation and removal of git worktrees and associates terminals with them ([worktree IPC](https://github.com/stablyai/orca/blob/cb42b60849d81ff58976200baa6b89dc5df99fb7/src/main/ipc/worktrees.ts), [runtime](https://github.com/stablyai/orca/blob/cb42b60849d81ff58976200baa6b89dc5df99fb7/src/main/runtime/orca-runtime.ts), [worktree model documentation](https://www.onorca.dev/docs/model/worktrees)). The daemon owns the PTY, working directory, input/output stream, process signals, and kill escalation ([PTY session](https://github.com/stablyai/orca/blob/cb42b60849d81ff58976200baa6b89dc5df99fb7/src/main/daemon/session.ts)). The daemon is detached from Electron and intentionally remains alive across UI disconnects, allowing warm reattachment ([daemon initialization](https://github.com/stablyai/orca/blob/cb42b60849d81ff58976200baa6b89dc5df99fb7/src/main/daemon/daemon-init.ts)). PTY checkpoint/history restores terminal state; the downstream agent owns its model conversation and any vendor resume identifier.

Native Chat does not change that authority. It translates known vendors' transcript files and ignores unknown record shapes ([transcript reader](https://github.com/stablyai/orca/blob/cb42b60849d81ff58976200baa6b89dc5df99fb7/src/main/native-chat/transcript-reader.ts)). Live terminal output remains raw PTY bytes, while vendor hooks and OSC messages are normalized into working/waiting/permission/status snapshots ([hook listener](https://github.com/stablyai/orca/blob/cb42b60849d81ff58976200baa6b89dc5df99fb7/src/shared/agent-hook-listener.ts)). Orca observes tool activity; the downstream CLI actually chooses and executes tools.

### Permissions, questions, cancellation, and failure

Orca normally adds each known CLI's own permission-bypass flags; manual mode removes those defaults, but it does not replace the downstream CLI's authorizer ([permission launch policy](https://github.com/stablyai/orca/blob/cb42b60849d81ff58976200baa6b89dc5df99fb7/src/shared/tui-agent-permissions.ts)). Orca also writes selected vendors' project-trust artifacts so first-run trust menus do not consume the prompt ([trust presets](https://github.com/stablyai/orca/blob/cb42b60849d81ff58976200baa6b89dc5df99fb7/src/main/agent-trust-presets.ts)). A git worktree isolates branch/files operationally; it is not a process sandbox and does not stop the CLI or a plugin worker from accessing other paths.

Known hook payloads let Orca display a vendor question or permission request. The Native Chat interaction sends the chosen menu number or Escape back as PTY keystrokes ([interactive-card parser](https://github.com/stablyai/orca/blob/cb42b60849d81ff58976200baa6b89dc5df99fb7/src/renderer/src/components/native-chat/native-chat-interactive-prompt.ts), [interactive sender](https://github.com/stablyai/orca/blob/cb42b60849d81ff58976200baa6b89dc5df99fb7/src/renderer/src/components/native-chat/use-native-chat-interactive-send.ts)). The agent remains the enforcement point. Cancellation is likewise Escape/Ctrl-C/signal/process termination through the PTY, not an ACP cancellation request; the daemon performs graceful termination and then descendant/force cleanup ([PTY session](https://github.com/stablyai/orca/blob/cb42b60849d81ff58976200baa6b89dc5df99fb7/src/main/daemon/session.ts)).

Orca can durably represent a higher-level attended decision using its orchestration `ask`/`reply` and decision-gate `gate-create`/`gate-resolve` commands ([CLI orchestration specification](https://github.com/stablyai/orca/blob/cb42b60849d81ff58976200baa6b89dc5df99fb7/src/cli/specs/orchestration.ts), [orchestration documentation](https://www.onorca.dev/docs/cli/orchestration)). Those records coordinate a workflow; they do not by themselves grant or deny filesystem access or a downstream tool call. A runner must stop before mutation, wait for the gate result, and enforce the result.

## ACP finding

The pinned source has no dependency on the Agent Client Protocol and no implementation of ACP's initialize/session/prompt/update/cancel methods. Searches of source, documentation, and the package lock find no `agent-client-protocol` package or ACP session transport. Orca's process boundary is PTY, its structured remote boundary is its own runtime RPC, and its plugin boundary is its own host-call protocol.

Therefore:

- an **ACP Agent cannot be registered directly**;
- an **ACP Proxy or Conductor cannot be inserted into Orca's agent path directly**;
- a conductor can be **launched as a process**, but only if an outer adapter behaves as a supported terminal agent; and
- a native plugin worker can technically run an ACP client library or spawn an adapter, but that does not make Orca ACP-aware and depends on trusted Node behavior outside the supported host API.

“Orca can launch a process that internally speaks ACP” and “Orca supports ACP” are different claims. Only the first is true at this commit.

## Feasible integration shapes without modifying Orca

| Shape | Feasible now? | Lifecycle and authority boundary | Verdict |
|---|---|---|---|
| Native Orca plugin | **Yes, experimental and default-off.** | Orca discovers/consents/enables the bundle and supervises short-lived trusted Node workers and sandboxed panels. Supported calls are limited; unrestricted Node access is outside the capability boundary. | Use only as a thin attended launcher/status surface. Do not make it the durable improvement engine or security boundary. |
| Plugin-contributed agent | **No functional registration path.** | Manifest/artifact validation sees `contributes.agents`, but no registry consumer creates a launchable profile. | Reserved/dormant schema, not an integration seam. |
| Arbitrary custom CLI agent | **No first-class registry.** | The catalog is hard-coded. A command override can replace a known agent's executable, while Orca retains that known profile's terminal/prompt semantics. | A workable compatibility seam for a TUI wrapper; document which profile is borrowed and test every interaction. |
| Direct registered ACP Agent | **No.** | Orca has no ACP client/session transport. | Requires an Orca change. |
| Orca-launched ACP Conductor/Proxy chain | **Only behind a terminal wrapper.** | Orca owns the outer PTY/process/worktree. The wrapper owns ACP initialize/session/prompt/update/cancel, proxy order, permissions, and downstream failure mapping. | Feasible as an internal implementation detail, not native integration. |
| MCP server/tool | **Yes for compatible downstream agents.** | Orca can help configure MCP; the selected CLI discovers and calls the tools. Orca neither intercepts every prompt/tool result nor owns the MCP permission model. ([MCP/skills documentation](https://www.onorca.dev/docs/cli/skills), [MCP config model](https://github.com/stablyai/orca/blob/cb42b60849d81ff58976200baa6b89dc5df99fb7/src/shared/mcp-config.ts)) | Good for portable repo-specific tools; insufficient as the improvement-loop orchestrator. |
| Repository runner using the Orca CLI | **Yes; strongest supported seam.** | Orca owns worktrees, terminals, tasks, waits, questions, and gates through its runtime. The runner owns the loop policy and state machine; the downstream agent owns model/tool execution. ([CLI core specification](https://github.com/stablyai/orca/blob/cb42b60849d81ff58976200baa6b89dc5df99fb7/src/cli/specs/core.ts), [CLI reference](https://www.onorca.dev/docs/cli/reference)) | Recommended control plane. |
| Repository skills/prompts | **Yes, downstream-agent dependent.** | Skills are consumed by the selected coding agent, not Orca. | Port the LS-specific skills into a neutral canonical corpus and render adapters as needed; do not call them native Orca plugins. |

## Repository-scoped progressive-improvement loop

The loop can safely *intend* to inspect and mutate only the current Orca worktree, but Orca does not enforce that boundary for the agent or trusted plugin worker. The runner should resolve the current worktree through the `orca` CLI, verify it is the expected repository and not the primary worktree, and pass only that path to subprocesses. It should persist a durable run record containing the worktree/branch, objective, phase, agent terminal, proposed diff, test results, approval/gate ID, outcome, and cancellation/failure reason.

The supported control sequence is:

1. create or select an Orca worktree;
2. create a terminal in it and start a supported CLI agent or the compatible wrapper;
3. run research/plan/review phases read-only by runner policy;
4. stop at an Orca orchestration ask or decision gate before the first write, destructive command, external side effect, commit, push, or PR action;
5. after approval, allow the runner/downstream agent to mutate only the verified worktree;
6. read terminal output and use explicit test/diff checks rather than treating an agent status hook as success;
7. stop again at the chosen shipping boundary; and
8. store durable lessons in repository-owned Markdown/data, not solely in plugin storage or worker memory.

“Attended approval” should be represented as a durable Orca decision gate with a precise proposed scope (worktree, allowed paths, phase, external actions, expiry), plus the downstream CLI's own manual permission prompts. The runner must check both where applicable. Orca enforces its own worktree/terminal/gate records and PTY lifecycle. It does **not** enforce path confinement, tool allowlists, the meaning of a gate, or the downstream agent's promise to wait.

## Recommended architecture

```text
Orca desktop / orca serve
  ├─ optional thin native plugin: start run, show status, open terminal
  └─ public Orca CLI / orchestration RPC
       └─ repository-owned improvement runner + durable run ledger
            ├─ canonical LS workflow/skill corpus
            ├─ Orca worktree + PTY terminal
            ├─ supported CLI agent in manual mode
            └─ optional terminal wrapper
                 └─ optional ACP client/conductor/proxies
                      └─ ACP-capable downstream agent
```

This preserves Orca's current strengths—parallel worktrees, terminals, persistence, visible attended decisions—without pretending it provides ACP mediation. It also keeps the methodology and LS-specific knowledge independent of any one plugin format. DeepSeek or another harness can later sit behind the optional wrapper if it earns its place on model/tool quality; nothing in Orca's current native plugin API makes that dependency necessary.

The smallest research-to-build proof, when implementation is authorized, is not an ACP proxy. It is a terminal-compatible wrapper invoked through one known agent's command override that: resolves the current Orca worktree; creates an orchestration run/gate; delegates one read-only plan and one approved edit to a downstream agent; runs a bounded verification command; and records the result. Only after that path is reliable should an ACP conductor be added behind the same wrapper or a native plugin be added as UI.

## What would have to change in Orca for a pure native solution

A durable, safely scoped improvement plugin would need an upstream, versioned host API for at least:

- stable plugin API compatibility guarantees;
- selected worktree identity and canonical path, with explicit repo/path scope;
- worktree and terminal create/list/read/wait/close operations;
- long-running job handles, progress, cancellation, reconnect, and crash recovery;
- worker-to-panel messages;
- per-operation approval objects tied to enforceable host calls;
- a functional agent-profile registry; and
- either an ACP client/session API or an explicit guarantee that Orca remains a PTY host.

Until then, any plugin that performs the full loop by importing `node:fs` and `node:child_process` is merely a trusted local program packaged as an Orca plugin. It can be useful, but its repository scope and safety come from its own implementation and user trust, not Orca.

## Known gaps and unstable surfaces

- The native plugin API is explicitly experimental, off by default, and may change before its promised v1 freeze.
- `contributes.agents` has schema/validation but no launch registration consumer.
- Command overrides inherit a known agent profile and are not guaranteed compatible with an arbitrary wrapper.
- Native Chat and interactive cards are vendor-shaped; an unknown wrapper does not automatically receive transcript parsing, tool UI, resume, model controls, or status fidelity.
- Plugin worker consent is coarse and code-bundle based, not a per-run or per-file authorization mechanism.
- The plugin worker's actual Node authority exceeds the declared host capabilities.
- Orca's worktree boundary is operational isolation, not a filesystem sandbox.
- ACP support is absent, so any ACP adapter owns all protocol-version and error/permission translation risk.
- MCP is a downstream tool surface, not prompt or session middleware.

These findings should be revalidated against a pinned Orca commit before implementation because the plugin kernel landed less than three weeks before this research date and is actively evolving.
