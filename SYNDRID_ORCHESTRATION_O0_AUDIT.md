# Syndrid O0 Local Codex Integration Audit

## 1. Audit metadata

- Status: O0 local integration audit complete
- Branch: `phase-4/syndrid-ui-rework`
- Commit: `9e27a8843d8140a1b986fb9e78aa431a75d6c20b`
- Commit subject: `docs: define Syndrid orchestration architecture`
- Date: 2026-07-21
- Scope: static inspection of the local Syndrid/Codex fork; no implementation
- External repositories inspected: none
- Worktree status: pre-existing unrelated Bazel/build and `justfile` changes
  were present before this audit and were left untouched.

Evidence labels used below:

- **Verified**: supported by inspected local source, tests, or schemas.
- **Inference**: follows from verified source but is not itself an existing API.
- **Recommendation**: proposed O0/O1 direction.
- **Unresolved**: requires a later implementation audit or product decision.

## 2. Executive conclusion

**Verified:** Codex already contains the runtime primitives Syndrid must reuse:
`ThreadManager`, `CodexThread`, `AgentControl`, `AgentRegistry`, the V2
`spawn_agent` tool, `AgentGraphStore`, protocol thread/turn events, technical
sandbox policies, and rollout/thread persistence. The existing child-agent
implementation is separate forked threads with parent identity and structured
inter-agent messages; it is not a reason to create a second agent runtime.

**Verified:** The principal integration limitation is visibility and ownership.
The most useful scheduling and child-control surface is `pub(crate)` inside
`codex-core`, especially `AgentControl`. The app-server has public thread and
turn APIs, but using them as a workflow scheduler would not automatically give
Syndrid the same root-tree control, permission lineage, usage attribution, or
recovery semantics.

**Recommendation:** O1 should be a behavior-free typed domain model in a new
leaf orchestration crate or equivalent low-dependency module. It should not
execute agents, add TUI behavior, add app-server methods, or alter Codex core
execution. Later execution integration should use a narrow core-facing adapter
to native `AgentControl`/`ThreadManager`, with workflow IDs and policy checks
outside the existing model/tool runtime.

**Highest compatibility risk:** accidentally routing delegated work through
independent app-server sessions or a new runtime, which would duplicate context,
persistence, approvals, cancellation, and usage truth.

**O1 decision:** GO for the typed model only. NO-GO for Planner/Executor/
Verifier execution until the adapter, permission intersection, workflow event
attribution, and recovery seams are designed and tested.

## 3. Verified architecture map

```text
TUI / CLI composition roots
        |
        v
app-server v2 protocol and client       (wire-facing control surface)
        |
        v
codex-core ThreadManager / CodexThread / Session
        |
        +--> AgentControl / AgentRegistry / native child threads
        +--> tools, approvals, sandbox, MCP, model execution
        +--> rollout, thread-store, state, agent-graph-store
```

**Verified:** The normal TUI submission path is:

```text
AppCommand::UserTurn
  -> ThreadRouting::try_submit_active_thread_op_via_app_server
  -> AppServerSession::turn_start
  -> ClientRequest::TurnStart / TurnStartParams
  -> app-server TurnProcessor::turn_start_inner
  -> Codex thread/session execution
  -> thread/turn/item/token notifications
```

Supporting source: [`codex-rs/tui/src/app/thread_routing.rs`](codex-rs/tui/src/app/thread_routing.rs),
[`codex-rs/tui/src/app_server_session.rs`](codex-rs/tui/src/app_server_session.rs),
[`codex-rs/app-server/src/request_processors/turn_processor.rs`](codex-rs/app-server/src/request_processors/turn_processor.rs),
and [`codex-rs/app-server-protocol/src/protocol/v2/turn.rs`](codex-rs/app-server-protocol/src/protocol/v2/turn.rs).

**Verified:** Native delegated execution is:

```text
model tool call spawn_agent
  -> multi_agents_v2::spawn::Handler
  -> AgentControl::spawn_agent_with_communication
  -> ThreadManager / child Codex thread
  -> InterAgentCommunication and SubAgentActivity events
  -> parent wait/send/close/resume operations
```

Supporting source: [`codex-rs/core/src/tools/handlers/multi_agents_v2/spawn.rs`](codex-rs/core/src/tools/handlers/multi_agents_v2/spawn.rs),
[`codex-rs/core/src/tools/handlers/multi_agents.rs`](codex-rs/core/src/tools/handlers/multi_agents.rs),
and [`codex-rs/core/src/agent/control.rs`](codex-rs/core/src/agent/control.rs).

## 4. Workspace and crate dependency map

**Verified:** Relevant dependency direction from the Cargo manifests is:

```text
codex-protocol
   ^
   |-- codex-state ---> codex-thread-store ---> codex-rollout
   |-- codex-agent-graph-store
   |-- codex-config
   |-- codex-sandboxing / codex-tools / model crates
   v
codex-core
   ^                 ^
   |                 |
codex-app-server   codex-tui (via app-server-client/protocol, not core)
   ^                 ^
   +------ codex-cli composition root ------+
```

Key manifests: [`codex-rs/Cargo.toml`](codex-rs/Cargo.toml),
[`codex-rs/core/Cargo.toml`](codex-rs/core/Cargo.toml),
[`codex-rs/app-server/Cargo.toml`](codex-rs/app-server/Cargo.toml),
[`codex-rs/app-server-protocol/Cargo.toml`](codex-rs/app-server-protocol/Cargo.toml),
[`codex-rs/tui/Cargo.toml`](codex-rs/tui/Cargo.toml),
[`codex-rs/thread-store/Cargo.toml`](codex-rs/thread-store/Cargo.toml),
[`codex-rs/state/Cargo.toml`](codex-rs/state/Cargo.toml), and
[`codex-rs/agent-graph-store/Cargo.toml`](codex-rs/agent-graph-store/Cargo.toml).

**Verified ownership:** core owns live session/thread execution and model/tool
execution; `codex-protocol` owns shared IDs, events, token structures, and
configuration-facing protocol types; app-server-protocol owns wire contracts;
thread-store/rollout/state own persistence; agent-graph-store owns persisted
spawn topology; TUI owns presentation projections and event routing; CLI is a
composition root.

**Recommendation:** A full orchestration crate that directly depends on core
would either force core to depend back on it or make the crate depend on
crate-private internals. Prefer a low-dependency domain crate first, followed
by a narrow integration adapter at a legal composition boundary. This follows
the repository instruction to resist adding policy/domain code to `codex-core`.

## 5. Threads, turns, and child-agent findings

**Verified:** `AgentControl` is a shared, crate-private control handle carrying
session identity, a weak thread-manager state, `AgentRegistry`, V2 residency,
an `AgentExecutionLimiter`, and shared `RolloutBudget` (`core/src/agent/control.rs`,
roughly lines 85-126). It can send input, send inter-agent communication,
interrupt an agent, read status, and manage the live subtree.

**Verified:** `ThreadManager` exposes thread start, resume, fork, and bounded
shutdown paths (`core/src/thread_manager.rs`, approximately lines 656-837 and
919-950). `spawn_subagent` materializes/flushes the parent rollout, loads
history, and starts a child from a fork snapshot. Child context is therefore a
forked history boundary, not a full automatic transcript copy at every event.

**Verified:** V2 `spawn_agent` accepts a task message, task name, optional role,
model, reasoning effort, service tier, and fork-history selection. Non-full
history forks apply requested model/effort overrides; full-history forks reject
those overrides (`core/src/tools/handlers/multi_agents_v2/spawn.rs`, roughly
lines 40-131).

**Verified:** Parent/child identity uses `ThreadId`, `SessionSource::SubAgent`
with `ThreadSpawn` metadata, parent/fork fields, agent paths, and persisted
`thread_spawn_edges`. Child output returns through `InterAgentCommunication`
and collaboration activity items.

**Verified:** `AgentRegistry` enforces spawn slots and depth checks, and
`AgentGraphStore` persists open/closed parent-child edges. These are useful
native limits, but they are not the Syndrid policy of global maximum
concurrency 2 and one writer.

**Unresolved:** Independent child cancellation propagation, workflow-stage
attribution, and a public external scheduler API are not established by the
existing code. Recursive spawning is possible through native collaboration
paths subject to existing depth/limit controls; Syndrid still needs an
explicit orchestration-level delegation ceiling.

## 6. Model and reasoning-effort routing findings

**Verified:** `ThreadConfigSnapshot` contains resolved model, provider, service
tier, reasoning effort, permissions, parent/fork IDs, and source metadata.
`CodexThreadSettingsOverrides` can request model and effort changes, and
`preview_thread_settings_overrides`/session settings updates resolve them
(`codex-rs/core/src/codex_thread.rs`, approximately lines 60-159 and 356-410).

**Verified:** App-server v2 exposes model/provider/service-tier/effort fields on
thread start, resume, fork, and settings update. It returns resolved model and
effort in relevant responses (`codex-rs/app-server-protocol/src/protocol/v2/thread.rs`).

**Verified:** The narrowest existing per-child routing point is the native V2
spawn path, where `SpawnAgentArgs` is translated into child configuration. The
TUI’s ordinary path passes requested model and effort into `TurnStartParams`.

**Inference:** A future Syndrid role router can express requested model/effort
using these existing fields, but must distinguish requested values from actual
resolved values and logged fallback/reroute events.

**Unresolved:** A public API for applying a role-specific model/effort while
retaining native root-tree control does not exist. Fallback behavior must be
observed from `ModelRerouteEvent` and response/config data, not inferred from
the request.

## 7. Token, context, and usage findings

**Verified:** `codex_protocol::TokenUsage` has input, cached input, output,
reasoning output, and total fields. `TokenUsageInfo` separates cumulative
`total_token_usage` from incremental `last_token_usage` and includes context
window data (`codex-rs/protocol/src/protocol.rs`, approximately lines
2032-2109).

**Verified:** `TokenCountEvent` may include usage and `RateLimitSnapshot`.
Rate-limit windows expose used percentage and optional reset timestamps. The
session updates and persists token information and shared rollout-budget usage
in `core/src/session/mod.rs`, approximately lines 3668-3795.

**Verified:** App-server `ThreadTokenUsage` and raw response completion carry
thread/turn usage, including cached and reasoning fields
(`codex-rs/app-server-protocol/src/protocol/v2/thread.rs`, approximately lines
1380-1435). TUI converts thread notifications into its token display in
`codex-rs/tui/src/chatwidget.rs` and routes token notifications by thread.

**Verified:** Exact runtime/provider usage is available when present. Derived
values include non-cached input, blended usage, context percentage, and
remaining workflow budget. Baselines, forecasts, speedups, future demand, and
similar-task cost remain Estimated. Missing provider fields are Unavailable.

Double-counting risks:

1. `append_last_usage` expects an incremental last usage; appending a cumulative
   provider total duplicates history.
2. Cached input must not be added again to an input value already including it;
   use the explicit `non_cached_input` calculation where appropriate.
3. `total_tokens` is not interchangeable with billable input/output and must
   not be summed as another cost dimension.
4. Resumed/forked state is seeded from persisted token-count state; replaying
   historical cumulative events and then adding the current cumulative total
   duplicates usage.
5. Account-level lifetime usage and thread-level usage have different scopes.

**Unresolved:** There is no verified workflow/stage/agent usage ledger. Child
threads can expose their own thread usage, but aggregation and orchestration
overhead attribution must be added without replacing Codex’s accounting truth.

## 8. Event and correlation findings

**Verified:** `codex_protocol::Event` has an `id` described as a submission
correlation ID and contains `EventMsg`. `EventMsg` includes turn lifecycle,
raw response, item, tool, error, token, model-reroute, cancellation, and
collaboration variants (`codex-rs/protocol/src/protocol.rs`). Collaboration
items carry sender/receiver or `agent_thread_id`/`agent_path` identifiers
(`codex-rs/protocol/src/items.rs`).

**Verified:** App-server notifications add thread and turn identifiers to
thread lifecycle, item, token, and collaboration notifications. TUI routing
targets notifications by thread ID (`codex-rs/tui/src/app/app_server_event_targets.rs`).

**Verified:** No generic orchestration sequence number, causation ID, or
correlation ID envelope was found on the core `Event`. Event persistence and
replay are provided through rollouts/thread state, but a separate append-only
workflow event log is not present.

**Recommendation:** Use existing `ThreadId`, `TurnId`, core submission event
ID, and agent path as references. Add only `WorkflowId`, `TaskId`, and an
orchestration `AgentId` when workflow state begins. If an orchestration event
envelope is later added, it should reference Codex events rather than copy
their payloads, and should add sequence/causation fields only at that layer.

## 9. Cancellation, failure, and retry findings

**Verified:** App-server `turn_interrupt` accepts thread and turn IDs and
submits `Op::Interrupt`; core session cancellation uses cancellation tokens and
aborts active tasks. Thread-manager shutdown is bounded and reports completed,
submission-failed, and timed-out threads. Interrupted fork snapshots can persist
an aborted boundary.

**Verified:** Agent status maps turn started/completed/aborted/error/shutdown
events to `PendingInit`, `Running`, `Completed`, `Interrupted`, `Errored`, and
`Shutdown` (`codex-rs/core/src/agent/status.rs`).

**Unresolved:** This audit did not establish a complete inventory of every
transport/model/tool retry. Orchestration must not duplicate retries until each
native retry path is specifically classified. No persisted workflow repair
counter or stage retry state exists.

| Failure | Existing evidence | O0 implication |
|---|---|---|
| Planner/explorer/executor/verifier failure | Native turn/agent error or abort | Workflow stage classification is missing |
| Tool failure | Tool/command/error events and exit status | Reuse evidence; do not retry blindly |
| Model/transport failure | Error/stream-error/reroute events | Separate provider retry from workflow retry |
| User cancellation | `TurnAborted`, interrupt path, cancellation token | Workflow cancellation state must reference turn IDs |
| Process interruption | Bounded shutdown and rollout boundaries | Resume needs explicit workflow state |
| Restart after interruption | Thread resume and persisted graph metadata | No persisted workflow reconstruction yet |

**Recommendation:** Initial repair should be a workflow policy, not a second
Codex retry loop. It must have a hard counter and be triggered only by observed
verification evidence.

## 10. Permission, approval, and sandbox findings

**Verified:** `SandboxPolicy` includes technically meaningful read-only and
workspace-write policies. Permission profiles are converted into sandbox
command arguments in `codex-rs/sandboxing/src/landlock.rs`. Thread start,
settings update, and turn parameters carry approval, sandbox, and permission
fields.

**Verified:** Native child agents inherit effective provider, approval policy,
sandbox, and working directory before role-specific configuration. Approval
requests and permission conversion have app-server tests, and the TUI has an
approval overlay with Syndrid branding gates.

**Unresolved:** A general public child/workflow permission intersection is not
available. Workflow and role ceilings, parent cancellation lineage, and
technical read-only capability for an externally assigned Explorer require a
core-facing adapter or native extension. Role instructions alone are prompt
policy, not enforcement.

**Recommendation:** Enforce
`user ∩ session ∩ workflow ∩ parent ∩ role ceiling ∩ task request ∩ runtime`
before invoking Codex. Never construct a child configuration that widens the
parent. Use native sandbox/permission capability for read-only roles where
possible; expose any prompt-only restriction as a security limitation.

## 11. Persistence, resume, and recovery findings

**Verified:** `ThreadStore` persists threads, turns/items, rollout metadata,
history, and thread metadata; `StateRuntime` backs SQLite state; rollout files
hold session history. `AgentGraphStore` persists thread-spawn edges and
`restore_v2_agent_metadata` reconstructs open descendants on resume.

**Verified:** Thread resume/read/list and fork APIs exist in app-server v2.
Thread snapshots preserve parent/fork/source metadata. No workflow, stage,
bounded handoff, verification-evidence, budget-attribution, or repair-counter
store was found.

| Recovery situation | Current Codex state | Missing Syndrid state |
|---|---|---|
| Clean completion | Durable thread/turn/rollout records | Workflow completion and evidence summary |
| User cancellation | Aborted turn and cancellation path | Workflow cancellation and resumability policy |
| Model/tool failure | Error/tool events and persisted history as applicable | Stage failure and retry classification |
| Process crash | Partial rollout/turn boundary may be resumable | Atomic workflow checkpoint |
| Machine restart | Thread and child graph can be resumed | Workflow/task/budget reconstruction |
| Partial workflow | No workflow state | Completed-stage and pending-stage state |
| Executor done, verifier pending | No native workflow distinction | Persisted verifier requirement and handoff |

**Recommendation:** Persist a small separate orchestration record or extend an
existing state abstraction deliberately. Store workflow metadata, task/stage
state, policy snapshot, bounded handoff/evidence references, budget attribution,
verification results, and retry counts. Reference Codex threads/turns; do not
copy complete rollouts.

## 12. Verification-evidence findings

**Verified:** `codex-rs/tui/src/syndrid_live_state.rs` is presentation-only and
already defines `DataQuality`, `VerificationStatus`, `VerificationItem`,
`WorkflowUsage`, bounded activity, and validation projections. It explicitly
requires producers to use existing app-server/Codex notifications and avoids
execution or per-frame network/Git work.

**Verified:** Existing command/tool events expose exit status and outputs;
TUI projections expose file changes, activity, and verification rows. Existing
tests cover rendering bounds, missing-value em dashes, and Syndrid status.

**Unresolved:** A durable evidence object associated with a workflow task or
agent does not exist. Current success can be represented by observed events,
but a future Verifier needs persisted evidence references and must not turn an
agent claim into `Passed`.

**Recommendation:** Model evidence as bounded references to command results,
diff/repository state, tests, build/lint/snapshot results, exit codes, and
timestamps. Keep `Passed`, `Failed`, `Blocked`, and `Inconclusive` distinct.

## 13. App-server and protocol findings

**Verified:** v2 already exposes thread start/resume/fork/read, turn start/
interrupt/steer, settings update, collaboration items, thread status, token
usage, and account rate-limit notifications. Relevant processors are in
`codex-rs/app-server/src/request_processors/thread_processor.rs` and
`turn_processor.rs`; wire types are under
`codex-rs/app-server-protocol/src/protocol/v2/`.

| Potential API | O0 classification |
|---|---|
| Workflow start/read/cancel | Internal-only initially |
| Recommendation preview | TUI-local or internal initially |
| Mode configuration | TUI-local/internal until semantics stabilize |
| Agent status/progress | Internal projection first; experimental v2 later if needed |
| Budget status | Internal projection first; do not replace thread token APIs |
| Verification evidence | Internal persistence/projection first |
| Recovery/resume | Internal first; public API only after recovery model exists |

**Recommendation:** Add no protocol types in O0 or O1 unless type ownership
strictly requires it. Any later v2 surface must use experimental gating first,
follow existing naming/serialization rules, regenerate schemas, and add public
API tests. Existing thread/turn APIs are sufficient for the ordinary path but
not sufficient as a complete workflow API.

## 14. TUI integration findings

**Verified:** Syndrid TUI screens are presentation and navigation surfaces.
`SyndridScreen` owns cached screen state; `LiveSessionState` contains reserved
workflow, agent, usage, wait, forecast, and verification fields. `multi_agents.rs`
maps native collaboration items for display and states that coordination belongs
above presentation. Event routing is centralized in app/thread event modules.

**Recommendation:** A future mode selector and recommendation preview belong
in existing focused-screen/app command ownership, not in `chatwidget` as a new
execution engine. Workflow projections should consume an event-derived cache,
reuse thread-target routing, and preserve approval modal focus/restore behavior.
Do not add direct polling, per-frame repository scans, or TUI-owned policy.

**Unresolved:** Exact final screen/module placement depends on O1/O2 domain and
projection types. No broad TUI redesign is justified by this audit.

## 15. Single Mode zero-overhead path

**Verified current path:** `AppCommand::UserTurn` in
`codex-rs/tui/src/app/thread_routing.rs` either steers an active turn or calls
`AppServerSession::turn_start` in `codex-rs/tui/src/app_server_session.rs`.
That method sends one existing `TurnStartParams` request containing the current
model, effort, permissions, workspace roots, and input. No Syndrid planner or
workflow model call is involved.

**Recommendation:** Put the smallest future mode dispatch immediately before
the existing user-turn routing, with a `Single` branch that calls this exact
path. The branch must not create a workflow record, inject context, add a model
call, fork a thread, or add a second event projection. Benchmark request count,
latency, allocations, event handling, and persistence writes against the
pre-dispatch path.

```text
Current:   user input -> AppCommand::UserTurn -> turn_start -> Codex turn
Future:    user input -> mode dispatch
                         -> Single: same AppCommand::UserTurn path
                         -> other: orchestration coordinator
```

## 16. Recommended orchestration crate/module placement

**Recommendation (primary): option C.** Create a small low-dependency domain
crate for orchestration types and a later narrow core-facing integration
interface/adapter. The domain crate should depend on shared protocol/serde
types only; the adapter should be the sole place that translates workflow
assignments into native `ThreadManager`/`AgentControl` operations. Core remains
the runtime and data plane; TUI owns projections; app-server owns wire exposure
only when a public API is justified; persistence remains an explicit state
boundary rather than duplicated rollouts.

This gives O1 independent type tests, avoids a core cycle, keeps normal Codex
unchanged, and provides a future place for workflow policy without embedding a
second runtime.

## 17. Rejected placement alternatives

- **A. Fully independent `codex-orchestration` runtime crate now — Rejected.**
  It cannot directly use crate-private `AgentControl` and would tempt a second
  scheduler/runtime or an illegal dependency cycle.
- **B. Large module in an existing non-core crate — Rejected for the full
  feature.** It may be suitable for a temporary adapter, but workflow policy,
  persistence, and events would be poorly owned if spread through CLI/TUI.
- **D. TUI-owned orchestration — Rejected.** TUI is presentation/event routing;
  it must not own execution, permissions, persistence, or budget truth.
- **E. App-server-owned orchestration — Rejected initially.** It would make
  internal policy and recovery wire-facing too early and would not solve access
  to core-private native child control.

## 18. Exact integration-seam table

| Capability | Existing file/module | Existing type/function/event | Visibility | Reuse directly | Adapter required | Missing primitive | Risk | Recommended O-phase |
|---|---|---|---|---|---|---|---|---|
| Normal turn | `core/codex_thread.rs`, `tui/app_server_session.rs` | `CodexThread::submit_with_id`, `turn_start`, `TurnStartParams` | public/core or crate-local TUI | Yes | Dispatch adapter | Workflow task binding | Single Mode overhead | O1/O4 |
| Child spawn | `core/agent/control.rs`, `core/tools/handlers/multi_agents_v2/spawn.rs` | `AgentControl::spawn_agent_with_communication`, `SpawnAgentArgs` | mostly `pub(crate)` | Native path | Yes | External scheduler capability | Second runtime / visibility | O4 |
| Fork/resume | `core/thread_manager.rs` | `spawn_subagent`, `fork_thread`, `resume_thread_with_history` | core API/internal mix | Yes | Handoff/recovery adapter | Workflow checkpoint | Context duplication | O4/O13 |
| Model/effort | `core/codex_thread.rs`, protocol v2 thread | `ThreadConfigSnapshot`, `CodexThreadSettingsOverrides`, `ThreadSettingsUpdateParams` | public/wire plus internal | Yes | Role routing | Actual-route ledger | Silent fallback | O3/O4 |
| Usage | `protocol/protocol.rs`, protocol v2 thread | `TokenUsage`, `TokenUsageInfo`, `TokenCountEvent`, `ThreadTokenUsage` | public/wire | Yes | Workflow attribution | Stage/agent ledger | Double counting | O6 |
| Rate limits | `protocol/protocol.rs`, app-server account | `RateLimitSnapshot`, account notifications | public/wire | Yes | Forecast adapter | Reliable future demand | Guessed quota | O9/O10 |
| Events | `protocol/protocol.rs`, app-server notifications | `Event`, `EventMsg`, collaboration/token/turn events | public/wire | Reference | Event projection | Workflow envelope | Two truths | O2 |
| Topology | `agent-graph-store`, `state/runtime/threads.rs` | `AgentGraphStore`, `thread_spawn_edges` | public store/API | Yes | Workflow mapping | Task/stage topology | Resume mismatch | O2/O13 |
| Cancellation | `core/session/mod.rs`, app-server turn processor | cancellation token, `Op::Interrupt`, `turn_interrupt` | native APIs | Yes | Parent/workflow propagation | Independent task state | In-flight tool behavior | O4 |
| Permissions | `protocol/protocol.rs`, `sandboxing/landlock.rs` | `SandboxPolicy`, `PermissionProfile` | public/technical | Yes | Intersection/role ceiling | Public child ceiling | Widening permissions | O4 |
| Persistence | `thread-store`, `rollout`, `state` | `ThreadStore`, rollout/state APIs | public/internal | Thread truth only | Workflow store | Handoffs/evidence/budget state | Duplicate transcripts | O2/O13 |
| Verification | `tui/syndrid_live_state.rs`, core events | `VerificationItem`, command/file/error events | TUI types/public events | Evidence inputs | Evidence adapter | Durable evidence record | Claim treated as proof | O2/O4 |
| TUI projection | `tui/syndrid_screen.rs`, `app/*` | `SyndridScreen`, `LiveSessionState`, app event routing | TUI-internal | Projection patterns | Workflow projection | Shared workflow cache | Focus/polling regressions | O2/O7 |

## 19. Existing tests relevant to orchestration

**Verified test areas:** native agent execution and jobs under
`codex-rs/core/tests/suite/agent_execution.rs` and `agent_jobs.rs`; forks and
model overrides under `fork_thread.rs`, `model_overrides.rs`, and
`model_switching.rs`; interruption under `abort_tasks.rs`; approvals under
`approvals.rs`; thread lifecycle under `thread_manager_tests.rs`; native agent
control/registry tests under `core/src/agent/control_tests.rs`,
`agent/control/execution_tests.rs`, and `agent/registry_tests.rs`.

App-server coverage includes v2 `thread_start`, `thread_resume`,
`thread_fork`, `thread_status`, `turn_start`, `turn_interrupt`, and permission
request suites. Protocol tests cover collaboration item and token shapes.
TUI coverage includes app event/notification tests, token usage tests,
permission tests, `syndrid_screen` tests, and bounded live-state activity tests.

**Recommendation:** O1 tests should be focused typed-domain tests. O4 must add
integration tests around native thread/agent execution and permission/cancel
behavior; TUI changes later require snapshots under existing instructions.

## 20. Compatibility, security, and performance risks

### Compatibility risks

- Bypassing `AgentControl` can lose native parent/child status and cancellation.
- Using independent app-server sessions can break shared session truth and
  resume semantics.
- Rewriting context or injecting complete transcripts harms cache stability and
  may violate context bounds.
- Treating requested model/effort as actual route hides reroutes/fallbacks.
- Adding public protocol fields too early creates v2 compatibility and schema
  obligations.

### Security risks

- A child/workflow adapter could accidentally widen approval, sandbox, network,
  MCP, or file-write permissions.
- Read-only roles are unsafe if enforced only through prompt instructions.
- Persisted handoffs/evidence must not store secrets or unbounded tool output.
- Approval focus and cancellation must not be bypassed by orchestration UI.

### Performance risks

- A planner model call on every small task breaks the Single Mode baseline.
- Full transcript copying causes token, memory, and provider-cache costs.
- Polling Git, filesystem, quota, or events per frame harms TUI latency.
- Aggregating cumulative token counts incorrectly inflates budget and may stop
  useful work prematurely.
- Unbounded retries, recursive delegation, or parallel writers violate initial
  limits.

## 21. Open questions

1. What narrow public/core-facing trait can expose native child control without
   exporting the whole `AgentControl` implementation?
2. Where should workflow state live relative to `StateRuntime`, `ThreadStore`,
   and rollouts while avoiding duplicate session truth?
3. How will workflow/stage/agent usage be attributed from thread/turn totals
   without altering Codex token accounting?
4. What exact technical capability enforces read-only Explorer behavior for
   every supported sandbox/platform?
5. How should parent cancellation propagate to child turns and in-flight tools?
6. Which native retry classes are automatic, and which failures are eligible
   for one bounded repair pass?
7. What is the smallest event envelope needed for deterministic recovery and
   replay?
8. Does app-server need workflow APIs for the first production workflow, or can
   the CLI/TUI remain internal clients initially?
9. What benchmark threshold defines “materially unchanged” Single Mode?
10. Which domain types belong in a new crate versus `codex-protocol`?

## 22. O1 recommendation

**Recommendation:** Implement only an exhaustive, behavior-free typed
orchestration domain model. Include mode, workflow/task/agent IDs, stage,
lifecycle, wait reason, cancellation, role/profile, model/effort route,
multiplier/budget, adaptive policy, recommendation/forecast confidence and
data quality, handoff, permissions, work claim, verification requirement/state/
evidence, and workflow event shapes.

Keep lifecycle, stage, wait reason, cancellation, and verification as separate
dimensions. Add serialization only where persistence or tests require it. Do not
start agents, schedule work, inject context, change permissions, add app-server
methods, render new TUI, or alter core execution.

## 23. Proposed O1 acceptance criteria

- All core domain states are typed and exhaustive without wildcard behavior.
- Lifecycle/stage/wait/cancellation/verification are separate dimensions.
- `Exact`, `Derived`, `Estimated`, and `Unavailable` remain explicit.
- No model, tool, sandbox, approval, persistence, or app-server behavior changes.
- The domain crate has no dependency on `codex-core` unless a narrow interface
  is proven necessary.
- Serialization is bounded and tested only for justified persisted/public types.
- Tests compare complete values and cover invalid/state-transition boundaries.
- Single Mode has no new model call, context injection, or workflow execution.
- O1 diff remains independently reviewable and within repository change-size
  guidance.

## 24. Files O1 is expected to touch

**Provisional recommendation, subject to implementation review:**

- `codex-rs/Cargo.toml` for workspace registration, if a new crate is chosen.
- `codex-rs/orchestration/Cargo.toml`.
- `codex-rs/orchestration/src/lib.rs`.
- `codex-rs/orchestration/src/types.rs` and focused sibling modules.
- Focused test files adjacent to the new domain modules.
- Required Bazel metadata only if the repository’s crate build requires it.

These are future files, not changes made by this audit.

## 25. Files O1 must not touch

- `codex-rs/core/src/agent/control.rs` and native spawn execution, unless a
  separately approved adapter seam is required after domain review.
- Thread execution, tool runtime, sandbox, approvals, MCP, and token-accounting
  implementation.
- `codex-rs/tui` rendering or snapshots.
- App-server v2 protocol and generated schema files.
- Rollout/session transcript storage and existing Codex persistence truth.
- `AGENTS.md`, existing Syndrid architecture/reference documents, README, and
  nested instructions.

## 26. Final go/no-go decision for O1

**GO:** typed orchestration domain model and focused tests, using a legal
low-dependency ownership boundary.

**NO-GO:** agent execution, workflow scheduling, Planner → Executor → Verifier,
parallelism, automatic recommendations, adaptive budgeting, new TUI surfaces,
or app-server workflow APIs in O1.

The next execution phase should not begin until the open adapter, permission,
usage attribution, event correlation, and recovery questions have owners and
observed-test plans.

