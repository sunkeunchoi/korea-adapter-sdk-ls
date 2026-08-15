# Open-source GitHub issue-to-agent orchestration systems

Research date: 2026-08-15 (Asia/Seoul)

## Question

Is there a maintained open-source system that can discover a new GitHub issue,
triage it, dispatch an isolated coding agent, test and review the result, and
return a pull request or durable issue outcome—and does any such system remove
the need for this repository's proposed Orca-facing Repository Improvement
Runner?

## Verdict

**There are strong open-source parts and one credible all-in-one alternative,
but nothing currently eliminates the Repository Improvement Runner while Orca
remains the execution environment.**

The best fit is a composition:

1. **Integrate GitHub Agentic Workflows (`gh-aw`) for event-driven issue intake
   and triage.** Its issue workflow can run on `issues: [opened, edited]` with a
   read-only agent and constrained `add-labels`/`add-comment` safe outputs. This
   is a better and safer answer to "a triage agent picks up every new issue"
   than exposing a local Orca process to public webhooks or polling every issue
   with an unconstrained coding agent.
2. **Keep Orca as the local execution control plane.** An Orca automation can
   discover issues that triage moved to `ready-for-agent`; the Runner then
   claims the issue, binds the attempt to one Orca worktree, selects the
   repository capability, and enforces repository-specific gates and evidence.
3. **Adopt OpenCode only as an optional execution adapter, not as the workflow
   owner.** OpenCode is MIT-licensed, actively maintained, and exposes a native
   stdio ACP agent (`opencode acp`) with project rules, permissions, agents,
   skills, and MCP support. It is therefore an immediately available backend
   for the proposed ACP Conductor seam. Its GitHub Action can implement an issue
   after an explicit `/opencode` comment, but it does not perform automatic
   triage or preserve a durable multi-stage run.
4. **Borrow, but do not adopt, Open SWE's runtime architecture.** Open SWE is the
   closest full-stack alternative: signed GitHub webhooks, persistent per-thread
   sandboxes, LangGraph state, turn checkpoints, subagents, plan/workflow-file
   approvals, draft PRs, review, skills, and MCP integrations. It is a credible
   choice for an organization that wants a hosted internal coding-agent service.
   Here it would duplicate and displace Orca's worktrees, terminals, cards, and
   agent orchestration while adding a LangGraph service, GitHub App, dashboard,
   and cloud-sandbox control plane.
5. **Do not adopt OpenHands Resolver, Sweep, or SWE-agent as the coordinator.**
   The OpenHands V0 resolver was removed upstream in April 2026; Sweep has moved
   on from its issue-to-PR bot and its root license is not open source for
   production use; SWE-agent is now superseded by mini-SWE-agent, and
   mini-SWE-agent/SWE-ReX are agent/runtime primitives rather than GitHub
   workflow owners.

The Runner can consequently be **thin**, but not absent. Its irreducible job is
the boundary no evaluated project owns: GitHub/Orca identity synchronization,
atomic issue claiming, capability selection, `queue/items.jsonl` reconciliation,
exact LS gate and evidence semantics, attended credential/live-market routing,
and crash recovery across GitHub, Orca, terminals, and optional ACP sessions.

## Inspected snapshots

All source links below are pinned to the inspected commit rather than a moving
default branch.

| Project | Inspected source | License / activity signal |
| --- | --- | --- |
| GitHub Agentic Workflows | [`github/gh-aw@c35faf4`](https://github.com/github/gh-aw/tree/c35faf436c798d9535df6b69a6c6f3708c5ec021), release [`v0.86.2`](https://github.com/github/gh-aw/releases/tag/v0.86.2) | [MIT](https://github.com/github/gh-aw/blob/c35faf436c798d9535df6b69a6c6f3708c5ec021/LICENSE); public preview, release published 2026-08-11 and inspected commit dated 2026-08-14. |
| Open SWE | [`langchain-ai/open-swe@8dcac4d`](https://github.com/langchain-ai/open-swe/tree/8dcac4d4470b0e93369eae3f1f3301e3dd37514a) | [MIT](https://github.com/langchain-ai/open-swe/blob/8dcac4d4470b0e93369eae3f1f3301e3dd37514a/LICENSE); inspected commit dated 2026-08-15; no stable release was used as the research baseline. |
| OpenCode | [`anomalyco/opencode@4643e65`](https://github.com/anomalyco/opencode/tree/4643e65ad6334de3e4e68dedc201d5fbb828c9fe), release [`v1.18.18`](https://github.com/anomalyco/opencode/releases/tag/v1.18.18) | [MIT](https://github.com/anomalyco/opencode/blob/4643e65ad6334de3e4e68dedc201d5fbb828c9fe/LICENSE); inspected commit dated 2026-08-14 and release published 2026-08-13. |
| SWE-agent / mini-SWE-agent / SWE-ReX | [`SWE-agent@3ea751c`](https://github.com/SWE-agent/SWE-agent/tree/3ea751c087f32b16e039a2233dd6eefecef325d5), [`mini-swe-agent@a83fcae`](https://github.com/SWE-agent/mini-swe-agent/tree/a83fcae82d2a08f0ee0c688f9d137b3566c097f8), [`SWE-ReX@5c995c3`](https://github.com/SWE-agent/SWE-ReX/tree/5c995c365dfb1fd5bc56fda688be5d8538f9931f) | All [MIT](https://github.com/SWE-agent/SWE-ReX/blob/5c995c365dfb1fd5bc56fda688be5d8538f9931f/LICENSE.txt). mini-SWE-agent release [`v2.4.6`](https://github.com/SWE-agent/mini-swe-agent/releases/tag/v2.4.6); SWE-agent's own README directs new users to mini-SWE-agent. |
| OpenHands / legacy Resolver | Current [`OpenHands@dc99e98`](https://github.com/OpenHands/OpenHands/tree/dc99e98615de4ace821692773b00a7f50d476e50), release [`v1.13.0`](https://github.com/OpenHands/OpenHands/releases/tag/v1.13.0); resolver's final parent [`7bc3300`](https://github.com/OpenHands/OpenHands/tree/7bc3300981fa1cb4689d6e1b0c0bdd7fd77ac954) | Current project is [MIT](https://github.com/OpenHands/OpenHands/blob/dc99e98615de4ace821692773b00a7f50d476e50/LICENSE), but the resolver was [removed in `cc100c0`](https://github.com/OpenHands/OpenHands/commit/cc100c0d10fbefcc35eb80936a926405801d941a) on 2026-04-23. |
| Sweep | [`sweepai/sweep@a8b8b67`](https://github.com/sweepai/sweep/tree/a8b8b67bda4f89faac9314d34e7c7d5a64f76046) | The root [Sweep EE License](https://github.com/sweepai/sweep/blob/a8b8b67bda4f89faac9314d34e7c7d5a64f76046/LICENSE) restricts production/commercial use and redistribution; inspected code commit dated 2025-09-18. |

Release dates are maintenance signals, not quality guarantees. Open SWE's rapid
change without a pinned stable release and `gh-aw`'s public-preview status both
argue for version pinning and a small integration boundary.

## Capability comparison

Legend: **yes** means the capability is present in the inspected open-source
source; **partial** means it exists only as a lower-level primitive or with a
material limitation; **no** means no relevant implementation was found.

| System | New-issue trigger and automatic triage | Issue to PR | Isolation | Durable state / resume | Approval and write boundary | Multi-agent and repo instructions | ACP / MCP | Orca fit |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| **`gh-aw`** | **Yes.** GitHub event triggers; official triage pattern classifies, detects duplicates, and asks for missing data. | **Yes.** `create-pull-request` transports the agent's git bundle to a separate write-scoped job; draft is default. | **Yes.** Agent Workflow Firewall container plus optional gVisor or microVM; Linux self-hosted runners supported. | **Partial.** GitHub run logs/artifacts and issue/PR outputs are durable, but a run is an ephemeral Actions job, not a resumable Orca workflow. | **Strongest.** Read-only agent job; validated, bounded SafeOutputs; protected files, branch/file/patch limits, staged rollout. | **Yes/partial.** Inline subagents and skills are projected into supported engine formats, but execution remains within one Actions run. | **MCP yes**, including an MCP Gateway and `gh aw mcp-server`; no ACP agent/proxy role found. | **Complement.** Excellent GitHub intake/safe-output layer; not an Orca worktree or terminal controller. |
| **Open SWE** | **Partial.** Signed `issues`/comment webhooks, but a title/body/comment must mention an Open SWE handle; it is implementation dispatch, not automatic classification of every issue. | **Yes.** Agent commits, pushes, opens/updates draft PRs, responds to feedback, monitors CI, and has a reviewer graph. | **Yes.** One persistent pluggable cloud sandbox per thread; local backend is explicitly non-isolated dev mode. | **Yes.** LangGraph thread state, saved sandbox identity/reconnection, per-turn git refs, follow-up message queue. | **Mixed.** User/repository allowlists, plan mode, and workflow-file push approvals exist; the agent otherwise receives full shell access and GitHub operations through `gh`, unlike `gh-aw` SafeOutputs. | **Yes.** Deep Agents subagents, middleware, `AGENTS.md`, bundled skills, user skills. | **MCP partial** through selected server-side integrations; no ACP implementation found. | **Replacement, not complement.** It brings its own service, dashboard, state and sandbox control planes. |
| **OpenCode GitHub Action + ACP agent** | **No automatic triage.** Action starts on explicit `/opencode` or `/oc` issue/PR comments. | **Yes.** For an issue it creates a branch, edits, pushes, and opens a closing PR. | **Partial.** GitHub-hosted runner isolation for the Action; local/Orca execution inherits the host boundary. | **Partial.** An OpenCode session exists within the Action/local runtime, but the Action does not implement a durable cross-stage issue-run ledger or resume protocol. | **Partial.** GitHub App/OIDC token flow and agent permission system; writes occur from the agent process rather than a separate validated SafeOutputs stage. | **Yes.** Agents, subagents, skills, `AGENTS.md`, custom commands and permissions. | **ACP and MCP yes.** Native `opencode acp` stdio server is the strongest protocol fit inspected. | **Strong execution adapter.** Orca already launches CLI agents; ACP also fits the planned Conductor seam. It does not own intake or repository policy. |
| **mini-SWE-agent + SWE-ReX** | **No.** A CLI/Python task is supplied by a caller. | **Partial.** The model can be prompted to use `gh`, but there is no first-party issue lifecycle/PR coordinator. | **Yes at runtime layer.** mini supports several sandboxes; SWE-ReX abstracts local, Docker, remote, Fargate, Modal, and Daytona execution. | **Partial.** Linear message trajectory and runtime sessions; no GitHub workflow checkpoint/resume owner. | **No workflow-level boundary.** Caller owns credentials and approvals. | **Partial.** Parallel runs and configurable agent; not an issue-level worker DAG or portable repository capability system. | No ACP/MCP implementation found in the inspected projects. | **Possible backend primitive**, but less directly useful than Orca's existing terminals/worktrees and OpenCode ACP support. |
| **OpenHands V0 Resolver** | **Label/comment dispatch only.** `fix-me` label or `@openhands-agent`; no automatic triage. | **Yes, historically.** Draft PR on success or branch on failure, plus issue comment. | **Yes, historically**, through OpenHands runtime/container settings. | **Weak.** JSONL output and branch/PR outcome, not a resumable multi-stage state machine. | **Weak.** Example granted contents/issues/PR write to the workflow and required a broad PAT in some configurations. | Repository microagent instructions; resolver described one issue at a time. | Current OpenHands has MCP features, but the removed resolver was not ACP middleware. | **Reject.** The exact component was deleted upstream, so using it means maintaining a fork of a retired implementation. |
| **Sweep** | Historically label/comment/GitHub App dispatch; no current evidence of a maintained automatic triage-to-execution boundary. | Historically yes. | Partial remote sandbox/container machinery. | Product-specific progress state, but the inspected repository no longer presents this as its active direction. | Product/GitHub App specific. | Product-specific agents/configuration; no portable capability contract. | No ACP/MCP implementation found. | **Reject.** License, maintenance direction, and architecture all conflict with the target. |

## Candidate findings

### GitHub Agentic Workflows: integrate the intake and safety model

The official
[`ai-issue-triage.md`](https://github.com/github/gh-aw/blob/c35faf436c798d9535df6b69a6c6f3708c5ec021/docs/src/content/docs/guides/ai-issue-triage.md)
is an exact match for the desired front door: `issues` opened/edited starts an
agent that reads repository and issue context, classifies type/priority, looks
for duplicates, and requests missing details. The agent receives read
permissions while only allowlisted labels and one bounded comment are
externalized by SafeOutputs.

That separation is structural, not merely a prompt. The
[security architecture](https://github.com/github/gh-aw/blob/c35faf436c798d9535df6b69a6c6f3708c5ec021/docs/src/content/docs/introduction/architecture.mdx)
runs the agent and write-capable handlers in different jobs; buffered outputs
can be structurally constrained, analyzed, sanitized, and rejected before the
write job. The
[pull-request SafeOutput](https://github.com/github/gh-aw/blob/c35faf436c798d9535df6b69a6c6f3708c5ec021/docs/src/content/docs/reference/safe-outputs-pull-requests.md)
defaults to draft PRs and supports allowed repositories/branches, protected and
excluded files, maximum patch size/file count, reviewers, and a runtime kill
switch. It moves code as a git bundle or patch artifact rather than giving the
agent's job direct repository write access.

`gh-aw` also has the best reusable security ideas in the set:

- a report-only → staged → shadow → production
  [safe-rollout ladder](https://github.com/github/gh-aw/blob/c35faf436c798d9535df6b69a6c6f3708c5ec021/docs/src/content/docs/practices/safe-rollout.md);
- a sandbox/firewall with allowlisted egress and stronger gVisor/microVM options,
  documented in its
  [security architecture](https://github.com/github/gh-aw/blob/c35faf436c798d9535df6b69a6c6f3708c5ec021/docs/src/content/docs/introduction/architecture.mdx);
- [inline subagents](https://github.com/github/gh-aw/blob/c35faf436c798d9535df6b69a6c6f3708c5ec021/docs/src/content/docs/reference/inline-sub-agents.md)
  projected to Copilot, Claude, Codex, and Gemini host formats; and
- MCP in both directions: an isolated MCP Gateway and a
  [`gh aw mcp-server`](https://github.com/github/gh-aw/blob/c35faf436c798d9535df6b69a6c6f3708c5ec021/docs/src/content/docs/reference/gh-aw-as-mcp-server.md)
  exposing compilation, audit, logs, checks, and maintenance tools.

It still does not replace the Runner. An Actions workflow does not create or
own an Orca card/worktree, synchronize terminal/ACP sessions, or resume a local
attempt after an app/host failure. Its state is primarily one workflow run plus
GitHub artifacts and resulting issues/PRs. Its own
[FAQ](https://github.com/github/gh-aw/blob/c35faf436c798d9535df6b69a6c6f3708c5ec021/docs/src/content/docs/reference/faq.md)
describes agentic workflows as additive to deterministic CI, not a replacement
for it. That is the correct role here as well.

**Recommended integration:** use one pinned `gh-aw` triage workflow to assign
only a bounded label vocabulary such as `needs-info`, `ready-for-agent`,
`ready-for-human`, `duplicate-candidate`, task type, and risk class. It may ask
for information, but should not automatically close an issue or mark `wontfix`.
The local Orca/Runner side consumes `ready-for-agent`; no GitHub Actions agent
receives LS paper credentials.

### Open SWE: strongest full replacement if Orca were removed

Open SWE's
[architecture and feature inventory](https://github.com/langchain-ai/open-swe/blob/8dcac4d4470b0e93369eae3f1f3301e3dd37514a/README.md)
covers nearly the entire generic flow. Its signed
[GitHub webhook route](https://github.com/langchain-ai/open-swe/blob/8dcac4d4470b0e93369eae3f1f3301e3dd37514a/agent/webhooks/github_routes.py)
handles issue, issue-comment, PR and review events, enforces repository/user
gates, and dispatches a deterministic LangGraph thread. The trigger is
deliberate—an Open SWE mention is required—so automatic triage of every new
issue would still be custom work.

The runtime has strong state mechanics:

- one sandbox ID is stored against a thread and
  [reconnected on later runs](https://github.com/langchain-ai/open-swe/blob/8dcac4d4470b0e93369eae3f1f3301e3dd37514a/agent/utils/sandbox_state.py);
- every turn records a bounded, private git-ref
  [worktree checkpoint](https://github.com/langchain-ai/open-swe/blob/8dcac4d4470b0e93369eae3f1f3301e3dd37514a/agent/utils/turn_checkpoint.py);
- [plan-mode middleware](https://github.com/langchain-ai/open-swe/blob/8dcac4d4470b0e93369eae3f1f3301e3dd37514a/agent/middleware/plan_mode.py)
  removes mutating tools while planning; and
- GitHub Actions workflow changes carry durable, fingerprinted
  [push-approval records](https://github.com/langchain-ai/open-swe/blob/8dcac4d4470b0e93369eae3f1f3301e3dd37514a/agent/dashboard/workflow_approval.py).

It also demonstrates a useful improvement-loop pattern: the bundled
[`continual-learning` skill](https://github.com/langchain-ai/open-swe/blob/8dcac4d4470b0e93369eae3f1f3301e3dd37514a/agent/skills/continual-learning/SKILL.md)
promotes repeatedly confirmed review findings and suppresses recurring false
positives based on recorded outcomes. That is safer than recursively asking an
agent to "improve the repository" without measured feedback.

But Open SWE's
[installation](https://github.com/langchain-ai/open-swe/blob/8dcac4d4470b0e93369eae3f1f3301e3dd37514a/docs/INSTALLATION.md)
requires its own backend, webhook service, GitHub App, state, dashboard and
sandbox configuration. Its default isolated runtime is a cloud sandbox; `local`
is explicitly development-only and unisolated. Its GitHub App is intentionally
write-capable, and ordinary GitHub operations happen through `gh` in the
sandbox rather than through a universally staged output boundary. Those are
reasonable decisions for its product, but they overlap directly with Orca and
are less conservative than needed for an LS trading SDK.

**Decision:** borrow persistent-thread/sandbox identity, per-turn git
checkpoints, approval fingerprinting, queued follow-up messages, and
outcome-based reviewer learning. Do not adopt the runtime unless the upstream
destination changes from "Orca-native" to "self-hosted internal coding-agent
service."

### OpenCode: adoptable ACP execution adapter, not coordinator

The OpenCode
[GitHub Action](https://github.com/anomalyco/opencode/blob/4643e65ad6334de3e4e68dedc201d5fbb828c9fe/github/README.md)
responds to explicit `/opencode` or `/oc` comments. For issues its
[`github/index.ts`](https://github.com/anomalyco/opencode/blob/4643e65ad6334de3e4e68dedc201d5fbb828c9fe/github/index.ts)
creates a branch, runs an agent, commits, pushes, opens a closing PR, and updates
the issue comment. For PR/review comments it can update the existing branch.
This is a maintained and compact issue-to-PR implementation, but it has neither
automatic triage nor a durable repository-level sequence across Actions runs.

The more important component for this architecture is OpenCode's native
[ACP support](https://github.com/anomalyco/opencode/blob/4643e65ad6334de3e4e68dedc201d5fbb828c9fe/packages/web/src/content/docs/acp.mdx):
`opencode acp` exposes the normal OpenCode agent over stdio while retaining file
and terminal tools, custom commands, MCP servers, `AGENTS.md`, agents, and its
permission system. This is already the interface the proposed ACP adapter would
otherwise need to build around a CLI-only agent.

**Decision:** make OpenCode one optional, pinned Agent Adapter in the
falsification experiment. Do not make the OpenCode Action the workflow owner;
`gh-aw` is safer for intake and Orca is the selected workspace/runtime host.

### SWE-agent, mini-SWE-agent, and SWE-ReX: useful execution primitives only

SWE-agent's current
[README](https://github.com/SWE-agent/SWE-agent/blob/3ea751c087f32b16e039a2233dd6eefecef325d5/README.md)
states that most development moved to mini-SWE-agent and recommends it for new
use. mini-SWE-agent deliberately provides a small linear-history agent whose
only tool is bash and supports local, Docker/Podman, Apptainer, bubblewrap and
other environments, as documented in its
[README](https://github.com/SWE-agent/mini-swe-agent/blob/a83fcae82d2a08f0ee0c688f9d137b3566c097f8/README.md).

SWE-ReX is the more reusable architectural piece. Its
[runtime interface](https://github.com/SWE-agent/SWE-ReX/blob/5c995c365dfb1fd5bc56fda688be5d8538f9931f/src/swerex/runtime/abstract.py)
and
[deployment backends](https://github.com/SWE-agent/SWE-ReX/tree/5c995c365dfb1fd5bc56fda688be5d8538f9931f/src/swerex/deployment)
separate agent logic from local, Docker, remote, Fargate, Modal, and Daytona
execution and support many concurrent shells.

None owns the GitHub intake/triage, issue claiming, permissions, PR lifecycle,
approval policy, or repository capability migration. Adding SWE-ReX beneath
Orca would introduce a second execution abstraction where Orca already owns
worktrees and PTYs. Its clean runtime interface is worth borrowing only if a
future backend needs remote Linux isolation that Orca cannot supply.

### OpenHands Resolver: a historical precedent, not a current dependency

Immediately before deletion, the resolver
[README](https://github.com/OpenHands/OpenHands/blob/7bc3300981fa1cb4689d6e1b0c0bdd7fd77ac954/openhands/resolver/README.md)
documented `fix-me` label and `@openhands-agent` triggers, one-issue-at-a-time
resolution, a draft PR on success, a pushed branch on failure, JSONL output,
and repository microagent instructions. Its
[example workflow](https://github.com/OpenHands/OpenHands/blob/7bc3300981fa1cb4689d6e1b0c0bdd7fd77ac954/openhands/resolver/examples/openhands-resolver.yml)
gave the job contents, pull-request, and issue write permissions and invoked a
reusable workflow from the moving `main` branch.

The decisive current fact is the upstream commit
[`cc100c0` "Removed the V0 resolver"](https://github.com/OpenHands/OpenHands/commit/cc100c0d10fbefcc35eb80936a926405801d941a),
which deleted the resolver code, tests, examples, and workflow. The generic
OpenHands agent may remain a possible model/agent backend, but "OpenHands
Resolver" cannot be selected as a maintained issue orchestrator.

### Sweep: reject on license and direction

At the inspected commit, Sweep's
[README](https://github.com/sweepai/sweep/blob/a8b8b67bda4f89faac9314d34e7c7d5a64f76046/README.md)
only thanks users and points to a JetBrains assistant. The repository retains
historical webhook handlers, PR creation agents, tests, and deployment files,
but the active README no longer documents the issue-to-PR service as its
direction.

More importantly, the root
[license](https://github.com/sweepai/sweep/blob/a8b8b67bda4f89faac9314d34e7c7d5a64f76046/LICENSE)
allows personal non-commercial development/testing but requires a subscription
for production/commercial use and forbids ordinary redistribution/derivative
use of the EE portions. A public source repository is not sufficient to call a
system open source. Sweep is therefore not an acceptable foundation for the
requested open-source repository plugin.

## Why generic orchestration cannot preserve the LS repository contract

All candidates can be told to run tests. None understands the repository's
operational truth or safety model without a repository-owned capability layer.
The current
[`AGENTS.md`](https://github.com/sunkeunchoi/korea-adapter-sdk-ls/blob/ea0c076c5a67283b513aa38f2f4776fed8ccd565/AGENTS.md)
makes several rules architectural rather than advisory:

- `queue/items.jsonl` is the sole staging location for new and pre-staged
  operational work; changes must go through `lab-next add/done/supersede`, not
  direct edits;
- `make gate-run` carries resumable gate state, and the required test surface
  changes when SDK/core edits can reach the standalone Nautilus adapter or the
  morning script harness;
- TR lifecycle transitions require projected metadata/baselines and exact
  cross-check registration rather than a generic "tests passed" result;
- paper live smokes require named, gitignored credentials and explicit paper
  environment verification; and
- order work uses no-retry dispatch, deduplication, kill switch, redaction, a
  distinct success predicate, reconciliation, and an attended guarded paper
  order. It must never be inferred from an ordinary issue label that unattended
  mutation is authorized.

Therefore a GitHub issue cannot silently become the competing operational
queue. Triage should classify an issue as one of:

- ordinary repository work, for which the GitHub issue is canonical;
- operational/TR work, for which the Runner links or creates the authoritative
  queue item through `lab-next` and records that identity on the issue;
- credentialed/live/order work, routed to `ready-for-human`; or
- incomplete/duplicate/out-of-scope intake, left with a durable triage outcome.

Likewise, generic agent tests cannot stand in for `make gate-run`, Paper Live
Smoke evidence, or the order evidence matrix. Capability Contracts must name
the deterministic executor, expected evidence, allowed paths, risk class, and
approval boundary. GitHub, Orca, ACP, and whichever agent is selected carry
that contract; none is allowed to reinterpret it.

## Recommended state and control flow

```text
GitHub issue opened or edited
        |
        v
gh-aw triage workflow
  read-only analysis + bounded SafeOutputs
        |
        +--> needs-info / duplicate-candidate / ready-for-human
        |
        `--> ready-for-agent
                 |
                 v
Orca automation discovers candidate
                 |
                 v
Repository Improvement Runner
  atomic claim + risk/capability/queue reconciliation
                 |
                 v
one Orca card + one worktree + one branch
                 |
        +--------+---------+
        |                  |
   PTY adapter       ACP adapter
                     Conductor -> OpenCode or another ACP agent
        |                  |
        +--------+---------+
                 |
                 v
deterministic repo gates + credential-free evidence
                 |
                 v
draft PR / needs-human outcome / resumable failure
                 |
                 v
GitHub review and checks; human merge
```

State remains deliberately split by responsibility:

- **GitHub issue/PR:** canonical shared objective, triage decision, review and
  durable outcome;
- **Orca card/worktree/terminals:** live local execution resources and human
  visibility;
- **`queue/items.jsonl`:** canonical operational/TR sequencing where existing
  repository rules require it; and
- **minimal Runner checkpoint:** issue and attempt identity, capability version,
  Orca worktree/branch identity, current deterministic phase, terminal/ACP
  session references, pending approval, evidence pointers, failure reason, and
  exact resume command.

This is not duplicated Kanban. Each source owns facts the others cannot safely
reconstruct.

## Adopt / integrate / borrow / reject matrix

| Component | Decision | Use now | Do not give it |
| --- | --- | --- | --- |
| `gh-aw` issue triage | **Integrate** | GitHub `issues` event, duplicate/missing-info/risk classification, allowlisted labels/comments, pinned compiled workflow. | LS credentials, order authority, issue closure/`wontfix`, Orca worktree ownership. |
| `gh-aw` SafeOutputs and rollout model | **Borrow now; optionally integrate for remote low-risk tasks** | Read-only agent/write-job separation, draft PR, protected files, patch bounds, staged/shadow promotion. | Authority to bypass `make gate-run`, queue rules, or attended live gates. |
| OpenCode ACP agent | **Adopt as an optional Agent Adapter** | `opencode acp` behind the pinned Conductor/Proxy experiment; preserve `AGENTS.md`, skills, MCP and permissions. | Workflow/state ownership or automatic issue triage. |
| Open SWE | **Borrow; reject as canonical runtime while Orca is required** | Persistent sandbox identity, turn checkpoints, follow-up queue, approval fingerprints, outcome-driven reviewer learning. | A second dashboard/Kanban, canonical worktree state, LS credentials by default. |
| SWE-ReX | **Borrow/interface reserve** | Consider only if a later requirement proves Orca cannot supply remote isolated execution. | GitHub coordination or repository policy. |
| mini-SWE-agent | **Reject as default; retain as possible benchmark backend** | A small comparison agent for adapter tests if useful. | Triage, approvals, or capability migration. |
| OpenHands V0 Resolver | **Reject** | Historical lessons only. | A production dependency on deleted upstream code. |
| Sweep | **Reject** | None. | Production use or copied architecture under an incompatible license. |

## Smallest research-backed experiment

The lowest-risk validation is narrower than implementing a general recursive
improvement bot:

1. Install a pinned `gh-aw` triage workflow in report-only mode, then allow only
   `needs-info`, `ready-for-agent`, `ready-for-human`, and type/risk labels.
2. Create synthetic issues covering incomplete, duplicate, low-risk research,
   code change, live-smoke, and order scenarios. Verify the latter two can never
   reach unattended dispatch.
3. Let an Orca automation discover one `ready-for-agent` **research** issue and
   hand it to a minimal Runner checkpoint.
4. Complete it once through an ordinary Orca PTY agent and once through the ACP
   Conductor with unchanged OpenCode as the base agent.
5. Cancel and resume one attempt. The same GitHub issue, Orca worktree, branch,
   capability version, and evidence contract must survive while terminal/ACP
   session identities may change.
6. Produce a cited research artifact and draft PR or issue-resolution comment;
   no product code, credentials, live smoke, queue mutation, or automatic merge
   is needed for this proof.

This experiment distinguishes the layers cleanly. A triage failure challenges
the `gh-aw` intake policy; an Orca/run recovery failure challenges the Runner
boundary; an ACP-only failure rejects ACP as the portable adapter but leaves
the GitHub/Orca architecture intact.

## Direct answer

**Yes, open-source projects already perform substantial portions of the desired
flow.** `gh-aw` is the best current automatic GitHub issue-triage and guarded
write layer. Open SWE is the strongest current end-to-end internal coding-agent
service. OpenCode is the best inspected ACP-native base agent and has a useful
explicit-comment issue-to-PR Action.

**No, none is a drop-in Orca plugin or eliminates the Repository Improvement
Runner for this repository.** The right result is not to reimplement their
generic machinery. Use `gh-aw` at the GitHub boundary, use Orca for live local
execution, use an existing agent such as OpenCode behind PTY/ACP adapters, and
keep only the repository-specific coordination and safety semantics in the
Runner.
