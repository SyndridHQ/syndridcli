# Syndrid Orchestration Architecture

> Status: Architecture foundation  
> Runtime foundation: OpenAI Codex  
> Current implementation state: Documentation and integration-audit phase  
> Initial maximum concurrency: 2  
> Initial writer limit: 1  
> Default compatibility mode: Single  
> Authoritative reference manifest: `SYNDRID_REFERENCE_REPOS.md`  
> Last reviewed commit: `1ccc6da035bda2991acfa4f9fd567eb1b0c122ae`  
> Last reviewed date: 2026-07-21

This document is the product and architecture authority for Syndrid orchestration. Future implementation must revalidate this architecture against the current local Codex codebase before assuming exact integration seams. It is a specification, not an implementation plan that authorizes code changes by itself.

## Product definition

Syndrid is a Rust coding-agent CLI and harness derived from OpenAI Codex. Its goal is:

**Codex execution compatibility** + **strong task planning and selective delegation** + **Syndrid-specific model control, effort control, usage budgeting, forecasting, verification, transparency, and adaptive efficiency.**

The system optimizes for correctness, low latency, useful output throughput, minimal unnecessary model calls, minimal duplicate context, bounded memory and retries, transparent usage, observable verification, safe permission inheritance, graceful cancellation, recovery after interruption, compatibility with normal Codex behavior, and conservative usage for limited-plan users, including ChatGPT Plus users. Syndrid does not claim guaranteed superiority over any competing coding agent.

## Runtime and control-plane boundary

```text
Codex runtime and data plane
↓
Syndrid orchestration control plane
↓
Syndrid TUI and user controls
```

Codex remains responsible for model execution and inference requests, model responses, threads, turns, conversation and session state, shell and tool execution, file operations, sandboxing, approvals, MCP, event production, cancellation primitives, token and context accounting, rollout/session persistence, configuration, existing child-agent or thread-forking primitives, and app-server APIs where applicable.

Syndrid is responsible for orchestration-mode policy, task classification, planning, selective delegation, workflow state, roles, scheduling, limited concurrency, model and reasoning-effort routing, workflow usage budgets, Adaptive Efficiency, recommendations, forecasts, structured handoffs, permission intersection, role ceilings, verification requirements and evidence, bounded repair, orchestration recovery, event projections, and TUI observability.

Syndrid must not duplicate Codex's model runtime, tool runtime, sandbox, approval engine, persistence engine, token-accounting truth, session truth, or thread truth. Codex remains the execution and data-plane runtime; Syndrid coordinates and projects it.

## Orchestration modes

### Single Mode

Single Mode preserves normal Codex behavior: one main coding agent; no separate Syndrid planner, verifier, or Syndrid-created subagents; no workflow multiplier; no additional orchestration model calls; and no coordination overhead beyond negligible mode dispatch. Ordinary Codex approvals, tools, sandboxing, persistence, and session behavior remain unchanged.

It is used for small fixes, simple questions, short edits, straightforward debugging, users who prefer ordinary Codex behavior, and work where delegation costs more than it saves. It must remain available, the compatibility/performance/usage baseline, and materially unaffected by new orchestration infrastructure. Dispatch must not introduce unnecessary context or model calls.

### Manual Mode

Manual Mode gives the user direct control over whether subagents are used, agent count and roles, model and reasoning effort per agent, workflow order, sequential or limited parallel execution, maximum concurrency, verification depth, retry and repair limits, usage-budget multiplier, optional exploration, permission envelope, and task-specific instructions.

Initial roles are Planner, Explorer, Executor, and Verifier. Manual Mode still enforces configured and global concurrency limits, one writer per worktree, no uncontrolled recursive delegation or arbitrary swarms, bounded retries and repair, permission and budget ceilings, cancellation, event recording, and verification evidence. Initial global concurrency is 2.

Example configuration: Planner GPT-5.6 Sol High; Executor GPT-5.6 Sol Medium; Verifier GPT-5.6 Luna High; maximum concurrency 1; usage ceiling 1.20× estimated Single Mode usage; repair limit 1; simultaneous writers prohibited. Manual Mode serves advanced users, precise workflows, benchmarking, orchestration debugging, routing experiments, and users who already know their desired team.

### Recommended Mode

Recommended Mode analyzes a task and proposes a workflow, then waits for confirmation before launching it. The proposal states whether orchestration is useful or Single Mode is preferable; roles and count; model and effort per role; order; concurrency; verification depth; retry and repair limits; usage multiplier; predicted total usage, overhead, completion-time effect, and latency effect; confidence; data-quality labels; reasons; and major uncertainties.

The user may approve, edit, reject, select Single Mode, switch to Manual Mode, change models or effort, alter concurrency or verification, or change the multiplier. Recommended Mode must not execute before confirmation. Recommendations are forecasts, not guarantees.

Example:

- Main executor: GPT-5.6 Sol, Medium
- Read-only explorer: GPT-5.6 Luna, Low
- Verifier: GPT-5.6 Luna, Medium
- Maximum concurrency: 2
- Predicted usage: 1.08× Single Mode
- Predicted completion-time improvement: 19%
- Confidence: Medium
- Usage quality: Estimated

### Automatic Mode

Automatic Mode chooses the workflow under a user-selected usage-budget ceiling. Initial presets are 1.00× (Single-agent target), 1.10× (Light acceleration), 1.25× (Balanced), 1.50× (Aggressive), and 2.00× (Maximum permitted acceleration). A bounded custom multiplier may be supported only after safe minimum and maximum limits are defined.

The ceiling is `estimated Single Mode baseline × selected usage multiplier`. It is a ceiling, not a spending target. Automatic Mode chooses whether to remain Single, whether planning, exploration, or separate verification is useful, model and effort per role, concurrency, context allocation, verification depth, retry limit, and repair limit.

It defaults to one main agent; avoids orchestration when benefit is uncertain; adds agents only when expected benefit exceeds token, latency, and coordination overhead; uses narrower/lower-cost agents for narrow work; avoids complete-transcript duplication; stops optional exploration near the ceiling; reduces optional verification when necessary without falsifying success; never skips mandatory verification; prohibits swarms and unlimited retries; caps concurrency at 2; permits one writer; and exposes decisions and forecasts in the TUI.

Example at 1.10×: Sol Medium executor, Luna Low read-only explorer, no separate second verification pass, concurrency 2, predicted usage 1.07×, predicted time improvement 16%, Medium confidence, Estimated forecast quality.

### Adaptive Efficiency Mode

Adaptive Efficiency optimizes usage across the remaining provider quota period, not only the current task. When available, inputs may include exact or estimated usage remaining, reset date/time, time until reset, protected reserve, recent burn rate, historical daily usage and task costs, task classification, model and effort history, agent/planning/exploration/verification/repair overhead, failure rate, task priority and complexity, and likely future demand.

Conceptually:

```text
usable remaining = remaining allowance - protected reserve
safe period burn = usable remaining / time until reset
```

It compares safe burn, recent actual burn, predicted current-task cost, historical similar-task cost, predicted future demand, and reserve policy. It may adjust the effective multiplier, roles, model, effort, concurrency, exploration and verification depth, retry/repair limits, context size, and optional work.

Example: with estimated remaining usage 42%, reset in 6 days, reserve 12%, recent burn 9.4%/day, and safe target approximately 5%/day, a policy might select Sol Medium, a narrow Luna Low Explorer, concurrency 2, effective ceiling 1.08×, one mandatory verification, no optional second pass, one repair, Medium confidence, and Estimated quota quality.

When exact quota data is unavailable, use local history, label quota values Estimated or Unavailable, apply a larger reserve, behave conservatively, never present a guessed reset as exact, allow user-supplied reset/allowance information, and distinguish user-provided from observed information.

## Usage-budget system

`UsageBudgetMultiplier` is a bounded multiplier applied to the estimated Single Mode baseline. A `WorkflowBudget` contains the baseline estimate, selected multiplier, maximum permitted usage, actual attributed usage, remaining usage, projected final usage, optional-work allowance, reserve, confidence, and data quality.

Track input tokens, cached input tokens, uncached input tokens where available, output tokens, reasoning tokens where exposed, total provider-counted usage where exposed, usage by workflow/task/agent/stage, and orchestration overhead. Cached input tokens must never be counted twice, and provider accounting fields must not be assumed uniform.

Data-quality labels are explicit:

- **Exact:** directly supplied by a trusted runtime or provider.
- **Derived:** calculated deterministically from identified exact/source data.
- **Estimated:** predicted, inferred, sampled, unverified user-provided, or historical.
- **Unavailable:** not exposed or not reliably inferable.

## Initial production workflow

The first production workflow is **Planner → Executor → Verifier**, sequential by default.

The Planner inspects scope, relevant files, risks, validation requirements, success criteria, and a bounded execution plan without modifying files. The Executor implements the plan as the only writer, stays within permissions and budget, and records changed files, commands, failures, and a structured handoff. The Verifier inspects the diff and repository state, runs or inspects relevant tests/builds/linting, evaluates success criteria, records evidence and failures, and requests no more than the configured repair limit.

Initial constraints are concurrency 2 globally, one writer, one initial repair attempt, no arbitrary DAG, no recursive swarm, no hidden chain-of-thought display, and no unbounded transcript copying. This workflow may begin with concurrency 1 for the sequential O4 implementation.

## Optional Explorer

Explorer is read-only. It may search and inspect the repository and architecture, investigate failures, identify candidate locations, and summarize findings in a structured handoff. It must not modify, stage, commit, push, change configuration, widen permissions, or spawn unrestricted descendants. Create it only when expected time savings exceed coordination and usage overhead. Read-only must be technically enforced where the runtime supports it.

## Structured handoffs

Agents do not automatically receive full transcripts. A bounded `StructuredHandoff` includes workflow ID, task ID, source agent ID, destination role, task summary, objective, scope, findings, files inspected/changed, commands and outcomes, verification evidence, blockers, risks, confidence, unresolved questions, recommended next action, and references to persisted detailed evidence.

Every field and collection has hard limits. Large tool output is referenced rather than copied repeatedly. Hidden chain-of-thought is neither exposed nor stored; handoffs contain concise observed summaries and execution evidence.

## Domain model

Recommended typed Rust concepts include `OrchestrationMode`, `WorkflowRun`, `WorkflowId`, `TaskId`, `AgentId`, `WorkflowStage`, `RunLifecycleState`, `WaitReason`, `CancellationState`, `AgentRole`, `AgentProfile`, `AgentAssignment`, `ModelRoute`, `EffortRoute`, `UsageBudgetMultiplier`, `WorkflowBudget`, `AdaptiveBudgetPolicy`, `Recommendation`, `Forecast`, `ForecastConfidence`, `DataQuality`, `StructuredHandoff`, `VerificationRequirement`, `VerificationState`, `VerificationEvidence`, `PermissionEnvelope`, `WorkClaim`, and `WorkflowEvent`.

Lifecycle state, workflow stage, wait reason, cancellation state, and verification state must remain separate dimensions, not one oversized enum. Do not put all orchestration logic in one file. Follow repository module-size and crate-boundary instructions, resist adding everything to `codex-core`, and consider a dedicated crate only after O0 determines dependency direction.

## Permission model

Effective permissions are the intersection of user permissions, session permissions, workflow permissions, parent-agent permissions, role ceiling, task request, and runtime sandbox restrictions:

```text
effective permissions = user ∩ session ∩ workflow ∩ parent ∩ role ceiling
                       ∩ task request ∩ runtime restrictions
```

A child cannot gain a permission unavailable to its parent, and a role cannot gain one unavailable to the workflow. Runtime capabilities, not prompt instructions alone, must enforce read-only behavior where possible.

## Verification

Verification requires observed evidence: test, build, lint, formatter, status, diff, file existence, structured tool output, command exit status, snapshots, runtime behavior, or explicitly marked manual verification. States may be `NotRequired`, `Pending`, `Running`, `Passed`, `Failed`, `Blocked`, and `Inconclusive`. A claim of success without evidence cannot produce `Passed`. Failed verification may request one bounded repair pass initially.

## Event model, persistence, and recovery

Use append-only orchestration events correlated by workflow, task, agent, Codex thread and turn IDs, sequence number, timestamp, causation ID, correlation ID, stage, lifecycle transition, usage attribution, and verification evidence. The TUI consumes projections derived from events; there must not be multiple unrelated sources of truth.

Persist enough information for resume, interrupted-run detection, completed-stage and cancellation recovery, budget reconstruction, role reconstruction, verification reconstruction, and handoff recovery. Reference Codex thread and turn identifiers rather than duplicating complete Codex session records or persistence.

## Model and effort routing

Routing may consider task complexity, role, expected context, historical performance, budget and quota, latency, model/effort availability, user preference, and provider restrictions. Decisions are explicit and observable. Material fallbacks are logged and shown with their reason; model switches must not be silent when they change cost, quality, context, or behavior.

## Performance principles

Optimize useful completion time, not agent count. Default to one agent; avoid a model call solely to decide whether another is needed unless evidence shows value; reuse deterministic local analysis; cache bounded repository summaries; preserve incremental history and provider cache stability; use structured handoffs; limit tool-output injection and active-memory event retention; derive TUI projections incrementally; avoid per-frame filesystem/Git/network/quota polling and repeated scans; stop optional work near budget limits; and benchmark Single Mode against every orchestrated mode.

## TUI requirements

Orchestration TUI surfaces follow root and nested `AGENTS.md`, `codex-rs/tui/styles.md`, `PublicBrand::Syndrid` branding gates, and normal Codex compatibility. The visual system is a dark neutral or matte-black canvas, warm firefly-gold for active selection/focus/active values/mascot glow, neutral borders and secondary text, compact top-oriented density, aligned dividers, responsive widths, Unicode display-width correctness, and stacked/scrollable narrow layouts. There must be no focus stealing, unrelated process launch, oversized decoration, stock Codex appearance on Syndrid-specific surfaces, or unintended normal Codex rendering changes. Unavailable values render as `—`; Exact, Derived, Estimated, and Unavailable labels remain visible.

Potential surfaces include mode and multiplier selectors, Manual editor, Recommended plan, Automatic and Adaptive policy views, workflow and per-agent progress, role/model/effort routing, budget and actual/predicted usage, speedup, confidence, wait reason, cancellation, and verification evidence. Every user-visible change requires appropriate `insta` snapshot coverage under repository instructions.

## Git and GitHub CLI boundary

Use local Git for status, diff, staging, commits, branches, worktrees, and local history. Use an installed `gh` executable for authentication status, pull requests, checks, workflow runs, GitHub API operations, and supported issue operations. Detect `gh`, invoke it through bounded subprocess commands, prefer structured JSON, preserve output and exit status, and require confirmation for mutations. Do not embed its Go runtime or copy its internals by default.

Explicit approval is required before staging unrelated files, committing, pushing, opening or merging a PR, deleting branches, force-pushing, or changing remotes. Never force-push automatically.

## Tentative module map

This proposal must be validated by O0 and is not a final placement decision. Do not create these modules during this documentation task.

```text
codex-rs/orchestration/
  Cargo.toml
  src/{lib.rs,types.rs,modes.rs,workflow.rs,scheduler.rs,budget.rs,adaptive.rs,
       routing.rs,recommendation.rs,forecast.rs,history.rs,handoff.rs,
       permissions.rs,verification.rs,events.rs,projections.rs,recovery.rs}

codex-rs/tui/src/syndrid_orchestration/
  {mod.rs,mode_view.rs,budget_view.rs,recommendation_view.rs,workflow_view.rs,
   agent_view.rs,adaptive_view.rs}
```

Do not assume `codex-core` is correct; O0 determines dependency boundaries first.

## Implementation roadmap

Each phase must be independently reviewable, have acceptance criteria, tests, and rollback boundaries, respect change-size guidance, and avoid broad refactors.

- **O0 — Local Codex integration audit:** inspect exact thread, turn, event, token, cancellation, permission, model, effort, persistence, and child-agent seams; crate boundaries; app-server implications; compatibility constraints. No behavior before review.
- **O1 — Typed domain model:** core types, required serialization, exhaustive states, focused tests, no behavior change.
- **O2 — Event log and read-only projections:** append-only events and workflow/task/agent/usage/verification projections; no execution.
- **O3 — Single and Manual configuration:** preserve Single and store Manual configuration; no automatic recommendation.
- **O4 — Sequential Planner → Executor → Verifier:** one workflow, initially concurrency 1, one writer, bounded failure handling.
- **O5 — Structured handoffs:** bounded schema and referenced evidence; no transcript duplication.
- **O6 — Workflow budget enforcement:** baseline, multiplier, ceiling, actual usage, optional-work stop.
- **O7 — Recommended Mode:** proposal, confirmation, editable plan, confidence and data-quality labels.
- **O8 — Automatic Mode:** conservative role/routing choices, concurrency 2, one writer.
- **O9 — Historical forecasting:** classification, similar-session retrieval, overhead measurement, confidence calibration.
- **O10 — Adaptive Efficiency:** reset-aware allocation, reserve, burn comparison, dynamic policy, estimated-data safeguards.
- **O11 — Limited parallel read-only exploration:** concurrency 2, read-only tasks only, no parallel writers.
- **O12 — GitHub CLI integration:** `gh` detection, auth, PRs, checks, workflows, approvals.
- **O13 — Persistence and recovery:** interrupted workflows, resume, budget and verification reconstruction, cancellation recovery.
- **O14 — Benchmarks and optimization:** Single overhead, orchestration tokens, completion time, memory, rendering, event projection, duplicate context, failure and retry cost.

