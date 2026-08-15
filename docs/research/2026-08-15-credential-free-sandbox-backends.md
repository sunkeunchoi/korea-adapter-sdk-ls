# Credential-free sandbox backends for Orca agent execution

Research date: 2026-08-15 (Asia/Seoul)

## Verdict

Use a small backend interface in the future Repository Improvement Runner, but
validate only one local backend first:

1. **macOS/Apple Silicon:** falsify a pinned Apple `container` configuration.
   It gives each container its own Linux VM, explicit host mounts, CPU/memory
   and ulimit controls, interactive stdin/TTY support, and an internal
   host-only network. It is the cleanest match for this maintainer's current
   macOS 26.6.1 arm64 host.
2. **Linux:** prefer rootless Podman with no network, no host-proxy inheritance,
   an empty environment, explicit mounts, dropped capabilities, read-only root,
   and cgroup limits. Add gVisor only where its compatibility cost is justified;
   use Kata/microVM isolation only for a later multi-tenant Linux service.
3. **Remote optional lane:** GitHub Agentic Workflows (`gh-aw`) is the strongest
   examined packaged remote boundary because it combines ephemeral Actions
   runners, an egress firewall/API proxy, stronger runtime choices, and
   permission-separated safe outputs. It is not the Orca-visible local execution
   plane required by the current architecture, so it is an alternative lane,
   not the local backend.

No backend alone makes A1/A2 safe. The Runner still owns claims, exact attempt
identity, policy, cancellation, evidence, diff validation, and the A2 publisher.
In particular, Apple `container` can enforce **no egress** with an internal
network, but it does not provide a domain allowlist. A production agent that
needs a model API therefore needs a Runner-controlled, credential-injecting
egress broker on the host-only network. The agent must receive neither the real
model credential nor GitHub/LS credentials. A short-lived token is not
"credential-free" merely because it expires: its usable authority and theft
impact must be bounded to the autonomy contract.

The smallest experiment is intentionally secret-free and offline. It tests the
isolation substrate without pretending to solve model authentication. Backend
installation is a separate, explicitly authorized implementation step: on the
research host, `container`, Docker, Podman, Colima, and Lima are currently not
installed.

## Decision against the autonomy contract

The governing contract is [issue 284](https://github.com/sunkeunchoi/korea-adapter-sdk-ls/issues/284):
A1 permits edits/tests/commits only inside one assigned worktree after an
enforceable credential-free preflight; A2 adds a bounded branch push and draft
PR through a deterministic publisher; A3/A4 remain attended/operator-only.
Issue text, repository content, model output, and tool output are all untrusted.

| Backend or class | Preventive A1/A2 fit | Decision | Why |
|---|---|---|---|
| Apple `container` 1.2.2 | **Promising after falsification; not sufficient alone** | **Adopt for the first macOS experiment** | Per-container VM, explicit bind/volume/tmpfs mounts, resource controls, capabilities, read-only root, stdin/TTY, lifecycle API, and internal networking. Needs a private Git layout and external auth/egress broker for production. |
| Rootless Podman 6.1.0 on Linux | **Good with a hardened profile** | **Adopt as the Linux baseline later** | Rootless user namespace, OCI controls, `--network=none`, resource limits, PTY/stdin, and mature lifecycle. Shares the Linux kernel; configuration must defeat proxy/environment inheritance. |
| Podman Machine on macOS | **Possible but default is unsafe** | **Borrow only if Apple `container` fails** | The Linux VM is useful, but the documented default mounts `$HOME:$HOME`; a dedicated machine with no default volumes must be proven. More persistent state and cleanup burden than a VM per container. |
| Lima 2.2.0 / Colima 0.10.3 | **Building blocks, not the preferred policy boundary** | **Borrow, do not adopt directly** | Lima's builtin mount default is empty, but its default template mounts home read-only and propagates proxy variables; Colima mounts home writable by default. Both require a dedicated disposable VM profile plus a container policy inside it. |
| Moby/Docker Engine through Colima | **Technically possible, weaker operational fit** | **Reject for first choice** | Apache-2.0 Moby is mature, but Colima defaults expose home and a persistent rootful daemon expands the trusted control plane. Docker Desktop is not the requested fully open-source backend. |
| gVisor `runsc` | **Stronger Linux syscall boundary** | **Integrate optionally with Podman/AWF** | A userspace application kernel substantially reduces direct host-kernel surface and supports arm64/x86_64, but it is Linux-only and can break workloads using unimplemented syscalls. It still needs mount/network/cgroup policy. |
| Kata / Firecracker microVM | **Strong Linux isolation** | **Defer** | Hardware VM boundary and arm64 support, but Firecracker is a VMM rather than an execution policy; Kata supplies OCI integration at materially higher setup/operations cost. KVM/Linux only. |
| `gh-aw` AWF, gVisor, Docker sbx, or Cloud Hypervisor | **Strong remote A1/A2 package** | **Keep as an optional remote lane** | Egress proxy, API credential proxy, digest-pinned workflow compilation, ephemeral runner, and permission-separated safe outputs. On self-hosted runners AWF exposes `$HOME` read-write, so use ephemeral hosted runners or independently harden the host. Linux/Docker only; no Orca-local PTY/worktree. |
| SWE-ReX 1.4.0 | **Not a security boundary** | **Borrow its runtime abstraction only** | Useful uniform command/session/PTY/lifecycle API over Docker, Modal, Fargate and remote hosts. Its Docker backend accepts arbitrary `docker_args`; its local backend is explicitly only a wrapper. Policy remains the caller's job. |
| E2B infrastructure | **Capable hosted/multi-tenant substrate** | **Reject now; reconsider at scale** | Apache-2.0 Firecracker sandboxes, cgroups, network namespaces/firewall, PTY and pause/snapshot/resume are strong, but self-hosting requires a cloud control plane, Terraform/Nomad, databases, object storage and provider credentials. It does not support a general single Linux host. |
| Daytona | **No longer supportable as an OSS dependency** | **Reject** | The pinned README says the public repository became unmaintained in June 2026 and core development moved private; the pinned tree does not contain the linked license file. |
| Bubblewrap 0.11.2 | **Useful low-level Linux primitive** | **Borrow, not adopt alone** | Empty mount namespace, explicit binds, environment/network/PID/IPC isolation and `no_new_privs`; its own documentation says it is not a complete sandbox policy. It lacks integrated image, cgroup, evidence and remote lifecycle management. |
| Dedicated OS account | **Insufficient alone** | **Reject as a boundary; use as defense in depth** | File ownership can separate home/worktrees, but it does not by itself deny network, constrain syscalls/resources, remove inherited sockets/environment, or provide deterministic cleanup. Linux service namespaces/cgroups can repair this, but that becomes a custom sandbox. |

This comparison is about the **untrusted workload boundary**, not about which
agent model or ACP implementation runs. ACP stdio can run through a container's
stdin/stdout; a TTY is for human/TUI sessions and must not be enabled on a
machine-readable ACP stream because TTYs can merge or transform streams.

## The two seams that decide whether the design is real

### Git cannot use the host worktree's `.git` authority

An Orca worktree normally contains a `.git` *file* pointing into the primary
repository's common Git directory, typically `.git/worktrees/<name>`. Mounting
only the worktree either breaks Git because that target is absent or tempts the
implementation to mount the common `.git` directory. The latter exposes all
refs, remotes, worktree metadata, hooks/config, and broader mutation authority;
it violates the one-attempt boundary even if the source files mount is narrow.

The safe shape is an **attempt-local repository**:

```text
trusted Runner
  -> materialize exact base commit into a fresh clone or private bare mirror
  -> remove/replace remotes and all credential helpers
  -> run agent against its private worktree + private Git directory
  -> collect patch/commit bundle as untrusted output
  -> validate paths, base, diff, gates and policy outside the sandbox
  -> import into the Orca-owned branch/worktree
```

The first experiment should use a local bundle or archive, not clone over the
network. A private Git directory in an Apple named volume is acceptable if its
exact lifecycle is tied to the attempt. Directly mounting the Orca worktree is
useful only for a read/write filesystem demonstration; it is not the final A1
commit design. Symlink traversal from the mounted worktree must be tested.

### Model authentication is not solved by container isolation

An autonomous coding agent normally needs both network egress and model
authentication. Putting a provider key in the agent's environment merely moves
a secret into the sandbox. Putting an opaque token there is safe only if the
broker verifies that the token is attempt-bound, model/API-bound, narrowly
budgeted, non-replayable outside the broker, rapidly revocable, and incapable
of authorizing any repository or LS operation. Until that is demonstrated, it
is a credential and the lane is not credential-free.

Production should use a Runner-owned broker that:

- lives outside the agent VM, or on a separately trusted peer;
- is the only route from an internal network to the internet;
- injects the real provider credential after authenticating the exact attempt;
- allowlists protocol, destinations and request budgets and records metadata
  without logging secrets or model content unnecessarily;
- cannot be reconfigured by the agent; and
- revokes the attempt capability during cancellation before process cleanup.

Apple `container`'s `--internal` network is host-only, as shown by its own
[network mode](https://github.com/apple/container/blob/7d4ffb6cb1aed2c4eba42d5787de09162c82b591/Sources/ContainerResource/Network/NetworkMode.swift),
[CLI creation path](https://github.com/apple/container/blob/7d4ffb6cb1aed2c4eba42d5787de09162c82b591/Sources/ContainerCommands/Network/NetworkCreate.swift),
and [integration test](https://github.com/apple/container/blob/7d4ffb6cb1aed2c4eba42d5787de09162c82b591/Tests/IntegrationTests/Network/TestCLINetwork.swift).
It blocks direct internet access but does **not** implement domain egress rules.
An allowlist therefore needs the external broker/firewall. `gh-aw` already
implements this pattern with an internal Docker network, dual-homed Squid proxy,
and API proxy; that is the design to borrow, not necessarily its whole runtime.

A2 has the same rule: the sandbox emits a patch/bundle and a proposed PR body;
a deterministic Runner publisher alone holds GitHub authority, verifies the
issue/branch/repository and ruleset, then performs bounded push plus draft-PR
creation. The agent never receives a GitHub token or repository write remote.

## Capability comparison

| Criterion | Apple `container` | Rootless Podman | `gh-aw` hosted | SWE-ReX + hosted backend |
|---|---|---|---|---|
| Hosts / architecture | macOS 26+, Apple Silicon; Linux guest | Linux arm64/x86_64; macOS through a Linux VM | GitHub-hosted Linux; stronger runtimes have runner/KVM constraints | Controller is portable; actual backend determines architecture |
| Filesystem / home | Only explicit mounts for ordinary containers; do **not** use `container machine`, which maps host home automatically | Explicit container binds, but Podman Machine defaults to `$HOME:$HOME` | Hosted runner home is ephemeral; AWF nevertheless exposes runner home and workspace rw | Provider-defined; local mode has no isolation |
| Worktree / Git | Private Git volume/clone required; never mount common host `.git` | Same | Ephemeral checkout; safe-output publisher can own write credentials | Must be designed by caller |
| Environment / secrets | Outer `env -i`, pinned image and explicit `--env`; verify effective env. Run source propagates host `SSH_AUTH_SOCK` when present, so prove it absent | Outer `env -i`, controlled image env, `--http-proxy=false`; never mount engine socket | API proxy and safe-output separation are useful; audit workflow secrets/permissions | Provider/client credentials remain in controller; runtime args decide guest env |
| Network | Internal network gives deny-by-default; external proxy needed for allowlist | `--network=none`; external/internal proxy topology needed for allowlist | AWF domain allowlist and sole-proxy egress | Provider-defined; not enforced by SWE-ReX |
| Process / resource | Per-VM CPU/memory plus OCI ulimits, capability drops, readonly root | cgroup CPU/memory/PID limits, seccomp, `no-new-privileges`, capability drops | Job timeout, container memory, runtime isolation; workflow cancellation | Deployment timeout and provider limits; caller must verify hard enforcement |
| PTY / ACP stdio | `-it` PTY test exists; use `-i` without TTY for ACP stdio | `-it` for TUI; `-i` for ACP because Podman documents TTY stream-merging caveats | Log-oriented batch job, not interactive Orca ACP | Multi-session interactive command API/PTY; not ACP itself |
| Deterministic mounts | Strong if every mount is generated from validated absolute attempt paths | Strong with a generated argument vector and no Machine defaults | Workflow checkout/cache/action mounts; ephemeral but not Orca's live mount | Provider-specific |
| Cancel / cleanup | Named container, signal forwarding, `stop`, `--rm`; Runner must reconcile after client crash | Named container, stop timeout, kill/remove; reconcile engine inventory | Actions cancellation and artifacts | `stop()`/kill or provider API; semantics vary |
| Checkpoint / evidence | Named volume or controlled export; host Runner owns authoritative record | Volume/export plus host Runner record | Logs/artifacts; remote checkpoint is a new Runner adapter | Modal/E2B-style provider snapshots possible; local Docker is ephemeral |
| Performance / burden | Lightweight per-container VM; host-specific, new operational dependency | Low on Linux; extra persistent VM on macOS | Pay/queue latency, no local setup, remote logs | Adds Python service/runtime protocol plus provider |
| Offline / self-host | Yes after pinned image/toolchain are present | Yes after pinned image/toolchain are present | Self-hosted Linux possible, but hosted services/model still matter | Docker/local yes; hosted backends no |
| Orca observe / stop | Orca launches the exact CLI outer process and can stream stdout/stderr; Runner maps attempt ID to validated container name and reconciles via CLI/API | Same | Orca can trigger/watch/cancel a workflow, but it is not a local Orca terminal/worktree | Orca would talk to a custom adapter; not native ACP |

## Backend details and trust boundaries

### Apple `container`: best first falsification target

Apple describes `container` as an Apache-2.0 Swift tool for running OCI Linux
containers as lightweight VMs on Apple Silicon/macOS 26
([README](https://github.com/apple/container/blob/7d4ffb6cb1aed2c4eba42d5787de09162c82b591/README.md),
[technical overview](https://github.com/apple/container/blob/7d4ffb6cb1aed2c4eba42d5787de09162c82b591/docs/technical-overview.md)).
Each container gets its own VM, and host data is visible only through explicit
mounts. Bind mounts support read-only mode; named ext4 volumes and sized tmpfs
are available
([volumes](https://github.com/apple/container/blob/7d4ffb6cb1aed2c4eba42d5787de09162c82b591/docs/volumes.md)).
CPU and memory are per VM
([resource usage](https://github.com/apple/container/blob/7d4ffb6cb1aed2c4eba42d5787de09162c82b591/docs/resource-usage.md));
OCI ulimits include process, file-size, CPU and descriptor limits
([ulimits](https://github.com/apple/container/blob/7d4ffb6cb1aed2c4eba42d5787de09162c82b591/docs/ulimits.md)).
The runtime also exposes capability drop, masked/read-only paths, read-only root
and init controls
([runtime configuration](https://github.com/apple/container/blob/7d4ffb6cb1aed2c4eba42d5787de09162c82b591/docs/runtime-configuration.md)).
The repository tests `container run --rm -it` through a PTY
([terminal integration test](https://github.com/apple/container/blob/7d4ffb6cb1aed2c4eba42d5787de09162c82b591/Tests/IntegrationTests/Run/TestCLITermIO.swift)).
Do not substitute the persistent `container machine` feature: it automatically
maps the host username and home, with read-write home as its default
([container-machine documentation](https://github.com/apple/container/blob/7d4ffb6cb1aed2c4eba42d5787de09162c82b591/docs/container-machine.md)).
The ordinary run path also conditionally propagates host `SSH_AUTH_SOCK`, so
the outer process must start from a genuinely empty environment and the probe
must confirm the variable and socket are absent
([run source](https://github.com/apple/container/blob/7d4ffb6cb1aed2c4eba42d5787de09162c82b591/Sources/ContainerCommands/Container/ContainerRun.swift)).

Trusted components remain substantial: macOS, Virtualization.framework/vmnet,
the Apple API service and XPC/launchd helpers, the VMM, guest kernel, virtiofs,
image and Runner policy. A writable bind gives the guest exactly the invoking
user's authority on that path. The VM does not repair an overbroad mount.
Nested virtualization must remain disabled, no container engine or host sockets
may be mounted, and capabilities should be dropped rather than relying on the
runtime's default set.

Lifecycle is observable enough for Orca: launch an argv (not a shell string)
with a unique validated name, stream the outer process, send graceful
cancellation, then use the [stop command](https://github.com/apple/container/blob/7d4ffb6cb1aed2c4eba42d5787de09162c82b591/Sources/ContainerCommands/Container/ContainerStop.swift)
and exact-name reconciliation. `--rm` covers normal exit; crash recovery must
enumerate only labels/names minted for the attempt and refuse ambiguous cleanup.

### Podman, Lima and Colima

Podman is Apache-2.0, daemonless on Linux, and supports rootless user namespaces
([README](https://github.com/podman-container-tools/podman/blob/a2409076ef2fef60ad9ac046375dedc7d9410ef4/README.md)).
Its primary options document no-network namespaces
([network](https://github.com/podman-container-tools/podman/blob/a2409076ef2fef60ad9ac046375dedc7d9410ef4/docs/source/markdown/options/network.md)),
explicit bind/tmpfs mounts
([mount](https://github.com/podman-container-tools/podman/blob/a2409076ef2fef60ad9ac046375dedc7d9410ef4/docs/source/markdown/options/mount.md)),
memory/PID controls
([memory](https://github.com/podman-container-tools/podman/blob/a2409076ef2fef60ad9ac046375dedc7d9410ef4/docs/source/markdown/options/memory.md),
[PID limit](https://github.com/podman-container-tools/podman/blob/a2409076ef2fef60ad9ac046375dedc7d9410ef4/docs/source/markdown/options/pids-limit.md)),
and `no-new-privileges`/seccomp
([security options](https://github.com/podman-container-tools/podman/blob/a2409076ef2fef60ad9ac046375dedc7d9410ef4/docs/source/markdown/options/security-opt.md)).
Host HTTP proxy variables are passed by default, so the hardened profile must
set `--http-proxy=false`
([proxy option](https://github.com/podman-container-tools/podman/blob/a2409076ef2fef60ad9ac046375dedc7d9410ef4/docs/source/markdown/options/http-proxy.md)).

On macOS Podman requires a managed Linux VM. Its machine documentation states
that default volumes come from `containers.conf` and default to `$HOME:$HOME`
([machine init](https://github.com/podman-container-tools/podman/blob/a2409076ef2fef60ad9ac046375dedc7d9410ef4/docs/source/markdown/podman-machine-init.1.md.in)).
That default fails before the agent starts. A fallback experiment would need an
isolated configuration root, a newly created dedicated machine, an effective-
configuration assertion proving no home/default mounts, and full disposal.

Lima's builtin default mounts nothing, forwards neither SSH agent nor X11, and
supports CPU/memory/disk controls, but its shipped default template mounts home
read-only and propagates proxy variables unless disabled
([default template](https://github.com/lima-vm/lima/blob/09903953eb78ac9a1a9d9d8f1eb0f6b6b85a0e5f/templates/default.yaml)).
Colima is an approachable Lima/container-runtime wrapper, but its own default
configuration says `$HOME` is mounted writable
([configuration](https://github.com/abiosoft/colima/blob/c3a5f9184d83a197184f897a9f07eb3c01b3bc88/embedded/defaults/colima.yaml)).
Neither default is acceptable; both can only be considered with a dedicated
generated profile and adversarial verification.

### Stronger Linux isolation

gVisor is an Apache-2.0 userspace application kernel, not a VM or syscall
filter. It reimplements the Linux interface in memory-safe Go and does not pass
workload syscalls directly to the host
([architecture](https://github.com/google/gvisor/blob/50e1502a95d36ad2faf2c7ef33b8bf21fe975293/g3doc/architecture_guide/intro_to_gvisor.md)).
Its rootless modes have meaningful networking, cgroup and checkpoint limits,
which must be evaluated rather than assumed
([rootless guide](https://github.com/google/gvisor/blob/50e1502a95d36ad2faf2c7ef33b8bf21fe975293/g3doc/user_guide/rootless.md)).

Firecracker is an Apache-2.0 KVM VMM for lightweight microVMs and explicitly
requires a correctly configured Linux host
([README](https://github.com/firecracker-microvm/firecracker/blob/ea50487ec11602100b90ed63f85fe00bd30fbde8/README.md)).
Kata is an Apache-2.0 OCI/containerd integration that runs containers in VMs and
supports x86_64 and arm64 hardware virtualization
([README](https://github.com/kata-containers/kata-containers/blob/9178ca41436ce28198e77f80423f27252350dfe6/README.md)).
They are stronger tenants but not macOS backends; building images, kernels,
network policy, snapshots, cleanup and evidence remains the Runner/operator's
job.

Bubblewrap creates an empty mount namespace and can unshare user, PID, IPC and
network namespaces, clear the environment, bind only selected paths, and die
with the parent. Its LGPL-2.0 source explicitly says it is a construction tool,
not a complete sandbox policy
([README](https://github.com/containers/bubblewrap/blob/2f55bae38468d0c50cf5df87b1e481e882b63acb/README.md),
[license](https://github.com/containers/bubblewrap/blob/2f55bae38468d0c50cf5df87b1e481e882b63acb/COPYING)).
It is useful under a dedicated Linux service account/cgroup, but Podman packages
more of the required lifecycle.

### Remote and abstraction choices

`gh-aw` is MIT-licensed and actively released. Its security architecture
separates substrate, declarative configuration and execution plan; the agent
runs read-only while trusted safe-output jobs perform filtered writes
([architecture](https://github.com/github/gh-aw/blob/19e875b8b03b9540159c8f8549954a86ab6c2446/docs/src/content/docs/introduction/architecture.mdx)).
AWF offers Docker, gVisor, Docker sbx microVM and preview Cloud Hypervisor
runtimes, a sole-proxy egress network and API proxy. The same documentation says
AWF's chroot compatibility mode mounts host home and `/tmp` read-write. This is
acceptable on a fresh hosted runner, not an unattended persistent maintainer
host. Self-hosted support requires Linux and Docker; macOS/Windows are not
supported
([self-hosted runners](https://github.com/github/gh-aw/blob/19e875b8b03b9540159c8f8549954a86ab6c2446/docs/src/content/docs/reference/self-hosted-runners.md)).

SWE-ReX is MIT-licensed and provides a useful provider-neutral runtime API with
parallel interactive sessions
([README](https://github.com/SWE-agent/SWE-ReX/blob/5c995c365dfb1fd5bc56fda688be5d8538f9931f/README.md)).
Its [Docker deployment](https://github.com/SWE-agent/SWE-ReX/blob/5c995c365dfb1fd5bc56fda688be5d8538f9931f/src/swerex/deployment/docker.py)
starts an authenticated remote runtime and accepts caller-provided Docker args;
its [local deployment](https://github.com/SWE-agent/SWE-ReX/blob/5c995c365dfb1fd5bc56fda688be5d8538f9931f/src/swerex/deployment/local.py)
adds no isolation. Adopt the idea of a runtime interface, not SWE-ReX as proof
of policy.

E2B's Apache-2.0 infrastructure runs each sandbox as a Firecracker microVM with
cgroups, network namespace/firewall, copy-on-write storage and pause/checkpoint
support
([architecture](https://github.com/e2b-dev/infra/blob/8446adb5081b324ac32124a860195c0ff0cd06fc/docs/ARCHITECTURE.md)).
The self-host guide supports GCP and beta AWS, not a general Linux machine, and
requires a substantial cloud stack and credentials
([self-hosting](https://github.com/e2b-dev/infra/blob/8446adb5081b324ac32124a860195c0ff0cd06fc/self-host.md)).
The controller credential can remain outside the guest, but repository import,
model auth, egress policy and evidence export still require an Orca adapter.

## Smallest adversarial isolation experiment

This is a future implementation experiment, not performed during this research.
Installing a backend and downloading an image are separate authorized setup
steps. Pin Apple `container` 1.2.2 and a minimal arm64 toolchain image by digest;
record the binary and image digests in the Attempt Record.

### Fixture

The trusted harness creates paths under one newly allocated temporary root:

- `forbidden/` with unique canaries standing in for home, primary repository,
  sibling worktree, `.ssh`, `.config`, LS env files and Git credential config;
- `attempt/repo/`, an independent repository materialized from a local bundle at
  an exact commit, with no network remote, credential helper, hooks or reference
  to the host common Git directory;
- a private Git directory/volume and bounded scratch/output locations; and
- a host-side append-only event log that is never mounted into the guest.

The sandbox receives only the attempt repository read-write, private Git state,
bounded scratch/tmpfs and an output directory. It starts with an empty host
environment, an audited image environment, read-only root, all capabilities
dropped, no nested virtualization, CPU/memory/PID/file-size/descriptor limits,
and an internal network with no proxy. No real secret or provider credential is
used; canaries make leakage detectable without creating impact.

### One hostile payload

Run a single version-controlled probe that attempts all of the following and
emits structured results to stdout:

1. enumerate environment variables, mounts, devices, processes, Unix sockets,
   home-like paths and the host canary names;
2. read and overwrite every forbidden canary, the primary repository and a
   sibling path, including through absolute and `..` paths;
3. follow pre-created and newly created symlinks from the writable repository
   to forbidden paths;
4. inspect `.git`, Git config/hooks/remotes/credential helpers and `commondir`,
   then create one allowed commit in the private repository;
5. connect by DNS name and literal public IPv4/IPv6, probe host gateway/control
   ports, and verify only loopback/internal peers exist;
6. inspect/kill a host canary process, ptrace, mount, write `/proc`/`/sys`, open
   `/dev/kvm`, reach an SSH agent or container-control socket, and create a
   nested namespace/container;
7. exhaust processes, memory, CPU, descriptors, file size and scratch space;
   each must hit the declared ceiling without destabilizing the host; and
8. start a child/grandchild that ignores graceful termination so the harness can
   test escalation and orphan cleanup.

Then run two lifecycle cases: normal `--rm` completion and simulated Orca/CLI
client crash. In both, the Runner revokes the attempt, sends graceful stop,
escalates after the declared threshold, reconciles the exact attempt name, and
verifies no VM/container/process/network/volume residue except intentionally
preserved evidence. Cleanup must stop rather than broaden scope if identity is
ambiguous.

### Pass bar

Pass only when all forbidden reads/writes and direct egress fail, host canaries
and primary repository are byte-identical, resource limits trip at their exact
class, no host process/socket is reachable, ACP-style stdin/stdout and a
separate PTY session both work, the private Git commit contains only the allowed
path, cancellation leaves no live descendant, and the trusted harness can
export a deterministic patch/bundle plus logs. Any unexpected visibility,
uncertain mount, surviving process, residual network, or unbounded resource is
a **policy violation**, not a flaky test to retry.

This experiment proves only the offline substrate. A second, separately
reviewed experiment must attack the model-auth/egress broker before any
networked A1 run. A third must attack the deterministic A2 publisher before an
autonomous push or draft PR.

## Pinned source snapshot

| Project | Pin / release at research time | License / maintenance signal |
|---|---|---|
| Apple `container` | [`7d4ffb6`](https://github.com/apple/container/commit/7d4ffb6cb1aed2c4eba42d5787de09162c82b591), [1.2.2](https://github.com/apple/container/releases/tag/1.2.2) | Apache-2.0; release 2026-08-08 |
| Apple Containerization | [`5427fd2`](https://github.com/apple/containerization/commit/5427fd21ded4b84034126caef5b3182900b4776d) | Apache-2.0; library underneath the CLI, not a separate Runner choice |
| Podman | [`a240907`](https://github.com/podman-container-tools/podman/commit/a2409076ef2fef60ad9ac046375dedc7d9410ef4), [6.1.0](https://github.com/podman-container-tools/podman/releases/tag/v6.1.0) | Apache-2.0; release 2026-08-12 |
| Lima | [`0990395`](https://github.com/lima-vm/lima/commit/09903953eb78ac9a1a9d9d8f1eb0f6b6b85a0e5f), [2.2.0](https://github.com/lima-vm/lima/releases/tag/v2.2.0) | Apache-2.0; release 2026-07-21 |
| Colima | [`c3a5f91`](https://github.com/abiosoft/colima/commit/c3a5f9184d83a197184f897a9f07eb3c01b3bc88), [0.10.3](https://github.com/abiosoft/colima/releases/tag/v0.10.3) | MIT; release 2026-06-04 |
| Moby/Docker Engine | [`7940443`](https://github.com/moby/moby/commit/794044356570ee606a27cfdbd73815a37ea66c53), [29.7.2](https://github.com/moby/moby/releases/tag/docker-v29.7.2) | Apache-2.0; release 2026-08-06 |
| `gh-aw` | [`19e875b`](https://github.com/github/gh-aw/commit/19e875b8b03b9540159c8f8549954a86ab6c2446), [0.86.2](https://github.com/github/gh-aw/releases/tag/v0.86.2) | MIT; release 2026-08-11 |
| gVisor | [`50e1502`](https://github.com/google/gvisor/commit/50e1502a95d36ad2faf2c7ef33b8bf21fe975293) | Apache-2.0; active rolling releases |
| Firecracker | [`ea50487`](https://github.com/firecracker-microvm/firecracker/commit/ea50487ec11602100b90ed63f85fe00bd30fbde8), [1.16.1](https://github.com/firecracker-microvm/firecracker/releases/tag/v1.16.1) | Apache-2.0; release 2026-07-02 |
| Kata Containers | [`9178ca4`](https://github.com/kata-containers/kata-containers/commit/9178ca41436ce28198e77f80423f27252350dfe6), [4.0.0](https://github.com/kata-containers/kata-containers/releases/tag/4.0.0) | Apache-2.0; release 2026-07-20 |
| SWE-ReX | [`5c995c3`](https://github.com/SWE-agent/SWE-ReX/commit/5c995c365dfb1fd5bc56fda688be5d8538f9931f), [1.4.0](https://github.com/SWE-agent/SWE-ReX/releases/tag/v1.4.0) | MIT; latest tagged release 2025-08-14, commits continued in 2026 |
| E2B infrastructure | [`8446adb`](https://github.com/e2b-dev/infra/commit/8446adb5081b324ac32124a860195c0ff0cd06fc), [2026.29](https://github.com/e2b-dev/infra/releases/tag/2026.29) | Apache-2.0; release 2026-07-28 |
| Daytona | [`ec4c21b`](https://github.com/daytonaio/daytona/commit/ec4c21b2d597091ac09ecc278f3bcc172575a987), [0.190.0](https://github.com/daytonaio/daytona/releases/tag/v0.190.0) | Public repository explicitly unmaintained; linked license absent from pinned tree |
| Bubblewrap | [`2f55bae`](https://github.com/containers/bubblewrap/commit/2f55bae38468d0c50cf5df87b1e481e882b63acb), [0.11.2](https://github.com/containers/bubblewrap/releases/tag/v0.11.2) | LGPL-2.0; release 2026-04-23 |

## Recommendation to the architecture tickets

Keep the Runner; no examined backend eliminates it. Define its eventual sandbox
port around attempt identity and observable operations (`prepare`, `start`,
`signal`, `stop`, `inspect`, `collect`, `destroy`, `verify-absent`), but do not
generalize implementation before the Apple experiment passes. Orca should
create and display the outer process/terminal and worktree card; the Runner
should translate that intent into an exact sandbox instance and maintain the
authoritative checkpoint. The backend is a replaceable enforcement mechanism,
not the state machine.

Start with the fully offline Apple test. If it passes, design the auth/egress
broker and attempt-local Git import/export as separate reviewed seams. Only
after those pass should A1 become unattended. Add the Linux rootless Podman
profile when there is a real Linux deployment target. Keep `gh-aw` available
for explicitly remote jobs where losing live Orca-local execution is an
acceptable tradeoff. Do not introduce SWE-ReX, E2B, DeepSeek Harness, or a
microVM control plane merely to wrap the first local backend.
