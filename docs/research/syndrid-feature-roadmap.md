# SyndridCLI Feature Roadmap

**Planning date:** 2026-07-14
**Scope:** Rust-native evolution over the existing Codex-compatible execution, authentication, protocol, storage, model-routing, sandbox, and approval foundations.

## Roadmap principles

1. Preserve Codex execution quality, sandboxing, authentication, protocol, storage, and model compatibility.
2. Add product and policy layers through existing Rust seams; do not replace the application with TypeScript or Python.
3. Treat approval UX as a layer over—not a substitute for—the sandbox and immutable policy boundaries.
4. Prefer local presentation/state changes before new serialized protocol or storage contracts.
5. Keep every external inspiration subject to `docs/research/provenance-policy.md`.
6. Ship inspectability before autonomy: users should see agents, tasks, policies, models, memory, and budgets before Syndrid automates more of them.
7. Use experimental/internal types until behavior stabilizes; version any later app-server or storage surface deliberately.

## Complexity legend

- **S:** Focused change in an existing component; low architectural risk.
- **M:** Multiple related modules or nontrivial UX/state work.
- **L:** Cross-component feature requiring new domain types and extensive tests.
- **XL:** New durable subsystem, security boundary, public compatibility surface, or distributed execution model.

## Recommended first Phase 3 implementation pass

### Phase 3A Milestone 1 — Syndrid status foundation

Build a **presentation-only TUI status foundation** consisting of:

- a Syndrid-owned compact header/status strip;
- current model and reasoning-effort display;
- active collaboration/agent role or profile label;
- separate sandbox and approval-policy indicators;
- context-window/token usage indicator when already available;
- active task/subagent count and a key to open an initially read-only activity overlay;
- compact and expanded rendering modes that preserve narrow-terminal behavior.

This is the best first milestone because it can use existing TUI, config, model catalog, thread settings, permission profile, session, and multi-agent activity state without altering:

- app-server protocol compatibility;
- authentication or credential storage;
- rollout, history, or SQLite compatibility;
- model-provider selection or request routing;
- model/reasoning wire values;
- sandbox enforcement or approval decisions;
- Codex-mode behavior and snapshots.

The first pass should remain a local TUI composition/state change. If a datum is not already available locally, omit it rather than adding a protocol field.

- **User benefit:** Immediate visibility into active model, effort, role, safety posture, context, and work activity.
- **Source inspiration:** Hermes task/status observability and OpenCode agent/model visibility, reimplemented with an original Syndrid layout and terminology.
- **Implementation approach:** Build a local read-only TUI view model from existing `PublicBrand`, thread settings, model catalog, resolved permissions, sandbox state, context metrics, and subagent activity.
- **Affected components:** `codex-rs/tui` and existing branding helpers; read-only use of current `core` and protocol types.
- **Dependencies:** Existing TUI event/state flow and Phase 1 runtime branding.
- **Complexity:** **M**
- **Prerequisites:** Snapshot current Codex and Syndrid TUI states; define minimum terminal width and unavailable-data behavior.
- **Acceptance criteria:** Both `codex` and `syndrid` retain existing behavior; Syndrid gains the status foundation; displayed state is accurate; no config schema, serialized type, auth path, storage format, provider route, or sandbox/approval behavior changes.
- **Explicit non-goals:** Interactive profile/model changes, new protocol fields, new persistence, task mutation, or safety-policy changes.

---

## Phase 3A — Syndrid visual identity and TUI foundation

### 3A.1 Syndrid header and status strip

- **User benefit:** Users can immediately see what model, effort, role, and safety posture are active.
- **Source inspiration:** Hermes TUI observability; OpenCode status-oriented agent/model UX; independently designed Syndrid presentation.
- **Implementation approach:** Compose existing `ChatWidget`, thread settings, model catalog, resolved permission profile, sandbox state, and brand state into a Syndrid-only view model and Ratatui component.
- **Affected components:** `codex-rs/tui`, `codex-rs/utils/cli` branding utilities; read-only use of `core`/protocol types.
- **Dependencies:** Existing runtime `PublicBrand`, model and permission state.
- **Complexity:** **M**
- **Prerequisites:** Confirm narrow-terminal and alternate-screen behavior; characterize current Codex snapshots.
- **Acceptance criteria:** Model, effort, role/profile, sandbox, and approval mode render accurately; unknown data renders as unavailable rather than guessed; Codex snapshots remain unchanged.
- **Explicit non-goals:** No model switching, protocol additions, storage changes, or safety-policy changes.

### 3A.2 Compact and expanded activity views

- **User benefit:** Small terminals remain readable while larger terminals expose richer details.
- **Source inspiration:** Hermes task overlays and OpenCode's dense TUI; independently designed layout.
- **Implementation approach:** Add a responsive layout state and a read-only overlay using existing history/tool/subagent events.
- **Affected components:** `codex-rs/tui` rendering, keymap, multi-agent/history UI.
- **Dependencies:** 3A.1 view model.
- **Complexity:** **M**
- **Prerequisites:** Terminal-width snapshot matrix.
- **Acceptance criteria:** No clipping at supported minimum width; users can open/close overlay without interrupting turns; streaming remains smooth.
- **Explicit non-goals:** No task mutation or new event types in the first pass.

### 3A.3 Collapsible tool-call and reasoning sections

- **User benefit:** Users can focus on outcomes while retaining access to details.
- **Source inspiration:** Common terminal-agent interaction pattern; OpenCode/Hermes public UX.
- **Implementation approach:** Add local presentation state keyed to existing history cells/items; default based on event state, not modified protocol data.
- **Affected components:** `codex-rs/tui` history cells/transcript rendering.
- **Dependencies:** Stable item identity already present in the transcript model.
- **Complexity:** **M**
- **Prerequisites:** Accessibility and keyboard interaction design.
- **Acceptance criteria:** Active calls remain visibly active; failed/approval-required calls cannot be hidden without an indicator; expansion state does not alter persisted history.
- **Explicit non-goals:** No content deletion, redaction semantics, or protocol mutation.

### 3A.4 Session timeline and activity indicators

- **User benefit:** Users can understand turn boundaries, compaction, forks, tool phases, and subagent activity.
- **Source inspiration:** Open Multi-Agent tracing/replay; Hermes session UX; existing Syndrid rollout state.
- **Implementation approach:** Derive a read-only timeline from existing transcript, rollout metadata, and app events.
- **Affected components:** `codex-rs/tui`, session resume/history modules.
- **Dependencies:** Existing event ordering and session metadata.
- **Complexity:** **M**
- **Prerequisites:** Define which internal events are stable enough to display.
- **Acceptance criteria:** Timeline preserves actual order; interruptions and resumed/forked sessions are labeled; no history schema changes.
- **Explicit non-goals:** Search, export, or task replay in Phase 3A.

### 3A.5 Command and slash-command discoverability

- **User benefit:** Features become learnable without memorizing commands.
- **Source inspiration:** Hermes/OpenCode command discovery.
- **Implementation approach:** Improve the existing completion/popup surface using current command metadata and fuzzy matching.
- **Affected components:** `codex-rs/tui`, command registry, fuzzy-match utility.
- **Dependencies:** Existing slash-command definitions.
- **Complexity:** **S/M**
- **Prerequisites:** Distinguish compatibility commands from Syndrid-only presentation commands.
- **Acceptance criteria:** Completion is keyboard accessible, filters incrementally, and does not change command semantics.
- **Explicit non-goals:** Third-party command installation.

---

## Phase 3B — Model, effort, and agent-profile controls

### 3B.1 Unified model and reasoning selector

- **User benefit:** Model and reasoning choices are visible and understandable in one flow.
- **Source inspiration:** OpenCode provider/model UX; Hermes provider profiles; existing Codex model popup.
- **Implementation approach:** Refine existing model popups to show supported effort presets, plan-mode effort, capability badges, and source profile while continuing to emit existing update events.
- **Affected components:** `codex-rs/tui/src/chatwidget/model_popups.rs`, reasoning shortcuts/settings, config persistence, model catalog.
- **Dependencies:** Existing model metadata and effort presets.
- **Complexity:** **M**
- **Prerequisites:** Capability display vocabulary that does not promise unsupported behavior.
- **Acceptance criteria:** Only supported effort values are selectable; current config/profile source is visible; resulting requests retain existing provider and wire flow.
- **Explicit non-goals:** New model IDs, provider routing, auth flows, or effort wire values.

### 3B.2 Named agent profiles

- **User benefit:** Users can switch between planning, implementation, review, exploration, and custom working styles predictably.
- **Source inspiration:** OpenCode named agents; Hermes profiles/delegation; Open Multi-Agent roles.
- **Implementation approach:** Define a Syndrid profile as a validated overlay on existing model, effort, instructions, tools, collaboration mode, and permission profile references. Resolve through existing config layering.
- **Affected components:** `codex-rs/core/config`, `protocol` only if an existing internal type cannot represent selection, `tui`, config persistence.
- **Dependencies:** 3B.1; permission-profile design from Phase 4 may initially be reference-only.
- **Complexity:** **L**
- **Prerequisites:** Decide profile precedence and managed-config constraints; reserve stable identifiers.
- **Acceptance criteria:** Named profiles switch without mutating unrelated config; invalid model/tool/policy references fail clearly; profiles cannot broaden managed safety constraints.
- **Explicit non-goals:** Separate credentials, storage roots, or provider implementations per profile.

### 3B.3 Per-agent model, effort, instruction, and tool overlays

- **User benefit:** Coordinators, workers, and reviewers can use fit-for-purpose settings.
- **Source inspiration:** Open Multi-Agent per-agent routing; Hermes delegated-agent controls.
- **Implementation approach:** Resolve child-agent settings through existing agent roles/config and model provider/catalog; add explicit inheritance diagnostics.
- **Affected components:** `codex-rs/core` agent configuration and spawn handlers, model manager, tool registry, TUI.
- **Dependencies:** 3B.2; existing agent registry.
- **Complexity:** **L**
- **Prerequisites:** Define inheritance and narrowing rules; verify legacy versus multi-agent-v2 behavior.
- **Acceptance criteria:** Spawned agents report resolved settings; unsupported model/effort combinations fail before work starts; child tools never exceed granted capability.
- **Explicit non-goals:** Per-agent provider credentials or sandbox bypass.

### 3B.4 Profile switcher and session pinning

- **User benefit:** Users can switch defaults while retaining a session's explicit settings.
- **Source inspiration:** OpenCode agent switching; Hermes profile UX.
- **Implementation approach:** Add a profile picker and distinguish session-pinned settings from global/profile defaults.
- **Affected components:** `codex-rs/tui`, config update/persistence, thread settings.
- **Dependencies:** 3B.2.
- **Complexity:** **M**
- **Prerequisites:** Clear precedence and reset behavior.
- **Acceptance criteria:** Existing sessions retain explicit settings; new sessions use selected defaults; UI shows effective source.
- **Explicit non-goals:** Storage migration or profile-isolated session databases.

---

## Phase 4 — Permission profiles and agent roles

### 4.1 Permission-profile inspector

- **User benefit:** Users can understand what the sandbox allows, what approvals may be requested, and which policy source controls the decision.
- **Source inspiration:** OpenCode ordered policy UX; Open Multi-Agent default-deny presentation; existing Codex resolved permission profiles.
- **Implementation approach:** Render the resolved profile, sandbox mode, network mode, protected paths, approval policy, managed constraints, and session grants separately.
- **Affected components:** `codex-rs/tui`, `core/config/permissions`, resolved profile types, sandbox summary utilities.
- **Dependencies:** Phase 3A status foundation.
- **Complexity:** **M**
- **Prerequisites:** Security review of wording; avoid exposing secrets or unstable internals.
- **Acceptance criteria:** UI never conflates approval with containment; immutable/managed limits are identifiable; unknown forward-compatible rules remain conservative.
- **Explicit non-goals:** New permission semantics.

### 4.2 Agent-specific permission overlays

- **User benefit:** Planning/review agents can be constrained to read-only behavior while implementation agents receive only required write/tool capabilities.
- **Source inspiration:** OpenCode per-agent rules; Hermes toolset restriction; Open Multi-Agent tool grants.
- **Implementation approach:** Add overlays that may narrow access or request an existing approval path, but cannot broaden the resolved session/managed profile or bypass sandbox enforcement.
- **Affected components:** `core` agent spawn/resolution, tool executor, permission protocol, TUI.
- **Dependencies:** 3B profiles; 4.1 inspector.
- **Complexity:** **L**
- **Prerequisites:** Formal monotonicity rules and security tests.
- **Acceptance criteria:** Effective child permissions are an intersection/narrowing of parent and managed limits; every escalation uses existing approval machinery; denied-read restrictions are preserved.
- **Explicit non-goals:** Independent child sandbox implementation or silent temporary grants.

### 4.3 Temporary, session, and project grant UX

- **User benefit:** Users can make scoped choices without repeatedly answering identical prompts or accidentally granting permanent access.
- **Source inspiration:** Common allow/ask/deny UX; existing Syndrid approval cache.
- **Implementation approach:** Present scope and expiry explicitly while mapping only to supported approval-store/config mechanisms.
- **Affected components:** `tui` approval dialogs, `core/tools/sandboxing`, config update paths, audit events.
- **Dependencies:** 4.1.
- **Complexity:** **M/L**
- **Prerequisites:** Define which project grants are safe and how managed policy constrains them.
- **Acceptance criteria:** Scope is visible before confirmation; grants are revocable; audit record identifies source and expiry; no unsupported persistence is implied.
- **Explicit non-goals:** Global unrestricted grants or automated approval.

### 4.4 Standard coordinator, worker, reviewer, planner, and explorer roles

- **User benefit:** Multi-agent work has predictable responsibilities and review boundaries.
- **Source inspiration:** Open Multi-Agent roles; Hermes delegation; existing Syndrid role config.
- **Implementation approach:** Ship Rust-native profile templates referencing existing model/tool/permission settings, with read-only planner/reviewer variants.
- **Affected components:** config/profile templates, `core` roles, TUI profile picker.
- **Dependencies:** 3B.2 and 4.2.
- **Complexity:** **M**
- **Prerequisites:** Provenance review for all text; write new Syndrid instructions.
- **Acceptance criteria:** Roles are inspectable/editable; each has explicit defaults and limits; read-only roles cannot mutate workspace state.
- **Explicit non-goals:** Hidden prompts copied from external agents or provider-specific role behavior.

### 4.5 Permission and tool audit trail

- **User benefit:** Users can answer who requested, approved, denied, or used a capability.
- **Source inspiration:** Open Multi-Agent tracing; Hermes security visibility.
- **Implementation approach:** Add structured internal audit events and a TUI view; persist only after privacy/storage design approval.
- **Affected components:** `protocol` internal events, `core` tool/approval path, TUI; optionally rollout/state.
- **Dependencies:** 4.2/4.3.
- **Complexity:** **L**
- **Prerequisites:** Redaction, retention, and schema-version decision.
- **Acceptance criteria:** Every grant/denial/use links to agent/task/tool and policy source; secrets and raw sensitive inputs are excluded by default.
- **Explicit non-goals:** Security telemetry export or permanent storage in the first iteration.

---

## Phase 5 — Multi-agent task orchestration

### 5.1 First-class task model and inspectable DAG

- **User benefit:** Users can see what will happen, dependencies, ownership, and completion state before and during execution.
- **Source inspiration:** Open Multi-Agent generated/explicit DAGs; deterministic graph literature.
- **Implementation approach:** Add internal Rust domain types for task, dependency, role, artifact reference, state, and budget. Keep internal/experimental until semantics stabilize.
- **Affected components:** `codex-rs/core`, agent graph store, TUI, optionally app-server experimental API.
- **Dependencies:** Phase 3A task overlay and Phase 4 roles.
- **Complexity:** **L**
- **Prerequisites:** Select multi-agent-v2 foundation; define cycle validation and stable task identity.
- **Acceptance criteria:** DAGs reject cycles/missing dependencies; users can inspect generated plans; no task executes before required approval/profile resolution.
- **Explicit non-goals:** Cross-machine execution or durable public protocol in the first release.

### 5.2 Goal decomposition with plan approval

- **User benefit:** A high-level objective becomes an editable plan instead of opaque autonomous spawning.
- **Source inspiration:** Open Multi-Agent goal decomposition and plan preview.
- **Implementation approach:** A coordinator proposes structured tasks; deterministic validation checks the graph; the user can approve, edit, or reject before execution.
- **Affected components:** `core` coordinator, model client, TUI plan editor, task model.
- **Dependencies:** 5.1.
- **Complexity:** **L**
- **Prerequisites:** Structured-output support and failure fallback.
- **Acceptance criteria:** Generated plans are never trusted without validation; short/simple tasks may bypass decomposition; rejected plans execute nothing.
- **Explicit non-goals:** Claiming generated plans are deterministic or always superior.

### 5.3 Dependency scheduler and bounded parallel execution

- **User benefit:** Independent tasks run concurrently while respecting limits and dependencies.
- **Source inspiration:** Open Multi-Agent scheduler; existing Syndrid agent registry reservations.
- **Implementation approach:** Schedule ready tasks through existing atomic agent-slot reservation, depth/runtime constraints, cancellation, and parent/child lifecycle.
- **Affected components:** `core` agent registry, task scheduler, multi-agent handlers, TUI.
- **Dependencies:** 5.1; validated concurrency semantics for the selected multi-agent version.
- **Complexity:** **L/XL**
- **Prerequisites:** Race, cancellation, and teardown test plan.
- **Acceptance criteria:** Limits fail closed; cancellation propagates; blocked tasks never start; slot accounting recovers after errors.
- **Explicit non-goals:** Unlimited nesting or parallelism.

### 5.4 Worker/reviewer pipeline and result synthesis

- **User benefit:** Work can receive independent review before a final result is presented.
- **Source inspiration:** Open Multi-Agent consensus/verification; common coordinator-worker-reviewer pattern.
- **Implementation approach:** Define explicit artifact/result contracts; reviewers receive only required context and return structured findings; coordinator synthesizes with provenance links.
- **Affected components:** `core` orchestration, context management, TUI, task artifacts.
- **Dependencies:** 5.1–5.3 and Phase 4 roles.
- **Complexity:** **L**
- **Prerequisites:** Context-budget and artifact-size policies.
- **Acceptance criteria:** Reviews identify source task/artifacts; synthesis preserves dissent/failures; reviewers cannot silently mutate worker output.
- **Explicit non-goals:** Majority voting as a correctness guarantee.

### 5.5 Artifact passing and task provenance

- **User benefit:** Agents exchange explicit, inspectable results rather than hidden conversation state.
- **Source inspiration:** Open Multi-Agent artifacts/checkpoints; Hermes fresh child contexts.
- **Implementation approach:** Introduce typed references to existing session files/patches/results with ownership, content type, checksum, and visibility.
- **Affected components:** `core`, state/thread store, TUI, possibly app-server experimental types.
- **Dependencies:** 5.1 and storage design review.
- **Complexity:** **L**
- **Prerequisites:** Privacy, retention, and compatibility policy.
- **Acceptance criteria:** Artifact lineage is visible; large/sensitive content is not duplicated unnecessarily; no arbitrary path access is implied.
- **Explicit non-goals:** A new incompatible general-purpose object store.

### 5.6 Orchestration budgets, retries, and cancellation

- **User benefit:** Users control concurrency, steps, context, tokens, optional cost, retry count, and elapsed time.
- **Source inspiration:** Open Multi-Agent budgets; Hermes delegation limits.
- **Implementation approach:** Central budget authority checked before spawn/model/tool/retry operations; use provider-reported cost only when reliable.
- **Affected components:** `core` orchestration/model client, agent registry, TUI.
- **Dependencies:** 5.1–5.3.
- **Complexity:** **L**
- **Prerequisites:** Define accounting units and unknown-cost behavior.
- **Acceptance criteria:** Exhausted budgets stop new work predictably; users can cancel any subtree; retries distinguish idempotent from side-effecting tasks.
- **Explicit non-goals:** Perfect cross-provider cost equivalence.

### 5.7 Live orchestration visualization and replay

- **User benefit:** Users can follow the graph, inspect failures, and understand prior runs.
- **Source inspiration:** Open Multi-Agent tracing/replay; Hermes subagent UI.
- **Implementation approach:** Extend Phase 3A overlay with DAG state and event history; add durable replay only after a versioned storage design.
- **Affected components:** TUI, task events, state/rollout, app-server later.
- **Dependencies:** 5.1–5.6.
- **Complexity:** Visualization **M/L**; durable replay **XL**.
- **Prerequisites:** Stable event model and redaction.
- **Acceptance criteria:** Live state matches scheduler truth; replay cannot re-execute side effects; persisted formats are versioned.
- **Explicit non-goals:** Video-style terminal recording or automatic rerun.

---

## Phase 6 — Skills and memory

### 6.1 Unified skill discovery and metadata

- **User benefit:** Users can see available local, project, and global skills, their origin, permissions, and compatibility.
- **Source inspiration:** Hermes progressive disclosure; OpenCode/agent-compatible `SKILL.md` discovery; existing Syndrid skill crates.
- **Implementation approach:** Normalize existing skill discovery into a metadata index with source path, scope, version, license, checksum, required tools, and trust state.
- **Affected components:** `codex-rs/skills`, `core-skills`, `tui`, config.
- **Dependencies:** Provenance policy and permission-profile UI.
- **Complexity:** **L**
- **Prerequisites:** Inventory current formats and precedence.
- **Acceptance criteria:** Duplicate/conflicting skills are explained; metadata loads without executing content; untrusted skills are visibly marked.
- **Explicit non-goals:** Automatic network installation.

### 6.2 Review-before-install skill workflow

- **User benefit:** Users can inspect files, licenses, scripts, permissions, and source before installation.
- **Source inspiration:** Hermes skill bundles, with stricter Syndrid trust controls.
- **Implementation approach:** Stage downloads outside active skill paths, pin source revision/checksum, enumerate files/licenses/scripts/assets, require approval, then install atomically.
- **Affected components:** skills, file-system, config, TUI, sandbox/tool policy.
- **Dependencies:** 6.1.
- **Complexity:** **L/XL**
- **Prerequisites:** Network/download policy, signature strategy, license scanner, rollback.
- **Acceptance criteria:** Nothing executes during review; install is pinned and atomic; provenance manifest is stored; removal is complete.
- **Explicit non-goals:** Trusting a registry listing or repository top-level license alone.

### 6.3 Skill permissions and version compatibility

- **User benefit:** Skills declare required capabilities and fail safely when incompatible.
- **Source inspiration:** OpenCode skill permissions; extension manifest conventions.
- **Implementation approach:** Add a Syndrid manifest adjunct that can wrap compatible open skill formats without modifying their content.
- **Affected components:** skills, config, permission resolver, TUI.
- **Dependencies:** 4.2 and 6.1.
- **Complexity:** **M/L**
- **Prerequisites:** Versioning and capability vocabulary.
- **Acceptance criteria:** Skills cannot self-grant; required capabilities are reviewed; incompatible versions are rejected clearly.
- **Explicit non-goals:** A universal cross-agent executable standard.

### 6.4 Transparent user and project memory

- **User benefit:** Users can explicitly save, inspect, edit, forget, and scope durable knowledge.
- **Source inspiration:** Hermes separation of user memory, project memory, and searchable history; existing Syndrid memory crates.
- **Implementation approach:** Build a UI and policy layer over existing memory abstractions with source, timestamp, scope, visibility, confidence, and last-used metadata.
- **Affected components:** memory crates, `core` context, state, TUI.
- **Dependencies:** Confirm current memory storage/schema capabilities.
- **Complexity:** **L**
- **Prerequisites:** Privacy and injection threat model.
- **Acceptance criteria:** No hidden durable write; every memory item shows provenance and scope; deletion is verifiable; sensitive data warnings exist.
- **Explicit non-goals:** Automatic indefinite retention or cross-project sharing by default.

### 6.5 Stale-memory review and injection defense

- **User benefit:** Old or suspicious memories do not silently steer future sessions.
- **Source inspiration:** Hermes memory scanning and bounded memory; independent security design.
- **Implementation approach:** Periodic/user-invoked review by age, conflict, source, and risk markers; treat memory as untrusted context and label it in prompts.
- **Affected components:** memory, core context assembly, TUI.
- **Dependencies:** 6.4.
- **Complexity:** **M/L**
- **Prerequisites:** False-positive and user-control policy.
- **Acceptance criteria:** Users can archive/delete/confirm entries; suspicious entries are not silently injected; stale review is explainable.
- **Explicit non-goals:** Claiming automated scanning eliminates prompt injection.

### 6.6 Searchable session history and import/export

- **User benefit:** Users can find prior work and move selected sessions safely.
- **Source inspiration:** Hermes SQLite/FTS search; OpenCode session export/import.
- **Implementation approach:** Extend existing state/thread-store indexes; export a versioned, sanitized Syndrid envelope referencing unchanged underlying session data when possible.
- **Affected components:** state/rollout/thread store, TUI, CLI.
- **Dependencies:** Storage and privacy review.
- **Complexity:** **L**
- **Prerequisites:** Redaction, encryption, and compatibility rules.
- **Acceptance criteria:** Search respects project/privacy scope; exports identify omitted/redacted data; imports never overwrite existing sessions silently.
- **Explicit non-goals:** Public sharing service.

---

## Phase 7 — Providers, remote execution, and extensions

### 7.1 Provider profiles and capability inspector

- **User benefit:** Users can understand endpoint, auth mode, available models, context, tools, image support, reasoning support, and limitations.
- **Source inspiration:** OpenCode and Hermes provider UX; existing Syndrid provider/model crates.
- **Implementation approach:** Add presentation and configuration profiles over `create_model_provider`, model catalog, auth manager, and capability metadata.
- **Affected components:** model-provider, model-provider-info, models-manager, config, TUI/app-server.
- **Dependencies:** Phase 3B controls.
- **Complexity:** **L**
- **Prerequisites:** Capability schema and secret-handling review.
- **Acceptance criteria:** Profiles never expose secrets; unsupported features are disabled; routing remains in existing provider factory.
- **Explicit non-goals:** Rewriting auth or supporting every external provider immediately.

### 7.2 OpenAI-compatible and local-provider hardening

- **User benefit:** Reliable use of local/self-hosted endpoints with clear capability fallbacks.
- **Source inspiration:** Hermes/OpenCode provider breadth; existing Syndrid Ollama/LM Studio/provider support.
- **Implementation approach:** Improve capability detection, model discovery, diagnostics, and compatibility tests behind existing provider interfaces.
- **Affected components:** provider crates, models manager, CLI doctor, TUI.
- **Dependencies:** 7.1.
- **Complexity:** **L**
- **Prerequisites:** Test fixtures for streaming/tool/reasoning variants.
- **Acceptance criteria:** Unknown capabilities fail conservatively; model discovery is cacheable/reloadable; no first-party routing regression.
- **Explicit non-goals:** Provider-specific types leaking into orchestration.

### 7.3 Execution environment profiles

- **User benefit:** Users can select local sandbox, container, SSH, or future remote workers with explicit trust and capability differences.
- **Source inspiration:** Hermes execution environments.
- **Implementation approach:** Define an execution adapter trait that delegates local work to existing Codex sandbox helpers and treats remote/container adapters as additional boundaries with independent policy checks.
- **Affected components:** sandboxing, exec server/protocol, CLI/config, app-server/daemon, future adapters.
- **Dependencies:** Security architecture review.
- **Complexity:** **XL**
- **Prerequisites:** Threat model, credential forwarding rules, filesystem mapping, cancellation, platform test matrix.
- **Acceptance criteria:** Local profiles preserve existing sandbox; remote profiles disclose isolation guarantees; no profile silently weakens policy; operations are auditable and cancellable.
- **Explicit non-goals:** Treating SSH or containers as automatically secure; simultaneous arbitrary backend selection per tool in the first version.

### 7.4 Remote workers and shared sessions

- **User benefit:** Long-running or resource-specific tasks can execute elsewhere while remaining visible in Syndrid.
- **Source inspiration:** Hermes gateways; OpenCode attached clients; existing Syndrid app-server daemon.
- **Implementation approach:** Extend authenticated app-server/daemon capabilities through versioned experimental APIs, operation locks, resumable event streams, and explicit worker registration.
- **Affected components:** app-server, app-server-protocol, daemon, thread manager/store, TUI.
- **Dependencies:** 7.3 and stable task model.
- **Complexity:** **XL**
- **Prerequisites:** Mutual authentication, authorization, transport encryption, version negotiation, offline recovery.
- **Acceptance criteria:** Workers cannot access unassigned sessions; reconnects do not duplicate tasks; slow/disconnected clients do not corrupt state.
- **Explicit non-goals:** Internet-exposed unauthenticated daemon or peer-to-peer discovery by default.

### 7.5 LSP tool integration

- **User benefit:** Agents can consume structured diagnostics and symbols instead of relying only on text search.
- **Source inspiration:** OpenCode LSP tools.
- **Implementation approach:** Add an optional Rust LSP client/tool layer with explicit server configuration and no automatic download by default.
- **Affected components:** new LSP crate/module, tools registry, config, TUI.
- **Dependencies:** Extension trust and process policy.
- **Complexity:** **L**
- **Prerequisites:** Lifecycle, workspace trust, and server provenance design.
- **Acceptance criteria:** Servers are user-configured/pinned; process access is mediated; diagnostics are bounded and attributable.
- **Explicit non-goals:** Bundling every language server or allowing LSP to bypass sandbox rules.

### 7.6 Versioned extension manifests and isolated hosts

- **User benefit:** Third-party tools/hooks can integrate without receiving unrestricted in-process access.
- **Source inspiration:** Hermes plugin taxonomy and OpenCode extensibility, redesigned for a stronger trust boundary.
- **Implementation approach:** Define a manifest with compatibility version, provenance, requested capabilities, entry point, and content hashes; run untrusted extensions in a subprocess RPC or WASM boundary.
- **Affected components:** plugins/hooks/core-plugins, protocol, tool registry, config, TUI.
- **Dependencies:** Phase 4 permissions and Phase 6 provenance workflow.
- **Complexity:** **XL**
- **Prerequisites:** ABI/RPC versioning, crash isolation, resource limits, signing policy.
- **Acceptance criteria:** Untrusted code cannot run in the main process; capabilities are explicit and revocable; incompatible extensions fail before launch.
- **Explicit non-goals:** JavaScript/Bun plugin parity or automatic activation of project-local extensions.

### 7.7 IDE and app-server profile/task integration

- **User benefit:** IDEs can select profiles and observe tasks using the same core semantics as the TUI.
- **Source inspiration:** OpenCode multi-client architecture; existing Syndrid app-server.
- **Implementation approach:** After internal semantics stabilize, add versioned app-server methods/events with capability negotiation and generated schema tests.
- **Affected components:** app-server-protocol, app-server, SDKs/clients, TUI parity tests.
- **Dependencies:** Stable Phase 3B profiles and Phase 5 task model.
- **Complexity:** **L/XL**
- **Prerequisites:** Backward-compatibility and experimental-to-stable process.
- **Acceptance criteria:** Old clients continue working; new fields are capability-gated; TUI and IDE observe the same effective settings/task truth.
- **Explicit non-goals:** Breaking existing wire names, auth, session IDs, or storage.

---

## Ranked top 15 highest-value features

| Rank | Feature | Phase | Value rationale |
|---:|---|---|---|
| 1 | Syndrid header/status foundation | 3A | Immediate product identity and transparency with minimal compatibility risk |
| 2 | Unified model and reasoning selector | 3B | Exposes an existing high-value capability cleanly |
| 3 | Named agent profiles | 3B | Makes specialized workflows repeatable and understandable |
| 4 | Task/subagent activity overlay | 3A | Turns existing multi-agent internals into a visible product feature |
| 5 | Permission-profile inspector | 4 | Clarifies safety without weakening sandbox boundaries |
| 6 | Agent-specific permission overlays | 4 | Enables safe planner/reviewer/worker specialization |
| 7 | First-class inspectable task DAG | 5 | Core differentiator for transparent multi-agent coordination |
| 8 | Bounded dependency scheduler | 5 | Converts subagents into reliable coordinated execution |
| 9 | Orchestration budgets and cancellation | 5 | Prevents runaway cost, context, and concurrency |
| 10 | Worker/reviewer synthesis pipeline | 5 | Improves quality while preserving inspectability |
| 11 | Transparent user/project memory | 6 | Provides durable value with explicit control and provenance |
| 12 | Provenance-aware skill discovery/install | 6 | Enables ecosystem growth without uncontrolled supply-chain risk |
| 13 | Searchable session history | 6 | Makes existing stored work reusable and discoverable |
| 14 | Provider capability profiles | 7 | Improves flexibility while retaining current routing/auth |
| 15 | Isolated extension host | 7 | Creates a sustainable extensibility boundary instead of in-process trust |

## Dependencies across phases

```text
3A status/activity foundation
  ├─> 3B model/profile controls
  │     └─> 4 agent permission overlays and standard roles
  │            └─> 5 task DAG, scheduler, budgets, synthesis
  ├─> 6 memory/skill visibility
  └─> 7 app-server/IDE parity

4 permission semantics
  ├─> 6 skill installation and permissions
  └─> 7 execution profiles and extension host

5 stable task model
  └─> 7 remote workers/shared sessions
```

## Program-level acceptance criteria

- Codex-mode behavior remains compatible unless an explicitly approved upstream-compatible change is made.
- No phase weakens existing sandbox, deny-read, approval, authentication, credential, protocol, or storage boundaries.
- New profile/agent permissions are monotonic: they can narrow access and request existing escalation, never bypass immutable limits.
- External inspiration has a recorded classification and source register.
- New durable or wire formats are versioned and tested for backward compatibility.
- Every extension, skill, MCP server, remote worker, and execution profile exposes trust and provenance state.
- Rust remains the primary implementation language; isolated integrations use other runtimes only when technically justified and security-reviewed.

## Program-level non-goals

- A broad internal rename of Codex crates, paths, protocol identifiers, auth identifiers, or storage.
- Replacing OpenAI/ChatGPT authentication or first-party model routing.
- Converting SyndridCLI into a TypeScript/Python application.
- Copying external prompts, layouts, names, assets, or implementation.
- Treating provider breadth, autonomy, or plugin count as more important than safety and compatibility.
- Shipping remote execution or extension installation before a documented trust model exists.
