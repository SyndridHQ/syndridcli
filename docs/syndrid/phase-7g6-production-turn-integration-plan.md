# Phase 7G6 — End-to-End Production Orchestration Turn Integration

## 1. Status and scope

Status: design-only integration plan. No production Syndrid turn is enabled by
this document.

Phase 7G6 will connect one trusted embedded Syndrid turn to the existing Phase
7 coordinator while preserving the current Codex path for every other caller.
The implementation must compose the contracts merged in 7G1–7G5; it must not
create another coordinator, provider system, lifecycle authority, transcript
system, or policy resolver.

The first supported surface is one embedded Syndrid TUI session. Remote
app-server clients, local daemon clients, older clients, and Codex-branded TUI
sessions remain on Codex compatibility execution.

The implementation is not ready to begin as one undivided change. Two
production contracts are blockers and should be completed in focused seams:
the live scoped Codex invocation used by role routes, and an explicit approved
final-response producer. The recommended split is in section 25.

## 2. Current Codex production path

The audited current path is:

```text
ChatWidget::submit_user_message_with_history_and_shell_escape_policy
  → AppCommand::UserTurn / AppEvent::CodexOp
  → App::submit_active_thread_op
  → App::submit_thread_op
  → App::try_submit_active_thread_op_via_app_server
  → AppServerSession::turn_start
  → ClientRequest::TurnStart
  → TurnProcessor::turn_start
  → TurnProcessor::turn_start_inner
  → CodexThread::submit_user_input_with_client_user_message_id
  → native Codex model/tool/approval loop
  → app-server item/token/tool/turn notifications
  → TUI event dispatch and transcript persistence
  → turn completion, failure, or interrupt
```

| Boundary | Current owner and concrete location | Input/output | Cancellation and busy state | Reuse classification |
| --- | --- | --- | --- | --- |
| Composer | `codex-rs/tui/src/chatwidget/input_submission.rs`, `ChatWidget::submit_user_message_with_history_and_shell_escape_policy` | Composer text becomes bounded `Vec<UserInput>` and `AppCommand::UserTurn`. | TUI owns input presentation; it does not own orchestration cancellation. | Presentation-specific input preparation. |
| TUI routing | `codex-rs/tui/src/app/thread_routing.rs`, `submit_active_thread_op`, `submit_thread_op`, `try_submit_active_thread_op_via_app_server` | Resolves thread, cwd, model, effort, permissions, approval, workspace roots, and starts or steers a turn. | Uses existing active-turn identity and `turn_interrupt`; TUI maintains local busy/transcript projections. | Reusable admission/request routing; no new execution authority. |
| Client transport | `codex-rs/tui/src/app_server_session.rs`, `AppServerSession::turn_start` and `turn_interrupt` | Builds `ClientRequest::TurnStart` and `ClientRequest::TurnInterrupt`. | Transport request/response; no orchestration ownership. | Reuse unchanged. |
| App-server admission | `codex-rs/app-server/src/request_processors/turn_processor.rs`, `TurnProcessor::turn_start_inner` | Validates protocol input, loads thread, applies settings, maps input to core `Op::UserInput`. | App-server owns active-turn admission and existing Codex interrupt mapping. | Correct future dispatch seam. |
| Codex execution | `codex-rs/core/src/codex_thread.rs`, `CodexThread::submit_user_input_with_client_user_message_id` | Starts the existing thread/turn agent loop. | Native Codex cancellation, approvals, sandbox, tools, persistence, and joins. | Compatibility runner must remain unchanged. |
| Notifications | App-server notification processors and TUI event dispatch | Emits assistant deltas, item completion, errors, tool events, and `TurnCompleted`. | App-server owns notification delivery; TUI consumes projections. | Existing transcript/event path is the target for 7G6. |

The current unconditional Codex entry is the call to
`CodexThread::submit_user_input_with_client_user_message_id` in
`TurnProcessor::turn_start_inner`. The 7G1 router is already evaluated before
thread loading there, but the Syndrid arm currently returns the typed
`Syndrid orchestration turn execution is not available yet` error.

The embedded TUI currently starts the in-process app-server with
`InProcessClientStartArgs` in `codex-rs/tui/src/lib.rs`; that structure does
not carry a production execution capability. The app-server in-process host
currently supplies the default Codex capability to `MessageProcessor`.

## 3. Merged 7G contract inventory

### 7G1 — selection

`codex-rs/app-server/src/production_turn.rs` defines:

- `ProductionExecutionCapability`, a typed trusted capability defaulting to
  `CodexCompatibility`;
- `ProductionTurnPath`, the immutable selected path;
- `ProductionTurnRouter`, a deterministic capability-to-path conversion.

`AppServerRuntimeOptions` owns the capability. It is not serialized, is not
derived from `PublicBrand`, executable name, prompt text, model, provider, or
execution mode, and is propagated into `MessageProcessor` and
`TurnProcessor`. Existing callers therefore remain Codex-compatible.

7G6 must add an explicit trusted embedded composition value. It must not infer
Syndrid from `PublicBrand`; branding may gate whether the composition root
offers the capability, but the capability itself is the execution authority.

### 7G2 — request, provider, and tool contracts

`codex-rs/core/src/syndrid_orchestration/production_request.rs` defines:

- `ProductionOrchestrationInput`, containing bounded run identity,
  instruction/context, workspace, task contracts, policy contracts, tool
  policy, cancellation, and timeout;
- `ProductionOrchestrationRequestBuilder`, which resolves and validates O6E
  policy and routing profile state before producing the existing
  `LiveOrchestrationRequest`;
- `ProductionProviderAdapter<P>`, which binds one exact route to an existing
  `ProviderInvocation` implementation and rejects provider/model mismatches;
- `ProductionProviderRoute`, which currently contains connection/provider/model
  selection and role effort.

`ProductionApprovedToolAdapter` in `subagent_tools.rs` delegates to the O6B
approved-tool runtime and preserves workspace, allowlist, output, and role
capability restrictions.

The request builder is reusable but its inputs are not yet supplied by a real
turn. It stores resolved policy/profile state in the builder and receives the
bounded turn payload separately. 7G6 must capture those values once at turn
admission and must not read mutable TUI selectors afterward.

### 7G3 — observations

`codex-rs/core/src/syndrid_orchestration/observation_delivery.rs` defines the
provider-neutral `OrchestrationObservationSink`, a no-op sink, and a bounded
Tokio `watch` channel retaining the latest `OrchestrationObservationUpdate`.
The watch sink assigns a per-sink sequence and preserves the snapshot's exact
generation. `LiveOrchestrationCoordinator::with_observation_sink` accepts it.

The app-server-client bridge converts the receiver to the existing embedded
event path:

```text
OrchestrationObservationUpdate
  → AppServerEvent::OrchestrationObservation
  → AppEvent::UpdateOrchestrationObservation
  → ChatWidget::update_orchestration_observation
  → SessionDashboardState
```

7G6 must create exactly one sink/receiver pair per run, attach the receiver's
bridge to the lifecycle, and close/join it only after the coordinator has
published the final post-cleanup snapshot. Watch semantics are latest-state,
not a timeline; slow consumers may miss intermediate snapshots.

### 7G4 — result and transcript translation

`codex-rs/core/src/syndrid_orchestration/turn_result.rs` defines:

- `UserFacingResponse`, bounded to 32 KiB of UTF-8 bytes;
- `OrchestrationOperationalMetadata`;
- `OrchestrationEvidence`, which is not transcript text;
- `OrchestrationTurnResult` variants for completed, partial, failed,
  cancelled, timed out, budget exhausted, and cleanup incomplete;
- `OrchestrationTurnResultBuilder`.

`codex-rs/app-server/src/orchestration_result.rs` defines
`translate_orchestration_result`, which emits existing assistant delta,
item-completed, error, and turn-completed notifications. It is deliberately
not connected to turn admission. It accepts a bounded response candidate, but
the current coordinator outcome does not itself contain a complete
user-facing response.

### 7G5 — lifecycle and cancellation

`ProductionOrchestrationLifecycle<T, E>` in
`codex-rs/core/src/syndrid_orchestration/production_lifecycle.rs` owns one
run's root `CancellationToken`, coordinator `JoinHandle`, optional bridge
`JoinHandle`, run identity, and bounded completion/shutdown state. It requests
cancellation, joins owned tasks, maps join failure to typed errors, and leaves
terminal-cause arbitration and cleanup to Phase 7E.

`ProductionOrchestrationCancellationRegistration` in
`codex-rs/app-server/src/production_cancellation.rs` associates a cancellation
handle with one exact turn ID. `ThreadState` stores an optional registration;
`turn_interrupt_inner` requests user cancellation only when the turn IDs
match, and listener cleanup requests session-shutdown cancellation.

The registration is not populated by any production Syndrid turn. Existing
Codex cancellation remains the active behavior.

## 4. Remaining blocker matrix

| Contract | Status | Evidence and required resolution |
| --- | --- | --- |
| Trusted activation from embedded Syndrid launcher | NARROW IMPLEMENTATION NEEDED | `PublicBrand` is available in `tui/src/lib.rs`, but `InProcessClientStartArgs` and `InProcessStartArgs` do not carry the capability. Add an explicit internal composition field only for the embedded local path. Remote and default clients must omit it and remain Codex. |
| Immutable production input capture | NARROW IMPLEMENTATION NEEDED | `ProductionOrchestrationInput` and the builder exist, but `turn_start_inner` does not construct them. Capture validated input, thread context, workspace, O6E state, profile, routes, tool envelopes, generation, timeout, and root token once. |
| O6E policy capture | READY | `SessionExecutionPolicyState` and `ResolvedExecutionPolicy` are authoritative. The runner must snapshot/validate them once and must not duplicate policy resolution. |
| Routing-profile capture | READY | `RoutingProfileRegistry`, `RoutingConnectionDirectory`, and builder validation exist. The runner must retain the validated profile snapshot for the run. |
| Exact multi-role provider dispatch | BLOCKED | The builder can derive one route per role and `ProductionProviderAdapter<P>` binds one route, but no production dispatcher supplies a distinct adapter for Planner, Executor, Verifier, and Repair in one coordinator. Add a narrow role-to-adapter dispatcher using existing invocation traits; no new registry or fallback. |
| Scoped native Codex invocation | BLOCKED | `CodexInvocationAdapter` and exact account metadata exist, but `UnavailableCodexInvocationClient` demonstrates the live scoped operation is not wired into the production adapter. Bind the existing `CodexThread`/agent-control/provider seam or explicitly reject Codex role routes before admission. |
| OpenRouter and OmniRoute invocation | NARROW IMPLEMENTATION NEEDED | `invoke_openrouter` and `invoke_omniroute` exist, but 7G6 must bind their existing credential/connection handles to role-specific adapters and test exact route identity. Unsupported or unavailable connections fail before coordinator work. |
| Approved role tools | READY | O6B adapter enforces role envelopes, workspace containment, allowlists, output bounds, and budgets. The runner must provide only pre-approved capabilities. |
| Interactive approval during orchestration | BLOCKED | The adapter does not own approvals, and no production bridge from role tool requests to app-server approval requests is defined. Initial 7G6 must either use envelopes that require no interactive approval or reject an approval-required operation with a typed failure. It must not bypass approval. |
| Conversation-context extraction | NARROW IMPLEMENTATION NEEDED | The turn processor has validated input and thread/config state, but no bounded orchestration context extractor is connected. Use an explicit bounded context provider; do not copy hidden instructions or unbounded transcript history. |
| Final synthesis ownership | BLOCKED | `LiveOrchestrationOutcome` and role outcomes contain operational metadata, not a user-facing answer. `OrchestrationTurnResultBuilder` accepts an optional candidate and otherwise emits a deterministic placeholder. 7G6 requires an approved finalizer or structured deliverable contract before claiming a successful answer. |
| Final assistant item identity | READY | `OrchestrationTranscriptContext` and existing notification types define the event identity. The runner must allocate one stable item ID using existing app-server conventions. |
| Observation bridge lifetime | READY | 7G3 receiver and bridge plus 7G5 lifecycle handle exist. The runner must attach the bridge before coordinator execution and join it after final publication. |
| Cancellation registration lifetime | NARROW IMPLEMENTATION NEEDED | Registration and matching exist, but no production caller registers/removes it around a real run. Registration must occur after turn ID admission and be removed in the single finalization path. |
| Busy/idle release | BLOCKED | Current Codex active-turn and TUI busy projections are driven by normal app-server notifications. No production runner owns release after coordinator, cleanup, result, bridge, and registration completion. Reuse the existing `TurnCompleted` path and add a narrow finalization guard before activation. |
| Usage/token attribution | NARROW IMPLEMENTATION NEEDED | Phase 7 budgets track structured provider/tool counts, while the normal Codex thread owns provider usage and transcript accounting. Exact multi-provider unified token/cost totals are not guaranteed. Report per-provider exact values where exposed; unified cost/quota remains Unavailable unless a trusted source exists. |
| Turn steering | DEFERRED OUTSIDE 7G6 | Existing active-turn steering targets a Codex turn. The first orchestration surface should reject steering or queue it through an explicit later contract rather than mutate an active immutable orchestration request. |
| Persistence and resume | DEFERRED OUTSIDE 7G6 | Ordinary translated assistant events can use existing persistence, but interrupted orchestration recovery and event-log reconstruction are not implemented. Do not claim resume of an in-flight orchestration run. |
| Session shutdown | READY WITH NARROW WIRING | 7G5 supplies bounded shutdown and ThreadState listener cleanup. The real runner must register its lifecycle and ensure shutdown completion is awaited by the existing session owner. |
| Existing Codex coexistence | READY | Default capability, explicit Codex capability, remote clients, and all current callers select the existing Codex path. |
| Remote activation | DEFERRED OUTSIDE 7G6 | No protocol field is required for embedded-only activation. Remote Syndrid requires a separately reviewed authenticated capability protocol. |

## 5. Initial supported production surface

7G6 should initially support only the embedded Syndrid TUI composition root.
The selection matrix is:

| Caller | Capability | Result |
| --- | --- | --- |
| Embedded Syndrid TUI, explicitly composed for Syndrid runtime | `SyndridOrchestration` | Eligible for the new runner after all admission blockers pass. |
| Embedded Codex TUI | `CodexCompatibility` | Existing Codex turn path unchanged. |
| Remote app-server client | omitted/unavailable | Codex compatibility; no remote activation. |
| Local daemon or older client | omitted/unavailable | Codex compatibility. |
| Any client with only `PublicBrand::Syndrid` presentation state | no trusted capability | Codex compatibility. |
| Explicit Syndrid capability while runner prerequisites are invalid | `SyndridOrchestration` | Typed unavailable/configuration error; never Codex fallback. |

The trusted source is the embedded composition function that already receives
the selected `PublicBrand` and constructs `InProcessClientStartArgs`. It may set
the capability only after an explicit Syndrid product/runtime gate, not merely
because a string or executable name matches. The capability should be passed
as an internal field through `app-server-client` to `AppServerRuntimeOptions`.
It must not enter `InitializeParams`, `ClientRequest`, rollout data, or any
remote protocol payload.

The embedded runner must capture the capability at session creation and the
selected `ProductionTurnPath` at turn admission. Pending TUI mode changes can
affect the next eligible turn only; they cannot mutate the active request.

## 6. Production turn-runner design

### Proposed abstraction

Name: `SyndridProductionTurnRunner`.

Owner: internal app-server turn-integration module, with a small core-facing
constructor contract if crate boundaries require it. It must not be a TUI
type, a public protocol type, or a second coordinator.

Inputs:

- admitted thread ID and new turn ID;
- immutable `ProductionTurnPath::SyndridOrchestration`;
- validated `TurnStartParams` mapped to bounded core input;
- thread/config/workspace/permission/approval state;
- captured O6E execution policy and routing profile;
- existing provider connection and credential handles;
- approved role capability envelopes;
- existing app-server outgoing event sender;
- existing app-server `ThreadState` and turn-finalization owner.

Outputs:

- existing response to `turn/start` after admission;
- existing `ServerNotification` sequence for translated result;
- lifecycle/typed failure status for internal logging and terminal mapping;
- no new remote protocol message.

The runner composes, in order:

1. Validate that the router selected Syndrid and reject any missing trusted
   capability before spawning work.
2. Admit the turn through the existing thread/turn identity and allocate one
   immutable run ID and O6E generation.
3. Capture bounded user input, approved context, workspace, policy, profile,
   route assignments, tool envelopes, deadline, and turn IDs. Store no raw
   credentials or hidden prompts in the run configuration.
4. Resolve and validate all role routes and provider/tool prerequisites before
   starting the coordinator. A missing route or unsupported invocation fails
   before task admission.
5. Create one observation channel and retain its sender in the coordinator.
6. Create the `ProductionOrchestrationLifecycle` with a coordinator future that
   calls the existing `LiveOrchestrationCoordinator::run`; attach the bridge
   handle before execution can publish observations.
7. Register the lifecycle cancellation handle in the existing `ThreadState`
   using the exact admitted turn ID.
8. Await lifecycle completion. The coordinator remains responsible for Phase
   7E terminal arbitration, budget terminalization, child cleanup, and the
   final cleanup snapshot.
9. Obtain an approved bounded final response candidate, build
   `OrchestrationTurnResult`, and pass it to the existing app-server
   translator.
10. Let the final transcript/turn events enter the existing outgoing path, then
    remove the cancellation registration and release active/busy state in one
    idempotent finalization guard.
11. Close the observation source and join the bridge after the final snapshot
    is published. If the lifecycle API is adjusted, preserve the same ordering
    and bounded shutdown semantics.

The exact ordering of steps 9–11 must be implemented as a single owner, not by
independent detached tasks. If `TurnCompleted` must be emitted after bridge
join for the app-server busy state, the translator should be invoked before
the final completion notification is sent; the runner must not emit a normal
completion before cleanup and all owned joins are accounted for.

### Task, cancellation, and failure ownership

The runner owns the lifecycle handle and registration. The lifecycle owns the
root token and task handles. The coordinator owns child-role cancellation,
terminal-cause arbitration, reservations, cleanup, and final observation.
The app-server owns turn identity, outgoing notifications, registration
removal, and busy-state release. The TUI only sends existing start/interrupt
requests and consumes notifications.

No provider, route, policy, tool, dashboard, or transcript logic belongs in
`ProductionOrchestrationLifecycle`.

## 7. Request construction

The runner needs a private immutable `ProductionOrchestrationRunConfig` (name
illustrative) to prevent mutable state leakage. It should contain:

```text
thread_id, turn_id, run_id, O6E generation
bounded user objective and approved context
canonical workspace root and effective permissions
resolved execution policy and routing profile snapshot
exact role routes and provider/account/connection handles
role-specific approved tool envelopes
planning/verification/failure/repair contracts
root cancellation token and bounded deadline
```

`ProductionOrchestrationInput` is then built from that snapshot and handed to
`ProductionOrchestrationRequestBuilder`. The builder remains the authority for
request validation, while O6E remains the authority for policy resolution.

The input source mapping is:

| Request value | Source and rule |
| --- | --- |
| Objective | Validated `TurnStartParams.input`, bounded and normalized by the app-server boundary. Never read from dashboard state. |
| Context | New bounded thread-context extractor using approved persisted context only. Hidden/system/developer instructions and raw role transcripts are excluded. |
| Workspace | Existing resolved cwd and runtime workspace roots, canonicalized and intersected with permissions. |
| Policy/mode | `SessionExecutionPolicyState` snapshot at admission; no later selector reads. |
| Routing | Validated `RoutingProfileRegistry` and `RoutingConnectionDirectory` snapshot. |
| Tasks/contracts | Existing Phase 7 planning, verification, failure, and repair contracts; no arbitrary user-supplied workflow authority. |
| Tools | O6B role envelopes intersected with native Codex sandbox/approval restrictions. |
| Cancellation | Root token created by 7G5 lifecycle owner. |
| Deadline | O6E policy timeout bounded by the existing app-server/session lifetime. |

Validation failures occur before provider invocation and are translated to a
bounded failed turn result. They do not silently execute Codex.

## 8. Multi-role provider dispatch

The current adapter binds one `ProductionProviderRoute` to one provider
implementation. A real run needs a role dispatcher:

```text
RoutingRole
  → exact ProductionProviderRoute
  → one existing ProviderInvocation implementation
  → ProductionProviderAdapter for that route
```

The dispatcher is an internal core orchestration adapter. It receives an
immutable route map from the runner and returns a typed unsupported/unavailable
error when a role route cannot be bound. It must not be a registry that makes
new routing decisions. It must not rotate accounts, retry outside existing
policy, change model/effort, or fall back from Codex to OpenRouter/OmniRoute.

The dispatcher must validate:

- planner, executor, verifier, repair, and main assignments required by the
  resolved policy;
- exact connection/provider/model identity;
- exact role effort;
- exact account selection where the provider uses an account registry;
- credential availability without putting credential material in errors;
- provider-specific cancellation and output bounds.

OpenRouter and OmniRoute have existing invocation functions and can be enabled
only after their production connection/credential handles are supplied by the
runner. Native Codex has an exact account-selection adapter and scoped-session
tests, but the live `UnavailableCodexInvocationClient` seam is a blocker. A
Codex role must fail admission until a real existing-thread/agent-control
invocation bridge is selected and tested.

## 9. Tool and approval behavior

Initial 7G6 should support only role envelopes that are already fully
authorized by the session and require no new interactive approval round-trip.
The envelopes are:

| Role | Initial capability ceiling |
| --- | --- |
| Planner | Read-only inspection and bounded metadata; no writes. |
| Executor | Existing authorized file/shell capabilities within workspace and native sandbox/approval restrictions; only one writer. |
| Verifier | Read-only inspection and bounded validation commands. |
| Repair | Executor-like write ceiling, but only for the bounded repair scope and configured repair count. |

Every role is subject to the intersection of user, session, workflow, parent,
role, task, sandbox, workspace, path, tool-budget, and output limits. The
adapter must reuse O6B and native Codex enforcement; it must not grant
arbitrary shell or network access.

If a role requests an operation that requires an approval interaction not yet
bridged to app-server, the runner returns a typed unavailable/approval-required
failure. It must not auto-approve and must not bypass the normal approval
engine. Interactive orchestration approvals are a separate follow-up if this
constraint makes the supported surface too narrow.

## 10. Final-response ownership

7G4 is not sufficient to claim successful user-facing output. The coordinator
returns role/lifecycle metadata and structured evidence, while the result
builder accepts an optional `UserFacingResponse` candidate. Its deterministic
fallback text is operational status, not a completed assistant answer.

The preferred 7G6 contract is a bounded `FinalSynthesis`/`UserFacingDeliverable`
result produced by an existing approved role output, in this order:

1. An explicit structured executor deliverable that is already marked
   user-facing and bounded.
2. A verifier-approved structured summary that explicitly guarantees it is
   user-facing and contains no hidden reasoning, raw role transcript, provider
   payload, or raw tool output.
3. A narrow final-synthesis invocation through the already-selected provider
   dispatcher, only if O6E policy explicitly permits it and its budget is
   reserved separately.

No current type proves that planner, executor, verifier, or repair text is
user-facing. Therefore final synthesis is a blocker for a full successful
surface. Until resolved, 7G6 may validate failure/cancellation paths and a
no-op/investigation result, but must not present a fabricated successful answer.

The finalizer's input is structured evidence and bounded deliverables, not raw
transcripts. Its output is `UserFacingResponse`; it has a hard 32 KiB UTF-8
bound and no mutable unchecked construction path. It must preserve partial,
failed, timeout, budget, cancellation, and cleanup-incomplete classifications.

## 11. Observation lifecycle

One run owns one `watch` sink/receiver pair:

```text
runner creates channel
  → coordinator receives sink
  → coordinator publishes authoritative progress and final cleanup snapshot
  → bridge receives latest updates
  → bridge sends AppServerEvent::OrchestrationObservation
  → existing in-process event conversion sends AppEvent::UpdateOrchestrationObservation
  → dashboard accepts by generation/sequence
```

The coordinator's `OrchestrationObservationCollector` and Phase 7E cleanup
remain the only runtime-fact authorities. The runner never constructs a
synthetic snapshot. The bridge copies generation and sequence exactly. The
watch channel is latest-state and bounded; it does not promise event history.

The sink must be created before coordinator execution and the bridge handle
attached to the lifecycle before progress can be published. On normal
completion, coordinator completion and final publication precede source close,
bridge exit, and bridge join. On forced shutdown, cancellation and the bounded
abort path may supersede delivery, but the terminal state must remain typed.
Closed dashboard or TUI receivers are non-fatal.

## 12. Cancellation lifecycle

The root token and registration sequence is:

```text
turn admitted with exact turn_id
  → ProductionOrchestrationLifecycle creates root token
  → coordinator receives token
  → planner/executor/verifier/repair and approved tools observe child cancellation
  → ThreadState registers handle for exact turn_id
  → matching turn/interrupt requests User cancellation
  → session shutdown requests SessionShutdown
  → coordinator performs Phase 7E arbitration and cleanup
  → lifecycle joins coordinator and bridge within bound
  → registration is removed
  → final result/turn completion is emitted
  → busy state is released
```

Timeout must be selected by the coordinator/policy and remain distinct from
user cancellation. Duplicate interrupts are harmless; a wrong turn ID cannot
cancel another run. The lifecycle does not choose terminal causes or release
reservations.

## 13. Result and transcript translation

The runner must use the existing `translate_orchestration_result` event path.
The intended event sequences are:

| Result | Event sequence | Terminal status |
| --- | --- | --- |
| Completed | One `AgentMessageDelta`, matching `ItemCompleted`, then `TurnCompleted` | `Completed` |
| Partial | Bounded prefixed assistant message, matching item completion, then `TurnCompleted` using existing non-unqualified semantics | Must not look like full success; exact status requires existing protocol mapping review. |
| Failed | One bounded `Error`, then `TurnCompleted` with failure | `Failed` |
| Cancelled | No fabricated assistant message; interrupted `TurnCompleted` | `Interrupted` |
| Timed out | Bounded timeout `Error`, then failed `TurnCompleted` | `Failed` |
| Budget exhausted | Bounded budget `Error`, then failed `TurnCompleted` | `Failed` |
| Cleanup incomplete | Useful bounded response only when safe, plus explicit cleanup failure `Error`, then failed `TurnCompleted` | `Failed` |

The runner must allocate one stable assistant item ID and ensure the final
message precedes item completion and turn completion. It must not emit both a
success assistant message and a failure for one result. Observation snapshots
and `OrchestrationEvidence` are not transcript items.

Ordinary translated assistant items can use existing persistence. No rollout or
remote protocol schema changes are required. Resume of an interrupted active
orchestration run remains unsupported; the persisted state must be marked or
left in an existing terminal-safe form rather than replaying a duplicate final
message.

## 14. Busy/idle ownership

App-server remains the owner of active-turn admission and `TurnCompleted`.
The runner must not invent a second busy flag. The finalization guard must run
only after:

1. coordinator result and Phase 7E cleanup are complete or classified
   cleanup-incomplete;
2. final observation has been published;
3. result translation has produced the required terminal notifications;
4. observation bridge has closed and joined, subject to bounded forced shutdown;
5. cancellation registration has been removed;
6. all owned task handles have been accounted for.

This ordering must release busy state for success, cancellation, provider/tool
failure, verification failure, repair exhaustion, timeout, budget exhaustion,
cleanup incomplete, join failure, and closed observation destinations. If the
existing Codex `ThreadState` completion path cannot represent this ordering,
7G6 must add the smallest internal finalization hook rather than a parallel
registry.

## 15. Session shutdown

The existing session shutdown path must call the registered lifecycle's bounded
shutdown exactly once for a matching run. Shutdown requests
`SessionShutdown`, allows Phase 7E cleanup to run, waits for final observation
publication where the bound permits, joins the bridge, removes registration,
and accounts for all handles. The default 7G5 bound is 30 seconds; the runner
must use an explicit session-bound timeout and must not wait forever.

If a shutdown bound expires, the result is a typed cleanup/shutdown failure,
owned handles are forced through the existing bounded abort path, and no user
message claims successful orchestration. Unrelated Codex turns are not
cancelled.

## 16. Persistence behavior

The final assistant result is persisted through ordinary app-server/Codex
assistant item events. Observation snapshots are dashboard projections only;
they are not assistant messages. Internal evidence, role transcripts, raw
provider payloads, and raw tool output are not persisted in the transcript.

7G6 does not add interrupted-run recovery, a new rollout format, or an
orchestration event log. Resume behavior for an already completed translated
turn remains ordinary Codex replay. Resume of an interrupted active
orchestration is explicitly deferred to the persistence/recovery milestone.

## 17. Usage attribution

| Metric | 7G6 reporting quality | Rule |
| --- | --- | --- |
| Per-provider invocation count | EXACT | Coordinator/provider adapters provide structured counts. |
| Per-role tool-call count | EXACT | Existing orchestration counters and bounded tool results. |
| O6E execution/tool/output budget consumption | EXACT or DERIVED | Use the existing budget ledger and policy definitions. |
| Provider-reported input/output/cached tokens | EXACT when the selected provider exposes them | Preserve provider-scoped attribution; cached input is not counted twice. |
| Unified cross-provider token total | UNAVAILABLE unless all providers expose compatible accounting | Do not add incompatible provider totals. |
| Unified cost/quota/reset forecast | UNAVAILABLE or ESTIMATED only with an identified trusted source | Never fabricate exact cost, quota, reset, or savings. |
| Dashboard lifecycle/stage | EXACT from Phase 7D snapshot | Do not derive it from transcript events. |
| Completion speedup versus Single Mode | UNAVAILABLE initially | No baseline comparison is produced by 7G6. |

Budget enforcement may proceed using the existing ledger even where display
metrics are unavailable. The result and dashboard must preserve the data
quality labels.

## 18. Failure matrix

| Failure | Owner/classification | Cleanup and user behavior |
| --- | --- | --- |
| Missing trusted capability | App-server admission | Fail the explicit request; no fallback; current callers remain Codex. |
| Invalid policy/profile/role route | Request builder/O6E | Fail before provider task spawn; typed bounded failure; no registration leak. |
| Provider connection/auth unavailable | Provider dispatcher | Typed provider failure; Phase 7E cleanup; bounded error without credentials. |
| Scoped Codex invocation unavailable | Provider dispatcher | Typed unsupported failure before execution; do not use another account/provider. |
| Tool envelope/workspace violation | O6B adapter | Typed tool failure; no permission bypass; Phase 7E cleanup. |
| Approval unavailable | Runner/tool boundary | Typed approval-required/unavailable result; no auto-approval. |
| Coordinator failure | Coordinator/lifecycle | Terminal failure preserved; cleanup and joins required; no success response. |
| User cancellation | Lifecycle and coordinator | `User` reason, child cancellation, Phase 7E cleanup, interrupted result. |
| Timeout | Coordinator policy/lifecycle | Distinct timeout terminal cause, cleanup, failed timeout result. |
| Budget exhausted | Budget ledger/coordinator | Distinct budget category, cleanup, bounded budget message. |
| Verification rejection | Phase 7E/verifier | Typed failure or bounded partial/repair-exhausted result. |
| Repair exhaustion | Phase 7E/repair | Partial or failed typed result; never unqualified success. |
| Cleanup incomplete | Phase 7E | Cleanup-incomplete result and failed terminal status. |
| Observation receiver closed | Sink/bridge | Drop/stop forwarding; result unchanged; no task leak. |
| Bridge join failure | Lifecycle | Typed lifecycle failure; no unqualified success. |
| Coordinator panic/join failure | Lifecycle | Typed join failure; registration removed after bounded accounting. |
| App-server destination saturation/disconnect | App-server transport | Existing bounded transport behavior; do not block coordinator indefinitely. |
| Session shutdown | Lifecycle owner | SessionShutdown cancellation, bounded joins, safe terminal status. |
| Final response unavailable | Finalizer/result builder | Block full-success claim; use partial/operational result according to explicit contract. |

## 19. Exact lifecycle sequences

### Successful supported turn

```text
embedded Syndrid composition sets trusted capability
→ turn/start admits exact thread and turn IDs
→ ProductionTurnRouter selects SyndridOrchestration
→ capture O6E generation, policy, route, workspace, tools, context, and deadline
→ validate all role/provider/tool prerequisites
→ create bounded observation channel
→ create lifecycle/root token and coordinator future
→ spawn and attach observation bridge
→ register exact turn ID cancellation handle
→ coordinator publishes authoritative progress
→ planner → executor → verifier (and bounded repair if permitted)
→ Phase 7E selects terminal cause and completes cleanup
→ final cleanup observation is published
→ lifecycle joins coordinator
→ approved final response candidate is bounded
→ OrchestrationTurnResultBuilder builds Completed only when synthesis is authorized
→ existing assistant delta → item completion → turn completion events
→ close observation source and join bridge
→ remove registration
→ release existing busy/active state
```

### User cancellation

```text
existing turn/interrupt with exact turn ID
→ ThreadState registration requests User cancellation
→ root token cancels coordinator child work
→ Phase 7E arbitrates cancellation and performs cleanup
→ final cleanup observation publishes
→ lifecycle joins coordinator and bridge within bound
→ Cancelled result → existing interrupted TurnCompleted
→ remove registration and release busy state
```

### Pre-spawn validation failure

```text
turn admission and trusted path selection
→ request/policy/route/tool validation fails
→ no coordinator, bridge, or cancellation registration spawned
→ bounded failed result through existing turn error/completion path
→ release busy state
```

### Provider/coordinator failure, timeout, budget, cleanup, join, and shutdown

All follow the same invariant: the typed terminal cause is preserved, Phase 7E
cleanup runs when the coordinator is active, final observation is published
when available, result translation emits no fabricated success, every owned
handle is joined or bounded-aborted, registration is removed, and busy state is
released. Timeout, budget, cancellation, cleanup-incomplete, and join failure
remain distinguishable.

## 20. Test architecture

Use fake provider invocation and approved-tool implementations; no external
network or credentials. Prefer app-server integration tests with the in-process
runtime and deterministic fake coordinator/provider seams.

### Route and trust tests

- embedded trusted Syndrid capability selects orchestration;
- Codex, missing, remote, and older callers select Codex;
- `PublicBrand` alone never activates orchestration;
- explicit Syndrid selection fails clearly when prerequisites are unavailable;
- no fallback from explicit Syndrid to Codex;
- selected route cannot change during a turn.

### Request and provider tests

- immutable request is built once from exact turn state;
- policy/profile/role validation fails before provider invocation;
- all role routes preserve provider, connection, account, model, and effort;
- provider dispatch invokes the selected adapter once per requested route;
- unavailable scoped Codex invocation is typed and does not rotate accounts;
- tool envelope, workspace, allowlist, approval, budget, and output bounds hold.

### Successful-turn tests

- coordinator is invoked exactly once;
- observations reach the embedded bridge with unchanged generation/sequence;
- final cleanup observation is available before bridge join;
- approved final response reaches existing transcript events exactly once;
- item IDs and event ordering match ordinary Codex turns;
- registration is removed and busy state is released only after finalization;
- all coordinator and bridge tasks are joined.

### Failure and cancellation tests

- invalid policy/profile/route fails before spawn;
- provider, tool, verifier, repair, timeout, budget, cleanup, and join failures
  preserve their typed classifications;
- matching interrupt cancels, wrong turn does not, duplicate interrupt is safe;
- session shutdown cancels and joins within a bound;
- closed observation receiver does not affect the result;
- no detached task, registration, or busy state remains after every terminal
  path.

### Compatibility and privacy tests

- existing Codex turn and interrupt tests remain unchanged;
- no protocol serialization changes are required;
- observations contain only structured privacy-safe fields;
- transcript events contain no internal evidence, credentials, prompts, hidden
  reasoning, raw provider payloads, or raw tool output;
- ordinary assistant persistence/replay does not duplicate a translated result.

## 21. Compatibility guarantees

- Default and missing capabilities remain Codex compatibility.
- `PublicBrand` remains presentation-only.
- Remote and older app-server clients cannot activate local Syndrid execution.
- Existing Codex thread/tool/approval/sandbox/persistence/cancellation paths are
  unchanged for Codex-selected turns.
- No remote protocol or rollout schema change is required for embedded-only
  activation.
- Explicit Syndrid selection never silently falls back to Codex.
- Dashboard visibility does not control execution or observation production.
- No raw role output is sent to the transcript or dashboard.

## 22. Security and privacy constraints

The runner and all adapters must preserve the existing effective-permission
intersection and workspace containment. Credentials are resolved behind
existing providers and never copied into request debug output, observations,
errors, or transcript events. User/system/developer prompts, hidden reasoning,
role transcripts, raw HTTP/provider responses, raw tool output, and sensitive
file contents are excluded from observations and final responses.

Every user-facing string is bounded and deterministic. Internal errors are
mapped to typed public classifications; raw `Debug`, stack traces, and provider
payloads are not emitted. Account and model changes are never silent.

## 23. Implementation file map

The following is a proposed narrow map and must be validated against the
current crate dependency direction before implementation:

| File/crate | Planned change |
| --- | --- |
| `codex-rs/tui/src/lib.rs` | Pass an explicit internal capability only for embedded Syndrid composition; preserve Codex and remote construction. No dashboard changes. |
| `codex-rs/app-server-client/src/lib.rs` | Carry the capability through the in-process-only startup args; do not serialize it or expose it to remote clients. |
| `codex-rs/app-server/src/in_process.rs` | Propagate the internal capability into `AppServerRuntimeOptions`/`MessageProcessor`. Existing default remains Codex. |
| `codex-rs/app-server/src/request_processors/turn_processor.rs` | Replace the explicit unavailable Syndrid arm with the internal runner call only after all blockers are resolved; retain current Codex arm byte-for-byte where practical. |
| `codex-rs/app-server/src/thread_state.rs` | Register/remove the lifecycle and add one idempotent finalization hook if existing turn state cannot express the required ordering. |
| `codex-rs/app-server/src/production_orchestration_turn.rs` (proposed) | App-server-owned runner composition, immutable run config capture, result/event finalization, and registration lifetime. Keep under module-size limits. |
| `codex-rs/core/src/syndrid_orchestration/production_dispatch.rs` (proposed) | Narrow role-to-existing-provider invocation dispatch; no new registry or policy authority. |
| `codex-rs/core/src/syndrid_orchestration/production_request.rs` | Only narrow input/route contract changes proven necessary by the runner; preserve builder validation. |
| `codex-rs/core/src/syndrid_orchestration/production_lifecycle.rs` | Only narrow lifecycle API adjustment if bridge closure/join ordering cannot be expressed; preserve 7G5 ownership. |
| `codex-rs/core/src/syndrid_orchestration/turn_result.rs` | Add only the approved final-synthesis/deliverable contract; do not weaken bounded response rules. |
| `codex-rs/core/src/syndrid_orchestration/codex_invocation.rs` and existing session seam | Bind the exact existing scoped invocation; do not add a second provider runtime. |
| Existing app-server-client/TUI event forwarding | Reuse current notification/event conversion; no remote protocol file changes. |

Likely tests belong beside each new module and in app-server in-process tests.
No Cargo or protocol schema changes are expected for the embedded-only first
surface.

## 24. Acceptance criteria

7G6 is complete only when all of the following are evidenced:

1. Only the trusted embedded Syndrid composition can select the Syndrid path.
2. Codex, missing-capability, remote, and older callers execute unchanged.
3. One real embedded Syndrid turn constructs one immutable request.
4. All required role routes validate before task spawn and preserve exact
   provider/connection/account/model/effort identity.
5. A production-capable provider invocation exists for every initially enabled
   route, or unsupported routes fail before admission.
6. Approved tools enforce role ceilings, workspace, approvals, budgets, and
   output bounds.
7. A real coordinator invocation is owned by 7G5 lifecycle and no task is
   detached.
8. Progress and final cleanup observations reach the embedded TUI bridge with
   generation/sequence intact.
9. A typed user-facing final response exists; no placeholder is presented as
   successful work.
10. Success, partial, failure, cancellation, timeout, budget, cleanup, and
    join outcomes map once to existing transcript/terminal events.
11. Interrupt, shutdown, timeout, and receiver closure leave no registration,
    task, or busy-state leak.
12. Ordinary assistant persistence/replay remains compatible.
13. Focused tests provide evidence for route trust, request/provider/tool
    behavior, observations, results, cancellation, joins, cleanup, and Codex
    compatibility.
14. Privacy review finds no credentials, prompts, hidden reasoning, raw role
    transcripts, raw provider responses, or raw tool output in user-visible or
    observation events.

## 25. Recommended milestone split

Do not implement the whole plan as one PR while the two blockers remain.

### 7G6A — Production role dispatch and scoped invocation

Goal: make exact role routes production-capable without activating turns.

Scope: internal embedded capability plumbing if needed for tests, immutable
route-map/dispatcher, native scoped Codex invocation binding, OpenRouter and
OmniRoute existing seams, typed pre-admission failures, and provider/tool
contract tests.

Out of scope: real turn dispatch, observation bridge activation, final result,
busy-state integration, and remote protocol.

Acceptance: a fake/adaptive dispatcher can invoke each enabled route exactly,
and every unsupported route fails without fallback or account rotation.

### 7G6B — Final deliverable and embedded runner composition

Goal: define an approved user-facing deliverable and compose request,
coordinator, observation sink/bridge, result builder, lifecycle, registration,
and existing transcript events behind an internal runner.

Scope: bounded final-synthesis/deliverable contract, context extraction,
pre-admission validation, in-process-only capability propagation, runner tests,
and deterministic fake-coordinator end-to-end tests.

Out of scope: remote activation, interrupted-run recovery, dashboard redesign,
and broad approval protocol work.

Acceptance: deterministic fake runs exercise success, partial, failure,
cancel, timeout, budget, cleanup, bridge closure, joins, persistence events,
registration removal, and busy release without invoking real providers.

### 7G6C — Single embedded production activation and validation

Goal: replace the explicit unavailable Syndrid arm for one narrowly supported
embedded workflow and validate it with a harmless real task.

Scope: production runner activation, exact app-server finalization wiring,
focused in-process integration tests, TTY validation, and rollback guard for
Codex compatibility.

Out of scope: remote Syndrid capability, steering, recovery, new dashboard
features, automatic routing expansion, and multi-writer workflows.

Acceptance: one real embedded Syndrid turn reaches coordinator, observations,
final transcript, cancellation/cleanup, and idle state; Codex and remote paths
remain unchanged.

### Deferred follow-ups

Interactive orchestration approvals, remote authenticated capability,
interrupted-run recovery/resume, unified cross-provider usage/cost reporting,
turn steering, and broader workflow surfaces remain separate milestones unless
7G6C proves an existing contract already supports them safely.

## 26. Explicit non-goals

- No second agent or provider runtime.
- No new app-server protocol field or remote Syndrid activation.
- No `PublicBrand`-controlled execution.
- No silent provider/account/model/effort fallback.
- No fabricated final response, cost, quota, speedup, or token total.
- No raw role transcript, hidden reasoning, prompt, credential, provider, or
  tool-output exposure.
- No dashboard redesign, new metrics, or observation authority.
- No persistence/recovery migration for interrupted orchestration.
- No turn steering implementation.
- No change to the existing Codex path.

## 27. Open blockers and decisions

1. Which existing native Codex thread/agent-control seam can perform a scoped
   role invocation while preserving the selected account and session
   semantics?
2. Which exact structured deliverable is authorized as the final user-facing
   response, or is one bounded synthesis invocation approved by O6E budgets?
3. What bounded context extractor is allowed to supply thread/project context
   without copying hidden instructions or unbounded transcript history?
4. Which role tool operations can be safely supported without an interactive
   approval bridge?
5. Can existing `ThreadState`/turn notification ordering release busy state
   after bridge join, or is a small internal finalization hook required?
6. Which provider usage fields are authoritative enough to expose per role,
   and which must remain Unavailable?
7. Should the initial embedded activation be gated by a dedicated trusted
   runtime constructor/configuration value in addition to the product launcher
   gate?

These decisions must be answered with inspected code and focused tests before
7G6C activation. No question should be resolved by executable-name detection,
prompt inference, fabricated defaults, or silent fallback.
