# Phase 7G — Production Orchestration-Turn Boundary

## 1. Status and scope

Status: design-only, not implemented.

This document defines the boundary required for a normal Syndrid user turn to
use the existing Phase 7 live orchestration runtime. It does not change Rust,
the app-server protocol, the Codex turn path, provider behavior, tools,
approvals, or the Phase 8 dashboard. It is an integration design and audit for
the follow-up implementation milestones 7G1–7G6.

The existing Phase 8A dashboard is a consumer of structured observations. It
does not currently receive live production observations because no production
user-turn caller invokes `LiveOrchestrationCoordinator` and no production
sender emits `AppEvent::UpdateOrchestrationObservation`.

## 2. Governing architecture and conflicts

The design was audited against these documents and implementation areas:

| Source | Relevant authority |
| --- | --- |
| `SYNDRID_ORCHESTRATION.md` | Codex remains the execution/data plane; Syndrid owns orchestration policy, routing, budgets, verification, repair, and projections. |
| `SYNDRID_ORCHESTRATION_O0_AUDIT.md` | The current Codex thread/turn path is the compatibility baseline; workflow state must not be recreated in an independent runtime. |
| `docs/phase-1-branding-boundary.md` | `PublicBrand` is presentation-only. It is not a provider, protocol, authentication, storage, or execution-policy identifier. |
| `docs/phase-2-distribution.md` | Distribution and public brand are separate concerns; app-server receives no implicit brand authority. |
| `codex-rs/tui/AGENTS.md` | Codex rendering and behavior remain unchanged; Syndrid presentation is explicitly gated. |
| `codex-rs/app-server/README.md` | `thread/start`, `turn/start`, `turn/interrupt`, and existing item/turn notifications are the supported session boundary. |
| Current Phase 7 source | `LiveOrchestrationCoordinator`, O6E policy state, routing profiles, bounded budgets, cleanup, typed failures, and snapshots are implemented in `codex-rs/core/src/syndrid_orchestration`. |

There are two important stale or potentially conflicting statements. The O0
audit describes the orchestration implementation as future work, while the
current repository now contains the Phase 7 coordinator and observability
implementation. Its compatibility and ownership conclusions remain valid, but
its implementation status does not. Also, a naive rule of “Syndrid executable
means orchestration” would conflict with the branding document: `PublicBrand`
is a presentation authority, not a trusted execution authority. This design
therefore introduces an explicit runtime execution-path capability at the
composition boundary and does not inspect process names deep inside the
app-server.

## 3. Current production turn path

The verified normal TUI path is:

```text
ChatWidget composer
  → ChatWidget::submit_user_message_with_history_and_shell_escape_policy
  → AppCommand::UserTurn / AppEvent::CodexOp
  → App::submit_active_thread_op
  → App::submit_thread_op
  → ThreadRouting::try_submit_active_thread_op_via_app_server
  → AppServerSession::turn_start
  → ClientRequest::TurnStart
  → app-server TurnProcessor::turn_start_inner
  → CodexThread::submit_user_input_with_client_user_message_id
  → existing Codex model/tool/approval loop
  → app-server turn/item/token/tool notifications
  → TUI event dispatch and transcript rendering
  → turn completion, failure, interrupt, or cancellation
```

| Boundary | Current symbol and file | Input/output | Cancellation and events | Ownership |
| --- | --- | --- | --- | --- |
| Composer | `ChatWidget::submit_user_message_with_history_and_shell_escape_policy` in `codex-rs/tui/src/chatwidget/input_submission.rs` | Composer text and shell-escape policy become bounded `Vec<UserInput>` and `AppCommand::UserTurn`. | TUI busy state and local transcript preparation begin here; no orchestration cancellation exists here. | TUI owns input presentation. |
| TUI dispatch | `AppEvent::CodexOp` in `codex-rs/tui/src/app/event_dispatch.rs` | Converts the app event into active-thread submission. | Async event dispatch awaits app-server submission; existing interrupt events remain available. | TUI routes events, not execution policy. |
| Thread routing | `submit_active_thread_op`, `submit_thread_op`, and `try_submit_active_thread_op_via_app_server` in `codex-rs/tui/src/app/thread_routing.rs` | Resolves thread, model, effort, approval, permission, workspace, and output settings; calls `turn_start` for a new turn or `turn_steer` for an active turn. | `AppCommand::Interrupt` calls `turn_interrupt`; stale steer races are handled here. | TUI selects request settings from the existing session state. |
| App-server client | `AppServerSession::turn_start` and `turn_interrupt` in `codex-rs/tui/src/app_server_session.rs` | Builds `ClientRequest::TurnStart` / `TurnInterrupt`; receives `TurnStartResponse`. | JSON-RPC request/notification transport. | Client transport; not an orchestration authority. |
| Turn processor | `TurnProcessor::turn_start_inner` in `codex-rs/app-server/src/request_processors/turn_processor.rs` | Loads the thread, validates input, maps protocol input to core `Op::UserInput`, and submits it. | `turn_interrupt` maps to the core interrupt operation. | App-server owns RPC/session admission and current turn lifecycle. |
| Codex thread | `CodexThread::submit_user_input_with_client_user_message_id` in `codex-rs/core/src/codex_thread.rs` | Submits the user input to the existing thread/session agent control. | Core cancellation, session shutdown, approval, sandbox, tools, and persistence remain native Codex behavior. | Codex owns the execution/data plane. |
| Notifications | app-server thread/turn/item/token/tool notification processors and TUI event targets | Produces turn lifecycle, assistant item, token delta, tool progress, error, and completion notifications. | Disconnect and interrupt are handled by existing app-server/session mechanisms. | Existing protocol and transcript consumers. |

The current embedded TUI starts the app-server with `SessionSource::Cli` for
both brands. `PublicBrand` is available in TUI composition and widgets, but is
not currently propagated to the app-server as an execution selector. This is
why executable-name inspection inside `TurnProcessor` would be both fragile
and contrary to the documented branding boundary.

## 4. Current Phase 7 coordinator path

The existing Phase 7 path is:

```text
LiveOrchestrationRequest
  → LiveOrchestrationCoordinator::run
  → O6E policy and routing-profile resolution
  → generation and budget ledger creation
  → validation
  → planner, executor, verifier, optional repair
  → terminal-cause arbitration
  → budget terminalization
  → cleanup and reservation release
  → final cleanup snapshot
  → state terminalization
  → LiveOrchestrationOutcome
```

Concrete current contracts:

- `LiveOrchestrationCoordinator<P>` is in `core/src/syndrid_orchestration/live_coordinator.rs`. Its constructor receives a provider, `RoutingProfileRegistry`, and `RoutingConnectionDirectory`. `run` receives `SessionExecutionPolicyState` plus `LiveOrchestrationRequest` and returns `LiveOrchestrationOutcome` or typed `LiveOrchestrationError`.
- `LiveOrchestrationRequest` is in `live_coordinator_types.rs`. It contains the run ID, optional resolved policy, optional routing profile ID, bounded instruction/context, task specifications, planning and verification contracts, failure policy, repair instruction, approved-tool policy, cancellation token, and overall timeout.
- `SessionExecutionPolicyState` in `session_execution.rs` is the O6E lifecycle authority. It guards mode and route mutation while active, allocates generations, and prevents stale terminalization.
- `ExecutionModeSelection` and `ResolvedExecutionPolicy` in `execution_modes.rs` define built-in and Custom modes, role activation, concurrency, invocation, tool, repair, context, output, and timeout budgets.
- `SubagentProvider` in `subagent.rs` is a provider-neutral asynchronous invocation seam. `SubagentRuntime` adds routing validation, bounded tool loops, budget reservations, provider usage accounting, and cancellation handling.
- `ProviderInvocationRequest`, `ProviderInvocationResult`, and `ProviderInvocationError` in `invocation.rs` are the existing provider-neutral invocation types.
- `invoke_codex`, `invoke_openrouter`, and `invoke_omniroute` are existing provider-specific seams. Codex selection is bound by `CodexInvocationAdapter` to an exact `ProviderSelection`, account registry, and credential provider. The current `UnavailableCodexInvocationClient` proves that a live scoped Codex operation is still missing; it must not be treated as a production implementation.
- `OrchestrationObservationCollector` in `orchestration_observability_runtime.rs` applies bounded `LiveEvent` values to a privacy-safe snapshot. It currently produces an observation for the final outcome, but has no live callback, sink, watch receiver, or broadcast sender.
- `finish_outcome` in `live_coordinator_mapping.rs` selects the terminal cause, marks the budget terminal, freezes failure, completes cleanup, obtains the cleanup snapshot, terminalizes the O6E state, and only then constructs the final observation and `LiveOrchestrationOutcome`. This ordering must remain authoritative.
- `LiveOrchestrationOutcome` contains lifecycle, role counts, budgets, terminal cause, failure, events, and the final observation, but no complete user-facing assistant response. `LiveRoleOutcome` also contains status/count metadata rather than raw role output.

The coordinator is currently exercised by core tests and has no production
user-turn caller. The production-readiness gaps are therefore at the boundary,
not a reason to duplicate the coordinator:

1. No trusted execution-path selector reaches the app-server/core turn boundary.
2. No production code captures a user turn into `LiveOrchestrationRequest`.
3. No production-capable provider dispatcher binds all coordinator roles to the exact selected Codex/OpenRouter/OmniRoute connection.
4. No production approved-tool/approval adapter maps role capability envelopes to the native Codex runtime.
5. No live observation sink connects the collector to app-server and `AppEvent`.
6. No typed user-facing final-result contract exists.
7. No production cancellation bridge owns coordinator task joins and cleanup completion.

## 5. Proposed architecture

The proposed future call graph is:

```text
TUI ChatWidget composer
  → existing AppCommand::UserTurn and thread routing
  → AppServerSession::turn_start with trusted session execution capability
  → TurnProcessor::turn_start_inner
  → ProductionTurnRouter
      ├─ CodexCompatibilityTurnRunner
      │    → existing CodexThread::submit_user_input_with_client_user_message_id
      └─ SyndridOrchestrationTurnRunner
           → capture immutable run configuration
           → create root cancellation scope
           → build LiveOrchestrationRequest
           → run existing LiveOrchestrationCoordinator
                → provider-neutral exact-route dispatcher
                → native approved-tool, sandbox, and approval adapter
           → observation sink/watch
                → app-server bridge
                → AppEvent::UpdateOrchestrationObservation
                → ChatWidget::update_orchestration_observation
                → SessionDashboardState
           → typed final result
                → existing agent-message/turn-completed translation
                → TUI transcript and persistence
```

The router is a dispatch boundary, not a second runtime. The compatibility
runner must call the current Codex path unchanged. The Syndrid runner translates
inputs and outputs around `LiveOrchestrationCoordinator`; it must not implement
planning, scheduling, verification, repair, policy resolution, or cleanup.

### Proposed abstractions

| Proposed abstraction | Owner and visibility | Inputs | Outputs | Responsibility |
| --- | --- | --- | --- | --- |
| `SessionExecutionPath` | Internal app-server/core boundary; not a public TUI brand enum | Trusted session capability and per-turn selection | `CodexCompatibility` or `SyndridOrchestration` | Selects the execution path once for a thread/turn. Omitted or unsupported capability defaults to Codex compatibility. |
| `ProductionTurnRouter` | Internal app-server/core turn integration module | Thread/session state, turn input, captured path | Runner result and existing turn lifecycle | Dispatches to exactly one runner and owns no policy. |
| `SyndridOrchestrationTurnRunner` | Internal orchestration integration module near the core/session boundary | Thread context, immutable policy/route, user input, native runtime handles | `OrchestrationTurnResult` and observations | Builds the existing request, invokes the existing coordinator, and translates results. |
| `ProductionProviderDispatcher` | Internal core orchestration adapter | `ProviderInvocationRequest`, exact route/account/connection, cancellation | `ProviderInvocationResult` or typed provider error | Reuses existing provider-neutral invocation types and exact provider adapters. It does not rotate accounts or silently fall back. |
| `OrchestrationObservationSink` | Provider-neutral core interface | Complete privacy-safe `OrchestrationObservationSnapshot` | Non-blocking publication result | Carries snapshots out of core without depending on TUI types. Closed receivers are non-fatal. |
| `OrchestrationTurnResult` | Internal core/app-server translation type | Coordinator outcome plus bounded synthesis result | Success, partial, typed failure, cancellation, timeout, budget, or cleanup result | Separates user-facing response from internal role evidence and observation state. |

The exact crate placement of the router and the native Codex invocation bridge
must be resolved in 7G1/7G2. A narrow internal adapter is preferred over adding
public API to `codex-core`; if crate privacy requires a core-facing trait, it
should expose only the legal operation needed by the app-server/session
boundary and delegate to existing Codex thread/session machinery.

## 6. Execution-path selection authority

The authority must be an explicit trusted runtime/session capability, not the
executable name and not a TUI presentation enum. The proposed values are:

```text
CodexCompatibility
SyndridOrchestration
```

Selection rules:

1. The composition root determines whether the session is authorized to offer
   Syndrid execution. `PublicBrand::Syndrid` may gate the presentation and
   capability offer, but is not itself the execution authority.
2. A trusted app-server/session runtime capability records the path before the
   first eligible turn. The app-server must not infer it from `argv[0]`, a
   binary name, or a user-controlled request field.
3. The path is captured into the thread/turn execution context and is immutable
   for that turn. A turn cannot switch from Codex to orchestration or back while
   active.
4. Missing, unsupported, or unacknowledged Syndrid capability selects the
   existing Codex compatibility path. This preserves old clients and remote
   app-server behavior.
5. Syndrid orchestration is selected only when the trusted capability, session
   policy, and resolved O6E policy all permit it. Invalid Custom policy remains
   a typed admission failure; it is not silently downgraded.
6. The TUI mode selector remains pending state for the next eligible run. The
   production boundary captures the resolved O6E state exactly once at dispatch;
   later pending changes cannot mutate the active run or its dashboard mode.

For the first implementation, the embedded TUI can carry this capability
through the in-process app-server composition boundary without changing the
wire protocol. A remote app-server cannot safely receive an implicit local
brand. If remote Syndrid execution is required, use an optional experimental
v2 capability/field with explicit server acknowledgement; omission retains
Codex compatibility.

## 7. Production request construction

At `turn_start_inner`, after thread admission and before execution dispatch, the
Syndrid runner captures one immutable `ProductionOrchestrationRunConfig` (name
illustrative) containing:

| Value | Capture owner and point | Request representation |
| --- | --- | --- |
| User objective | Turn processor after validated `UserInput` | Bounded `instruction`; no full prompt diagnostics. |
| Conversation/project context | Existing thread/session context provider | Bounded `context`; include only approved relevant context, never hidden reasoning or unbounded transcript. |
| Session/thread identity | App-server thread and turn admission | `run_id` derived from stable thread/turn/submission identity plus a new O6E generation. |
| Execution mode | O6E `SessionExecutionPolicyState` at dispatch | Immutable `ResolvedExecutionPolicy` and selected mode. |
| Routing profile | O6E/session routing state at dispatch | Immutable `RoutingProfileId`; validate the registry snapshot before execution. |
| Provider connections/accounts | Routing registry and credential/profile stores | Exact provider, connection, account, model, and effort selected by the resolved route. |
| Workspace and permissions | Existing Codex thread/session configuration | Native cwd, workspace roots, sandbox, approval policy, and permission restrictions intersected with role policy. |
| Approved capabilities | Workflow policy plus native runtime restrictions | `SubagentToolPolicy` or its production mapping, with role-specific ceilings. |
| Budgets | Resolved O6E policy and ledger creation | Existing execution, provider, tool, context, output, repair, and timeout budgets. |
| Cancellation | App-server turn scope | Root `CancellationToken`, with child scopes managed by the coordinator/runtime. |
| Deadline | Turn/session deadline and O6E timeout | `overall_timeout` bounded by the existing policy. |
| Planning/verification | Resolved policy and user-turn workflow contract | Existing `PlanningContract`, `VerificationContract`, failure policy, and bounded repair instruction. |

The request must not be constructed by the TUI from a dashboard snapshot or by
copying raw role transcripts. Initial workflow task specifications should be
created only from a bounded, policy-approved planning result or an explicit
single task. The coordinator remains responsible for planning, role scheduling,
verification, repair, terminal-cause arbitration, and cleanup.

## 8. Provider and tool adaptation

### Provider boundary

Reuse `ProviderInvocationRequest`, `ProviderInvocationResult`, and
`ProviderInvocationError`, and the existing exact-route provider adapters. The
production dispatcher should:

- route `codex`, `openrouter`, and `omniroute` by the exact resolved assignment;
- bind the exact selected connection, account, model, effort, and credential
  provider before the run;
- reject missing or invalid routes as typed failures;
- never rotate accounts, choose an alternate provider, or silently fall back;
- preserve cancellation, bounded output, request IDs, and usage accounting;
- keep provider payloads and credentials out of observations and diagnostics.

Native Codex requires a real scoped invocation bridge. The current
`CodexInvocationAdapter` and `invoke_codex` establish the right exact-selection
shape, but `UnavailableCodexInvocationClient` reports that the repository does
not yet expose the required scoped live operation. 7G2 must either bind the
adapter to existing `CodexThread`/`AgentControl` machinery or expose a narrow
core-private operation that does so. Calling a second unrestricted provider
client would bypass Codex session, approval, tool, persistence, and usage
truth and is not acceptable.

The `SubagentProvider` trait remains the coordinator seam. Extend or wrap it
only to supply the native runtime context required by production invocation;
do not introduce a second provider registry or policy. Test providers may remain
deterministic fakes.

### Tool and approval boundary

The adapter must map each role to the intersection of:

```text
user permissions
∩ session permissions
∩ workspace/sandbox restrictions
∩ workflow policy
∩ parent-role ceiling
∩ role capability envelope
∩ task request
∩ runtime restrictions
```

The initial envelopes should be explicit and bounded:

- Planner: read-only context and approved discovery operations.
- Executor: only approved file and shell operations, with existing sandbox and
  approval enforcement; writes remain workspace-contained.
- Verifier: read-only inspection and verification operations.
- Repair: executor-like write capability only when O6E repair policy authorizes
  it, with the same or narrower workspace and approval restrictions.

Approval requests must use the existing Codex approval path and remain
interruptible. No role receives arbitrary shell, network, or account access
because it is easier for the adapter. Tool outputs are bounded before entering
role context or any operational summary.

## 9. Live observation delivery

The selected mechanism is a provider-neutral observation sink backed by a
bounded latest-snapshot `tokio::sync::watch` channel per active run:

```text
OrchestrationObservationCollector
  → OrchestrationObservationSink::publish(snapshot)
  → bounded watch::Sender
  → app-server per-turn observation bridge
  → existing TUI app-event conversion
  → AppEvent::UpdateOrchestrationObservation
  → ChatWidget::update_orchestration_observation
  → SessionDashboardState and rendering
```

This fits snapshots rather than an audit log: the dashboard needs the newest
state, queue growth is impossible, and a closed receiver does not fail the
coordinator. The sink must publish without awaiting the TUI and ignore a send
failure caused by a closed receiver. It must not contain TUI types.

The collector/coordinator integration must publish after each meaningful
structured lifecycle projection: prepared/pending, policy validation, role
start, executor batch, verifier, repair, terminal-cause selection, cleanup in
progress, and cleanup complete. The final terminal snapshot is published only
after `OrchestrationCleanup::complete` and O6E terminalization. Generation is
owned by `SessionExecutionPolicyState`; sequence is owned by the collector for
that generation. A new generation accepts its own sequence values, while stale
generation or sequence values are rejected by the existing dashboard state.

The app-server bridge owns mapping a snapshot to a thread/turn and forwarding
it to the client. For an embedded TUI this can remain an in-process event sink
in the first implementation. If observations must cross a remote app-server
connection, add an optional privacy-safe notification only after the internal
bridge is proven. Dashboard closure must remove the receiver, not cancel the
run.

Snapshots may contain exact, derived, estimated, and unavailable values only as
defined by Phase 7D. They must not contain prompts, hidden reasoning,
credentials, tokens, raw provider responses, raw tool output, or arbitrary
role transcripts.

## 10. Final-result contract

`LiveOrchestrationOutcome` remains the lifecycle/evidence result and must not be
made into a raw transcript container. Add a separate bounded internal result,
tentatively `OrchestrationTurnResult`, with these cases:

```text
Completed {
    response: BoundedText,
    summary: SanitizedWorkSummary,
    verification: VerificationSummary,
}
Partial {
    response: BoundedText,
    summary: SanitizedWorkSummary,
    verification: VerificationSummary,
    cause: TypedPartialCause,
}
Failed { failure: TypedTurnFailure, user_message: BoundedText }
Cancelled { user_message: BoundedText }
TimedOut { user_message: BoundedText }
BudgetExhausted { category: BudgetExhaustionCategory, user_message: BoundedText }
CleanupIncomplete { cause: TypedCleanupFailure, user_message: BoundedText }
```

The exact type name and field set are a 7G4 decision, but the separation is
mandatory:

- internal role evidence stays bounded and private;
- operational metadata contains only privacy-safe counts/statuses and references;
- the user-facing response is an explicitly authored, bounded result;
- raw planner, executor, verifier, and repair transcripts are not emitted by
  default.

The designated synthesis stage authors the final response from approved bounded
role results. Verification can reject or require repair and can provide a
sanitized verification summary, but verifier internals are not the user answer
unless the workflow contract explicitly makes that role the final author. A
no-op or investigation-only run returns a bounded status/summary rather than a
fabricated success claim. Partial completion is typed and must not be reported
as complete.

The app-server translation should reuse existing turn/item notification shapes:
emit a bounded assistant message through the normal transcript path, then emit
the existing completion or error lifecycle. The translation must preserve the
typed terminal cause in internal state while presenting a clear bounded user
message. It must not fake TUI cells or bypass conversation persistence.

## 11. Cancellation and cleanup

Ownership and ordering are:

```text
TUI interrupt
  → existing turn_interrupt request
  → TurnProcessor finds active Syndrid run
  → runner cancels root CancellationToken
  → coordinator arbitrates UserCancelled against other terminal causes
  → child provider/tool/approval tasks observe derived cancellation
  → child tasks and coordinator task are joined
  → budget reservations and runtime resources are released
  → cleanup completes
  → final cleanup observation is published
  → app-server emits final turn status
  → TUI releases busy state
```

The runner owns the root token and exactly one join handle for the coordinator
task. The coordinator owns terminal-cause arbitration, child cancellation,
budget cleanup, failure freezing, and cleanup completion through Phase 7E. The
app-server owns turn admission and awaits the runner result; it must not detach
the task. Session shutdown cancels and joins all active runners before releasing
the session. Timeout uses the coordinator’s existing bounded timeout path and
must not create a competing terminal authority.

Cancellation during planning, execution, or repair has the same cleanup
guarantee. If a notification receiver closes, only observation delivery stops;
execution continues. If the app-server client disconnects, the server retains
turn cleanup ownership and records a typed disconnected-session outcome.

## 12. Transcript and app-server integration

The first implementation should reuse the current app-server protocol:

- the user message is admitted and persisted through the existing turn path;
- existing `turn/started` and turn identity remain authoritative;
- sanitized progress can use existing bounded status mechanisms where available;
- approvals and tool progress continue through native Codex mechanisms;
- dashboard snapshots use an internal bridge to the existing TUI app event;
- the final bounded assistant response is emitted through the existing assistant
  item/message path;
- terminal failure, cancellation, timeout, and completion use existing turn
  lifecycle notifications;
- resume uses persisted conversation and a new execution generation, never a
  reused active generation.

Steering an active orchestrated turn must be explicitly decided before 7G4: the
safe initial behavior is to reject or queue steering while the coordinator owns
the active run, rather than mutating its immutable instruction or policy.

## 13. Protocol impact

The smallest correct initial choice is Option A: no public app-server protocol
change for 7G1–7G5. The embedded TUI and app-server can use a trusted
composition-root capability and an in-process observation bridge while retaining
`turn/start`, `turn/interrupt`, and existing turn/item notifications. Existing
Codex clients omit the capability and remain on the current path.

Remote Syndrid execution and remote dashboard observation are not safely
expressible through the current protocol because `clientInfo` identifies a
client but is not an execution authorization. If remote support becomes a
requirement, use Option B, not a protocol version fork:

- add an optional experimental v2 execution-path capability with explicit server
  acknowledgement; omission means Codex compatibility;
- add an optional experimental privacy-safe observation notification carrying
  thread ID, turn ID, generation, sequence, and the structured snapshot only;
- gate both on client/server capability negotiation;
- define resume behavior as starting a new generation;
- keep Codex clients unaware and unaffected.

Exact wire names, TypeScript annotations, schema fixtures, and public API tests
belong to the later protocol decision, not this design-only pass.

## 14. Compatibility guarantees

The implementation must guarantee:

1. Normal Codex sessions and clients continue to call the current Codex
   `CodexThread` path with no extra provider calls, context injections, or
   workflow state.
2. A missing or unsupported Syndrid capability defaults to Codex compatibility.
3. `PublicBrand` remains presentation-only and cannot silently alter provider,
   auth, sandbox, approval, storage, or protocol behavior.
4. O6E remains the sole execution-policy authority; TUI mode selection is
   pending state and the active run uses one immutable captured policy.
5. Phase 7D remains the sole source of runtime dashboard facts.
6. Phase 7E remains the sole authority for terminal-cause selection, cleanup,
   cancellation, and finalization.
7. No second thread/session/task runtime is created for orchestration.
8. Existing transcript persistence, interruption, approvals, and resume rules
   remain the Codex data-plane behavior unless a compatible translation is
   explicitly proven.

## 15. Privacy and security constraints

The production boundary must never send to TUI events, app-server notifications,
logs, or dashboard snapshots:

- hidden chain-of-thought or raw reasoning;
- complete prompts or unbounded conversation context;
- credentials, OAuth tokens, API keys, or raw authorization headers;
- raw provider responses or request payloads;
- sensitive raw tool output;
- unbounded role transcripts.

All text crossing the boundary must have a hard bound and an explicit owner.
Provider/account/model selection is exact and inspectable internally. Approval,
sandbox, workspace containment, and network restrictions are enforced by the
native runtime and intersected with role policy; the adapter must not broaden
them.

## 16. Failure analysis

| Failure | Owner and terminal classification | Cleanup and user behavior | Observation/test requirement |
| --- | --- | --- | --- |
| Wrong execution path | Trusted session boundary; typed admission failure, with Codex default only when capability is absent | Do not start the wrong runner; clear configuration error if an explicit Syndrid request is invalid | Selection matrix, old-client compatibility, no executable-name inference. |
| Missing/invalid routing profile | O6E/routing validation; invalid route | No provider call; release any admission reservation; bounded route error | Exact profile/account assertions and no fallback. |
| Invalid Custom mode | O6E policy resolution | Reject before run; preserve pending selection for correction | Custom validity and immutable capture tests. |
| Provider unavailable/auth failure | Provider dispatcher; typed provider failure | Coordinator selects failure, joins children, releases budget, final cleanup snapshot | Fake provider failure and credential privacy tests. |
| Tool runtime unavailable | Native tool adapter; typed tool/runtime failure | Stop affected role, apply Phase 7 failure policy, clean up | Role capability and bounded tool-result tests. |
| Approval deadlock | Approval/runtime owner; timeout or cancellation | Interrupt approval, join task, complete cleanup | Approval cancellation and timeout tests. |
| Observation receiver closed/full | Observation sink; non-terminal delivery condition | Drop observation only; execution and cleanup continue | Closed receiver and bounded-channel tests. |
| Coordinator panic/join failure | Runner/app-server; internal/join failure | Await what can be joined, release reservations, emit cleanup failure if incomplete | Join-failure and shutdown tests. |
| Cancellation during planning/execution/repair | Phase 7E terminal-cause authority; cancelled | Child cancellation, joins, cleanup, final cancelled snapshot, clear user status | One test per stage and no detached task. |
| Timeout during cleanup | Phase 7E cleanup owner; cleanup-incomplete/internal failure | Do not claim completed; bounded failure status and final best-effort observation | Cleanup timeout and terminal ordering tests. |
| Final answer unavailable | Synthesis/translation boundary; typed partial or failure | Never fabricate an answer; emit bounded status and preserve lifecycle cause | Missing synthesis result test. |
| Transcript notification failure | App-server translation; turn notification failure | Preserve internal outcome and cleanup; surface bounded session error | Notification failure test. |
| App-server disconnect/session shutdown | App-server session owner; disconnected/cancelled | Cancel and join active run before shutdown; no survivor | Disconnect and shutdown integration tests. |
| Resume after interruption | Session persistence boundary; new generation | Do not reuse active generation; resume as explicit new turn/run | Generation monotonicity and stale snapshot tests. |
| Codex compatibility regression | Codex runner/app-server; existing Codex behavior | Use unchanged current path | Existing Codex turn integration suite. |
| Sensitive data in observation | Snapshot/sink boundary; privacy defect | Reject/redact before publication; do not log payload | Structural snapshot and serialization assertions. |

## 17. Milestone breakdown

### 7G1 — Production turn-selection boundary

Goal: introduce a trusted internal `SessionExecutionPath` capability and a
router seam while preserving the exact current Codex path as the default.

Likely areas: app-server/session composition, turn processing, narrow core-facing
adapter, and focused app-server/core tests. No provider, observation, final
result, or tool behavior changes.

Acceptance: explicit Syndrid capability selects a placeholder internal seam;
absence/unsupported capability selects Codex; path is immutable per turn; no
process-name detection; Codex turn tests pass.

### 7G2 — Production request and provider/tool adapter

Goal: capture policy, route, context, budgets, native runtime handles, and
cancellation once, then satisfy `SubagentProvider` through exact existing
provider seams and approved native tools.

Tests: fake provider/tool adapters, exact route/account/model assertions,
permission intersection, Custom-mode rejection, bounded input/output.

Dependencies: 7G1 and a decision on the native scoped Codex invocation bridge.

Out of scope: public protocol changes, dashboard redesign, account fallback.

### 7G3 — Live observation sink and delivery

Goal: add the provider-neutral sink, bounded latest-snapshot channel, collector
publication points, app-server bridge, and existing TUI app-event conversion.

Tests: pending/running/stage snapshots, cleanup-final snapshot, generation and
sequence ordering, closed receiver, dashboard visibility, and privacy bounds.

Dependency: 7G1 and coordinator event projection points. The dashboard remains
only a consumer.

### 7G4 — Typed final result and transcript translation

Goal: produce a bounded user-facing response or typed failure from coordinator
results and reuse existing assistant-item/turn-completion notifications.

Tests: success, partial, verifier/repair failure, provider failure, no-op,
budget, timeout, cancellation, and transcript persistence.

Dependency: 7G2; final synthesis ownership must be resolved before implementation.

### 7G5 — Cancellation and cleanup integration

Goal: connect `turn_interrupt`, session shutdown, timeout, child cancellation,
joins, reservation release, and final cleanup observation with one owner each.

Tests: cancellation at each stage, receiver closure, client disconnect,
coordinator join failure, cleanup timeout, no detached task, composer release.

Dependencies: 7G2–7G4.

### 7G6 — End-to-end production validation

Goal: validate the embedded Syndrid TUI with deterministic fake provider/tool
fixtures, then validate safe real-provider behavior only where credentials and
approval policy permit.

Tests: idle compatibility, harmless task, dashboard during run, resize,
close/reopen, transcript continuity, completion/failure/cancellation, and
privacy review. No destructive worktree task and no real credentials in tests.

Dependencies: 7G1–7G5.

## 18. Test strategy

- Unit-test the execution-path selection matrix and immutable capture.
- Test request construction as a complete object, including exact route,
  policy, generation, limits, cancellation, and bounded context.
- Use deterministic fake `SubagentProvider` and approved-tool adapters; never
  require provider credentials.
- Add coordinator behavior tests for live sink publication, final cleanup
  publication, stale generation/sequence rejection, and closed receivers.
- Exercise the actual app-server turn boundary with focused integration tests;
  if a public protocol is later changed, test its v2 JSON-RPC contract and
  backward compatibility.
- Test TUI event conversion and dashboard behavior without coupling core to
  TUI types. Preserve Phase 8 snapshot coverage for any visible changes.
- Assert that serialized observations contain only approved privacy-safe fields.
- Test Codex compatibility through the existing normal turn path and verify no
  additional orchestration call occurs in that mode.

## 19. Acceptance criteria

Phase 7G is complete only when:

1. An explicit trusted session capability selects Syndrid orchestration, while
   old and Codex-compatible sessions retain the current path.
2. One immutable O6E policy and routing profile are captured per active run.
3. The existing coordinator is the only planning/execution/verification/repair
   authority.
4. Exact production provider, account, model, tool, sandbox, and approval
   semantics are preserved without silent fallback.
5. Running and cleanup snapshots reach the existing dashboard event path
   without blocking or failing execution.
6. Final assistant response and typed failures reach the normal transcript and
   turn lifecycle.
7. Cancellation, timeout, disconnect, and shutdown join all children and
   complete Phase 7E cleanup before final status.
8. No sensitive payload or hidden reasoning crosses the observation boundary.
9. Focused core, app-server, and TUI tests cover the production boundary and
   Codex compatibility.
10. Interactive validation demonstrates a harmless run, dashboard continuity,
    transcript continuity, and cancellation without a stuck `Working` state.

## 20. Explicit non-goals

This milestone does not implement the coordinator integration, change the
app-server protocol, add a provider system, add a tool runtime, redesign the
dashboard, add charts/forecasts/custom-mode editing, expose workflow APIs,
change Codex execution, or claim that production observations are currently
live. It also does not add account rotation, hidden reasoning display, raw role
transcripts, or a second orchestration runtime.

## 21. Open decisions for later implementation

1. Which existing crate can legally own the native scoped Codex invocation
   bridge without widening `AgentControl` unnecessarily?
2. Should the trusted execution capability be passed only through the embedded
   in-process app-server first, or is remote Syndrid execution required in the
   first production release?
3. Is the final response authored by a dedicated synthesis step, the executor,
   or a bounded main-role result, and what exact summary schema is needed?
4. Which native approval/tool API can enforce planner/verifier read-only and
   executor/repair write ceilings on every supported platform?
5. How should an active orchestrated turn handle `turn/steer`: reject, queue, or
   translate into a new explicit workflow action?
6. What minimal persistence/recovery record is required to resume after an
   interrupted orchestration without reusing a generation?
7. Is a remote observation notification needed, and if so, what optional v2
   schema and capability negotiation are acceptable?
8. How should provider usage be attributed to roles while preserving Codex’s
   existing token/accounting authority?
