# Syndrid Product Vision

## 1. Vision statement

Syndrid is a fast, transparent, configurable, high-reliability coding-agent CLI and harness for both short edits and long autonomous engineering sessions.

It gives an engineer one coherent operating environment for understanding a repository, directing models and agents, controlling access, managing context and memory, verifying work, and recovering from failure. Syndrid should feel immediate when the task is small and dependable when the task spans hours, many tools, multiple agents, or interrupted sessions.

The product promise is not that an agent will always be right. The promise is that useful work will be performed within understandable boundaries, that consequential actions will be visible and controllable, that completion claims will be backed by evidence, and that failures will leave the repository in a diagnosable and recoverable state.

Syndrid is independently designed around recurring user problems observed across coding-agent products. It preserves an original identity, terminology, interaction model, and implementation. External products inform the problem landscape; they do not define Syndrid's product architecture or visual language.

## 2. Product thesis

Model quality matters, but harness quality is equally important. A capable model inside a weak harness can still edit too early, lose critical context, repeat failed actions, overrun scope, misuse permissions, declare success without verification, or leave a long-running task impossible to resume. A well-designed harness makes model capability usable, inspectable, and reliable.

Syndrid therefore treats the engineering loop—not the model call—as the product. It coordinates:

- **Models:** selection, capability fit, reasoning effort, provider limitations, and fallback behavior.
- **Agents:** roles, task ownership, isolation, dependencies, concurrency, review, and synthesis.
- **Context:** what is loaded, retained, summarized, removed, and handed off.
- **Memory:** explicit durable knowledge with scope, source, confidence, and user control.
- **Tools:** discoverability, capability exposure, execution, output handling, and auditability.
- **Permissions:** sandbox containment, approval policy, scoped grants, and monotonic child-agent restrictions.
- **Usage:** tokens, cost where available, quotas, reset windows, rate limits, and task budgets.
- **Verification:** tests, builds, linting, runtime checks, visual checks, and evidence collection.
- **Recovery:** checkpoints, cancellation, retries, resume, rollback, and honest failure reporting.

The result should be more than a chat interface around a shell. Syndrid is an execution discipline encoded into a terminal-native product: understand before acting, act within scope, verify before claiming completion, and preserve enough structured state to continue safely.

## 3. Problems Syndrid exists to solve

Coding-agent users repeatedly encounter failures that cannot be explained by model intelligence alone. Syndrid exists to reduce the following harness-level problems:

- **Context rot:** long sessions accumulate stale findings, superseded plans, noisy tool output, and contradictory assumptions until the agent's decisions degrade.
- **Weak memory:** useful project knowledge disappears between sessions, while poorly governed memory can become stale, invasive, or silently misleading.
- **Editing before research:** agents modify the first plausible location before understanding repository structure, conventions, existing helpers, or the true source of behavior.
- **Literal interpretation:** the system follows the surface wording of a request while missing the user's practical intent, repository constraints, or expected end state.
- **Duplicate code:** insufficient search leads to new utilities, configuration paths, types, or workflows that already exist.
- **Unverified completion:** an agent reports success after writing code without running the checks needed to demonstrate that the changed behavior works.
- **Excessive diffs:** broad rewrites, opportunistic cleanup, formatting churn, or unrelated changes make review and rollback harder.
- **Weak rollback:** recovery is limited to coarse Git operations or manual reconstruction rather than task-aware checkpoints and reversible steps.
- **Poor agent visibility:** users cannot tell which agents exist, why they were created, what they can access, or whether they are making progress.
- **Permission fatigue:** repetitive prompts train users to approve without reading, while overly broad presets weaken meaningful control.
- **Unclear usage and limits:** token consumption, cost, quotas, rate limits, and reset windows are hidden, delayed, or conflated.
- **Model-versus-harness failure ambiguity:** users cannot distinguish a provider outage, model limitation, tool failure, permission denial, context failure, orchestration bug, or repository problem.
- **Configuration sprawl:** settings are distributed across files, environment variables, profiles, flags, managed policy, and provider-specific mechanisms without clear precedence.
- **Loops without progress:** agents repeat searches, commands, edits, or failed strategies without recognizing that no new evidence is being produced.
- **Terminal and Windows problems:** path handling, shell assumptions, process control, rendering, credentials, notifications, and platform-specific tools fail disproportionately outside idealized Unix environments.
- **Focus stealing:** windows, prompts, notifications, or UI transitions interrupt the user's current work when no immediate action is required.
- **Unreliable long-running sessions:** network failures, process restarts, provider limits, context exhaustion, partial agent failure, and machine interruption can destroy hours of progress.

These problems are connected. Weak inspection causes duplicate code; opaque orchestration makes loops hard to diagnose; poor context management produces literal or contradictory behavior; weak completion contracts hide all of them. Syndrid addresses them as one reliability system rather than as unrelated features.

## 4. Core product principles

Each principle is a product contract. The boundary is as important as the behavior: Syndrid should be explicit about what the harness can guarantee and what still depends on models, providers, repositories, operating systems, and user policy.

### 4.1 Research before modification

- **User problem:** Agents often edit the first plausible file before locating the actual behavior, existing helpers, local conventions, tests, or constraints.
- **Syndrid behavior:** Before modification, Syndrid performs a proportional inspection pass: locate relevant files and symbols, identify repository instructions, search for reusable implementations, inspect nearby tests and conventions, and state the intended change surface. A write-capable agent receives the important findings or a structured handoff.
- **Product boundary:** Research is proportional, not an automatic exhaustive audit. An explicit emergency or mechanical mode may permit immediate edits, but the report must disclose reduced inspection.
- **Example interaction:** “Rename this flag.” Syndrid finds the declaration, parser, help text, tests, and compatibility aliases before changing the smallest coherent set.
- **Measurable success condition:** At least 95% of non-trivial modification tasks record inspected implementation and validation locations before the first write; duplicate-helper introductions attributable to missed repository search trend toward zero.

### 4.2 Intent over literal wording

- **User problem:** A request such as “fix the button” or “make this reliable” is underspecified, and literal execution may satisfy the sentence while failing the actual outcome.
- **Syndrid behavior:** Syndrid infers intent from repository context, existing behavior, issue evidence, and conventions. It resolves ordinary ambiguity with sensible defaults and asks only when competing interpretations materially change product behavior or risk.
- **Product boundary:** Syndrid does not invent business requirements, bypass explicit constraints, or treat inference as authorization for consequential actions.
- **Example interaction:** “Stop the command from hanging.” Syndrid investigates process lifecycle and cancellation rather than merely adding a shorter timeout to one call site.
- **Measurable success condition:** Rework caused by technically literal but outcome-wrong implementations decreases over time; clarification prompts are concentrated on decisions with materially different consequences.

### 4.3 Proportional process

- **User problem:** The same ceremony applied to a typo and a migration makes small work slow, while treating a migration like a typo makes large work unsafe.
- **Syndrid behavior:** Syndrid scales inspection, planning, checkpointing, orchestration, and verification to task size, uncertainty, reversibility, and blast radius.
- **Product boundary:** Proportionality never removes immutable sandbox restrictions or permits unsupported completion claims.
- **Example interaction:** A documentation typo receives a quick direct edit and syntax check; an authentication refactor receives repository mapping, a plan, checkpoints, staged implementation, and independent review.
- **Measurable success condition:** Median time-to-first-useful-action remains low for small tasks while escaped-defect and rollback rates do not rise for large tasks.

### 4.4 Short-session speed

- **User problem:** Coding agents can impose startup cost through repeated indexing, mandatory plans, configuration questions, and verbose narration even when the user needs a two-minute edit.
- **Syndrid behavior:** Syndrid starts quickly, reuses safe local indexes and known project context, keeps default output concise, and allows the direct path when scope and verification are obvious.
- **Product boundary:** Speed does not mean skipping necessary repository instructions, scope controls, or verification.
- **Example interaction:** “Fix this spelling error and check the file.” Syndrid edits the identified file, runs the relevant lightweight check, and reports the result without spawning agents.
- **Measurable success condition:** Small, well-specified tasks reach the first relevant read or edit within seconds on a warm project and avoid unnecessary agent creation.

### 4.5 Long-session durability

- **User problem:** Hours of work can be lost to context exhaustion, provider errors, terminal closure, process crashes, machine restarts, or one failed worker.
- **Syndrid behavior:** Long tasks maintain durable task state, semantic checkpoints, resumable artifacts, structured handoffs, bounded retries, and explicit recovery instructions. Independent worker failure does not erase completed work from other branches of the task graph.
- **Product boundary:** Syndrid cannot guarantee recovery from unrecorded external side effects or destructive actions outside its controlled tools.
- **Example interaction:** A migration pauses after a rate limit. On resume, Syndrid reconstructs the approved plan, completed tasks, changed files, verification state, and next runnable dependency.
- **Measurable success condition:** A high percentage of interrupted autonomous sessions resume without repeating completed work or requiring the user to restate the task.

### 4.6 Actively managed context

- **User problem:** More context is not always better; stale plans, huge logs, duplicated source, and irrelevant schemas consume tokens and distort decisions.
- **Syndrid behavior:** Syndrid budgets context by role and task, retrieves selectively, summarizes before exhaustion, removes stale data, preserves source references, and makes handoffs explicit.
- **Product boundary:** Compaction may lose nuance, so source material remains retrievable and uncertainty introduced by summarization is disclosed when material.
- **Example interaction:** After a long debugging pass, raw logs are retained as an artifact while the active context keeps the failing scenario, ruled-out hypotheses, decisive traces, and next experiment.
- **Measurable success condition:** Context-limit failures and repeated rediscovery decline; compaction preserves all task-critical facts in sampled recovery evaluations.

### 4.7 Transparent usage and cost

- **User problem:** Users cannot make informed decisions when token use, estimated cost, quota state, rate limits, and reset times are hidden or conflated.
- **Syndrid behavior:** Syndrid shows task and session consumption, provider-reported limits, estimated cost where defensible, budget state, and unavailable metrics as unavailable rather than guessed.
- **Product boundary:** Provider data may be delayed, incomplete, differently defined, or absent. Subscription allowance and API billing are never presented as interchangeable.
- **Example interaction:** Before spawning five reviewers, Syndrid shows the expected budget impact and the remaining task cap; if exact currency cost is unavailable, it displays tokens and provider status without fabricating a price.
- **Measurable success condition:** Usage displays reconcile with provider data within documented tolerances, and unknown or estimated values are always visibly classified.

### 4.8 Token efficiency without reducing correctness

- **User problem:** Wasteful context, duplicate searches, oversized tool output, and unnecessary agents increase cost and latency, but aggressive compression can hide evidence and reduce quality.
- **Syndrid behavior:** Syndrid deduplicates retrieval, caps noisy output, shares artifacts by reference, selects the smallest sufficient agent topology, and spends additional tokens when uncertainty or risk justifies them.
- **Product boundary:** Token reduction is not an objective when it would weaken inspection, safety, verification, or the fidelity of an important handoff.
- **Example interaction:** Three workers use a shared repository map and targeted file excerpts rather than each loading the entire tree; an independent reviewer still receives the full diff and relevant tests.
- **Measurable success condition:** Token cost per verified task decreases without lowering verification pass rates, increasing unnecessary diffs, or increasing user corrections.

### 4.9 Visible and bounded multi-agent orchestration

- **User problem:** Hidden fan-out can consume large budgets, duplicate work, race on files, and leave the user unable to understand or stop the system.
- **Syndrid behavior:** Every agent has a visible role, reason, owner task, context package, permissions, budget, status, outputs, and cancellation path. Concurrency, recursion, retries, and total agents are bounded.
- **Product boundary:** More agents are not presumed to produce better results. Syndrid may choose one agent when decomposition offers no clear benefit.
- **Example interaction:** A cross-platform bug creates one Windows investigator and one Unix behavior reviewer, then a synthesizer; the UI explains why parallel work is useful and caps each agent's tools.
- **Measurable success condition:** No agent executes without a visible parent task and budget; duplicate-agent work and runaway fan-out occur below a defined operational threshold.

### 4.10 Easy configuration with clear precedence

- **User problem:** Users cannot predict which setting wins across flags, project files, user files, environment variables, provider profiles, and managed policy.
- **Syndrid behavior:** Syndrid presents one inspectable resolved configuration, identifies each value's source, explains precedence, validates conflicts, supports named profiles, and offers focused overrides.
- **Product boundary:** Syndrid preserves upstream-compatible configuration and managed locks where required; a friendly product layer must not silently rewrite unrelated settings.
- **Example interaction:** `syndrid config explain model` shows the effective model, the profile that selected it, the project override it superseded, and the managed constraints that cannot be changed.
- **Measurable success condition:** Configuration-related support incidents and “why did this value win?” failures decline; every effective value can be traced to a source.

### 4.11 Supported bring-your-own accounts and keys

- **User problem:** Engineers need legitimate flexibility across official subscriptions, API keys, local models, and compatible endpoints without unsafe secret handling or unsupported automation.
- **Syndrid behavior:** Syndrid supports provider mechanisms that are officially available, named credential profiles, secure operating-system storage, capability-aware provider profiles, and explicit billing identity.
- **Product boundary:** Syndrid does not circumvent provider limits, automate unsupported account use, rotate accounts abusively, or imply that subscription authentication grants API billing privileges.
- **Example interaction:** A user selects a local profile for private exploration and an API profile for verified implementation; Syndrid shows which credential and billing mode each task will use without exposing the secret.
- **Measurable success condition:** Secrets do not appear in project configuration or logs; provider selection errors are diagnosed before task execution where capabilities are known.

### 4.12 Structured and inspectable memory

- **User problem:** Invisible memory can inject stale or sensitive assumptions, while no memory forces users to repeat stable project knowledge.
- **Syndrid behavior:** Memory is typed by user, project, and session scope; records include source, confidence, timestamps, review state, and visibility. Users can inspect, correct, save, reject, and forget records.
- **Product boundary:** Memory is bounded and curated, not an unlimited transcript. It is never silently treated as higher authority than current repository evidence or explicit instructions.
- **Example interaction:** Syndrid proposes saving “this repository requires a Windows smoke test” as project memory, shows the source, and waits for explicit save policy before future injection.
- **Measurable success condition:** Every injected memory item is visible and attributable; stale-memory incidents are detected by review or conflict checks before they cause changes.

### 4.13 Evidence-backed completion

- **User problem:** “Done” often means only that files changed, not that the requested behavior works.
- **Syndrid behavior:** Completion is tied to a declared verification plan and evidence. Reports distinguish implementation from tests, build, lint, runtime, and visual validation, including what was skipped.
- **Product boundary:** Passing checks do not prove all behavior, and unavailable checks must not be represented as success.
- **Example interaction:** A TUI rendering fix is reported as implemented but unverified until Syndrid launches the affected view and records the observed result; unit tests alone are not mislabeled as visual verification.
- **Measurable success condition:** The percentage of completion claims with relevant verification evidence approaches 100%, with false “verified” labels treated as product defects.

### 4.14 Scope and diff control

- **User problem:** Agents make unrelated cleanup changes, reformat files, or expand a local request into a broad refactor.
- **Syndrid behavior:** Syndrid establishes an allowed change surface, monitors changed files and diff growth, warns on scope expansion, and requires a reason or approval before crossing material boundaries.
- **Product boundary:** Some correct fixes reveal necessary adjacent changes; Syndrid permits justified expansion rather than enforcing an arbitrary file count.
- **Example interaction:** A parser fix unexpectedly touches generated fixtures. Syndrid pauses to explain why the new files are required and offers to include, isolate, or defer them.
- **Measurable success condition:** Unnecessary changed lines and unrelated-file modifications decrease; scope expansions are attributable to an explicit decision.

### 4.15 Semantic checkpoints and rollback

- **User problem:** A raw commit or filesystem snapshot does not explain which logical step it represents, which checks passed, or what external effects cannot be reversed.
- **Syndrid behavior:** Syndrid creates named checkpoints around meaningful task transitions, records repository state and verification status, and supports precise rollback or branch-from-checkpoint workflows.
- **Product boundary:** Rollback is guaranteed only for captured local state. Network operations, published artifacts, sent messages, database mutations, and other external side effects require separate compensation or confirmation.
- **Example interaction:** Before replacing a storage layer, Syndrid records “pre-migration baseline: tests passing”; after schema conversion it records another checkpoint with migration results.
- **Measurable success condition:** Recovery returns the repository to the selected semantic state without reverting unrelated user work, and every checkpoint states its reversible scope.

### 4.16 Adaptive permissions over the existing Codex sandbox

- **User problem:** Fixed prompting is either exhausting or too broad, and users may confuse an approval with actual operating-system containment.
- **Syndrid behavior:** Syndrid layers understandable, task-scoped permission policy over the existing Codex sandbox. It can reduce prompts through narrow grants and profiles while preserving immutable and managed restrictions. Child-agent permissions can only narrow inherited authority or request an existing escalation path.
- **Product boundary:** The Syndrid permission layer never weakens, replaces, or bypasses the Codex sandbox, provider security, credential boundaries, or managed policy.
- **Example interaction:** A reviewer receives repository read and test-result access but no file writes; an implementer may write only within the approved project and must request separately for network access.
- **Measurable success condition:** Permission prompts per task decrease while denied-boundary escapes remain zero and users can correctly distinguish approval state from sandbox state.

### 4.17 Native Windows quality

- **User problem:** Coding-agent CLIs frequently assume Unix paths, shells, signals, process groups, terminals, package managers, and credential behavior.
- **Syndrid behavior:** Windows is a first-class development and test environment. Path normalization, PowerShell and POSIX-shell distinctions, process cancellation, terminal rendering, file locking, long paths, credential storage, notifications, and native tooling receive explicit product coverage.
- **Product boundary:** Cross-platform sameness is not always possible; Syndrid documents platform-specific behavior rather than hiding it behind fragile emulation.
- **Example interaction:** A task running in Git Bash on Windows uses correct drive-path conversion and cancels a child process tree without leaving background processes or stealing terminal focus.
- **Measurable success condition:** Windows-specific failure rate converges with supported Unix platforms, with platform regressions tracked as release-blocking reliability defects when severe.

### 4.18 Quiet and non-disruptive operation

- **User problem:** Excessive notifications, animations, focus changes, popups, and narration interrupt concentrated engineering work.
- **Syndrid behavior:** Syndrid remains quiet by default, signals only meaningful state transitions, never steals focus for non-urgent events, and lets users choose notification channels and thresholds.
- **Product boundary:** Required approvals, destructive-action confirmations, and critical failures must remain visible even in quiet modes.
- **Example interaction:** A 40-minute test suite completes in the background; Syndrid updates status and sends the configured passive notification without moving cursor focus or opening a window.
- **Measurable success condition:** Unrequested focus-stealing events are zero; notification dismissal and mute rates remain low.

### 4.19 Remote supervision

- **User problem:** Long tasks may need to continue away from the original terminal, but remote control can create unclear trust, credential, and cancellation boundaries.
- **Syndrid behavior:** Authorized remote clients can inspect task state, approve bounded requests, pause, cancel, and review evidence through a consistent protocol. The execution environment, credential location, and active controller are always visible.
- **Product boundary:** Remote supervision is not a generic messaging platform and does not imply unrestricted remote shell access. Sensitive operations retain local policy and explicit authorization.
- **Example interaction:** From a trusted client, a user sees that a migration is blocked on one network permission, reviews the exact request, approves it for that task, and later reads the verification report.
- **Measurable success condition:** Remote actions are fully attributable and revocable; cancellation reaches the active execution environment within a documented latency target.

### 4.20 Clean, terminal-native, expandable UI

- **User problem:** Dense telemetry walls obscure the current objective and next required action, while overly minimal interfaces hide evidence.
- **Syndrid behavior:** The default terminal view emphasizes objective, current state, active work, blockers, and required user action. Details expand in place: task graph, agents, logs, usage, permissions, artifacts, and verification remain one action away.
- **Product boundary:** Syndrid does not force every capability into the primary screen or imitate a desktop dashboard inside the terminal.
- **Example interaction:** The compact view shows “implementing 2/5, one worker blocked, no user action”; expanding the blocked task reveals its permission request and evidence.
- **Measurable success condition:** Users can identify task state and required action within seconds in usability tests while detailed diagnostic information remains reachable without leaving the session.

### 4.21 Honest failure states

- **User problem:** Agents soften or hide failures, conflate partial progress with completion, or blame the model when the real cause is a tool, permission, provider, or harness fault.
- **Syndrid behavior:** Syndrid uses explicit outcomes, preserves the causal chain, separates implementation state from verification state, and names the failing layer when evidence supports it.
- **Product boundary:** Attribution may remain uncertain. Syndrid says “unknown” or presents competing causes rather than fabricating certainty.
- **Example interaction:** “Implemented but unverified: build could not run because the Windows linker was missing. No runtime claim made. Checkpoint available.”
- **Measurable success condition:** Postmortems can classify failures from recorded evidence; ambiguous attribution is visibly labeled and becomes less frequent over time.

### 4.22 Originality and provenance by design

- **User problem:** Competitive research can contaminate product identity or introduce code, prompts, strings, layouts, assets, or dependencies with unclear rights.
- **Syndrid behavior:** External evidence is classified before use. Generic behavior may inform an independently written Syndrid specification; direct artifact reuse requires exact license, source, dependency, attribution, and compatibility review. Restricted sources remain clean-room or rejected.
- **Product boundary:** A repository-level license is not automatic authorization for every bundled asset, dependency, trademark, prompt, or copied upstream artifact.
- **Example interaction:** An MIT-licensed orchestration project may inform task-DAG behavior. Syndrid still creates original Rust types, terminology, prompts, tests, and UI, and any proposed file reuse receives artifact-level review.
- **Measurable success condition:** Every externally derived implementation artifact has a provenance record and satisfied obligations; rejected sources contribute no implementation-shaped material.

## 5. Core operating lifecycle

The default task lifecycle is:

**Understand intent → inspect repository → identify conventions and existing helpers → plan proportionally → establish scope and checkpoint → implement → verify → report evidence → preserve useful memory**

This lifecycle is a reliability loop, not a fixed wizard. Steps may be nearly instantaneous for a small task or represented as explicit, reviewable artifacts for a large one.

1. **Understand intent.** Restate the desired outcome internally, identify material ambiguity, recognize user constraints, and distinguish requested behavior from incidental wording.
2. **Inspect repository.** Read applicable instructions, locate relevant code and tests, observe current state, and collect enough evidence to avoid premature edits.
3. **Identify conventions and existing helpers.** Search for analogous behavior, established abstractions, naming, error handling, configuration precedence, and validation patterns.
4. **Plan proportionally.** Choose the smallest process that can safely produce the outcome. Plans should expose decisions and dependencies, not add ceremony.
5. **Establish scope and checkpoint.** Record the allowed change surface, existing working-tree state, relevant baseline checks, and a recovery point appropriate to the task.
6. **Implement.** Make coherent, minimal changes in dependency order. Monitor diff growth and stop repeated actions that produce no new information.
7. **Verify.** Exercise the affected behavior through the strongest relevant combination of tests, builds, linting, runtime launch, and visual inspection.
8. **Report evidence.** State what changed, what was observed, what remains uncertain, and how to roll back.
9. **Preserve useful memory.** Propose durable, non-obvious knowledge with scope and provenance. Do not turn transient task details into permanent memory by default.

### Task modes

Modes are adaptive starting points, not bureaucratic gates. Syndrid may recommend a different mode when risk or uncertainty becomes visible, and users can inspect what the selected mode changes.

- **Quick:** For small, clear, reversible tasks. Minimal narration, narrow inspection, no multi-agent fan-out by default, one lightweight checkpoint, and focused verification.
- **Standard:** The default for ordinary engineering work. Repository inspection, a concise plan when useful, explicit scope, relevant verification, and a complete evidence report.
- **Deep:** For ambiguous, cross-cutting, security-sensitive, architectural, or difficult debugging work. Broader research, explicit hypotheses and dependencies, stronger checkpoints, independent review, and multiple verification methods.
- **Autonomous:** For long-running, interruptible work with an approved objective and bounded authority. Durable task graph, budgets, concurrency limits, remote supervision, periodic synthesis, semantic checkpoints, recovery, and stop conditions are mandatory.

A mode does not predetermine model count, agent count, or verbosity. A Quick task can pause if hidden risk appears. An Autonomous task can still use one agent if the work is sequential. The system should optimize for appropriate control, not for performing the mode theatrically.

## 6. Agent and orchestration philosophy

Syndrid uses agents to create useful separation of concerns, not to maximize parallel activity.

### Roles

- **Coordinator:** Owns the user objective, resolves priorities, maintains the task graph, enforces budgets and scope, and produces the final accountable outcome.
- **Planner:** Converts an understood objective into a proportional, inspectable sequence of tasks, dependencies, decision points, and verification obligations. Planning may be performed by the coordinator for simple work.
- **Workers:** Execute bounded tasks and return changed artifacts, findings, evidence, and unresolved issues. Workers do not silently expand their assignment.
- **Reviewers:** Independently examine work against the objective, repository conventions, correctness, security, scope, and verification contract. Reviewers should not merely endorse the worker's narrative.
- **Specialized agents:** Apply focused capabilities such as repository exploration, Windows diagnosis, test analysis, security review, visual inspection, documentation research, or migration design.

### Task model

Each task has an objective, owner, inputs, expected outputs, dependencies, allowed change surface, context budget, tool and permission profile, usage budget, retry policy, verification requirement, and terminal outcome. Dependencies are explicit so ready work can run concurrently while conflicting or unsafe work remains ordered.

Concurrency is bounded globally and per resource. Agents that would edit overlapping files are serialized or isolated. Recursion is disabled by default and, when supported, is depth- and budget-limited. Cancellation propagates to child tasks and tool processes. Retries require a reason, a changed strategy, and a limit; repeating the same action with the same inputs is not progress.

Artifacts—not oversized chat transcripts—are the primary sharing mechanism. Repository maps, findings, plans, patches, test logs, screenshots, provenance records, and review verdicts have stable identities and source metadata. Agents receive only the artifacts and excerpts needed for their tasks, with access to request more.

Independent verification is structurally separate from implementation for consequential changes. The coordinator synthesizes worker results and reviewer disagreements, but does not erase dissent. Final synthesis states which evidence prevailed and why.

### Required orchestration visibility

Users must always be able to understand:

- **Which agents exist:** role, identifier, parent task, and lifecycle state.
- **Why they were created:** the decomposition rationale and expected value over a single-agent approach.
- **What they are doing:** current task, progress, blockers, retries, and next step.
- **What context and permissions they received:** source artifacts, memory, tools, sandbox state, approval policy, and inherited or narrowed grants.
- **What they changed:** files, patches, external side effects, and checkpoint associations.
- **What they consumed:** tokens, time, concurrency, provider quota, and estimated cost where available.
- **What failed:** model, provider, tool, permission, repository, orchestration, or unknown cause with evidence.
- **What they produced:** artifacts, findings, test results, review verdicts, and handoff summaries.

Agent state is product state. It must not exist only in debug logs.

## 7. Context and memory philosophy

Context and memory are different systems. Context is the active working set for a task or agent. Memory is durable, curated knowledge that may be retrieved into future contexts. Neither is an invisible dumping ground.

### Context

- Every task and agent has a **context budget** expressed in provider-relevant units and a reserved margin for completion and recovery.
- **Selective retrieval** loads repository instructions, files, symbols, logs, schemas, and prior artifacts because they are relevant, not merely available.
- **Summarization before exhaustion** converts completed investigation into a structured state while there is still enough context to check the summary against sources.
- **Stale-data removal** drops superseded plans, disproven hypotheses, old diffs, repetitive logs, and resolved blockers from the active working set.
- **Structured handoffs** include objective, established facts, sources, decisions, attempted approaches, unresolved risks, scope, and expected output.
- Raw evidence remains available by reference so a compacted context can be challenged or reconstructed.

### Memory

Syndrid distinguishes:

- **User memory:** stable preferences or constraints that apply across projects.
- **Project memory:** non-obvious repository-specific practices, decisions, and recurring operational requirements.
- **Session memory:** temporary state needed to continue the current body of work.

Every durable memory record includes scope, source, author or producing agent, confidence, creation and review times, visibility, and optional expiry or supersession. Repository evidence and explicit current instructions outrank memory when they conflict.

Users receive explicit controls to:

- **Inspect** what memory exists and what is about to be injected.
- **Correct** a record while preserving correction history where appropriate.
- **Save** a proposed record deliberately or according to a visible policy.
- **Forget** a record or an entire scope, including derived indexes where feasible.

There is **no invisible memory injection**. A task can show a concise list of memory records loaded, their scopes, and why they matched. Sensitive values, secrets, raw credentials, and unreviewed external instructions are not stored as ordinary memory. Imported or tool-produced content is treated as data and screened for instruction injection before becoming a candidate record.

## 8. Usage and account philosophy

Syndrid supports legitimate provider choice while keeping identity, billing, capability, and secret boundaries understandable.

Supported account mechanisms may include:

- **Subscription authentication where officially supported** by the provider and compatible client flow.
- **API keys** for provider APIs.
- **Provider profiles** describing endpoint, authentication method, model capabilities, context limits, tool support, rate-limit behavior, and known restrictions.
- **Local models** through supported local runtimes.
- **OpenAI-compatible endpoints** with capability detection and explicit compatibility caveats.
- **Named credential profiles** so users can select a purpose-specific account without copying secrets into commands or repositories.
- **Secure OS credential storage** using supported platform facilities, with file-based fallback only when secure permissions and explicit user choice make it appropriate.
- **Clear separation of subscription versus API billing,** including which account and charging model a task will use.
- **Usage, reset, rate-limit, and cost visibility where available,** with observed, provider-reported, estimated, and unavailable values clearly distinguished.

Syndrid explicitly prohibits:

- Circumventing provider limits or safety controls.
- Unsupported account automation or credential harvesting.
- Abusive account rotation intended to evade quotas, enforcement, or billing.
- Plaintext secrets in project configuration, task artifacts, logs, or memory.

Provider flexibility is capability-aware rather than slogan-driven. A configured endpoint is not assumed to support every model feature, tool schema, image input, reasoning control, streaming behavior, or usage signal. The resolved profile tells users what Syndrid knows, what it tested, and what remains uncertain.

## 9. Verification and completion contract

Syndrid separates task outcome from verification evidence. The primary outcome is one of:

- **Completed:** The requested scope is implemented and the required verification contract is satisfied.
- **Completed with warnings:** The requested scope is implemented and verified, but non-blocking warnings or limitations remain.
- **Partially completed:** A useful subset is complete; the missing scope and reason are explicit.
- **Blocked:** Progress cannot continue without an external dependency, decision, permission, credential, environment, or prerequisite.
- **Failed:** The attempted work did not produce a valid result and no safe continuation is currently available.

Verification qualifiers describe what evidence exists:

- **Implemented but unverified:** Files or configuration changed, but the required checks did not run or did not produce usable evidence.
- **Verified by tests:** Relevant automated tests passed; the exact commands and scope are recorded.
- **Verified by runtime launch:** The affected executable, service, CLI, or flow was run and the expected behavior was observed.
- **Verified visually:** The rendered UI or generated visual output was inspected against explicit acceptance criteria, with an artifact where appropriate.

Qualifiers can be combined. “Completed” without any relevant qualifier is valid only when the task has no behavioral verification surface, such as a narrowly checked prose edit.

Every completion report discloses:

- Changed files and the purpose of each change.
- Tests run and their results.
- Builds run and their results.
- Linting or static checks run and their results.
- Runtime checks and observed behavior.
- Visual checks and observed criteria.
- Skipped verification, including why it was skipped and how to perform it.
- Unresolved warnings, limitations, flaky results, or environmental concerns.
- The rollback checkpoint and what it can restore.

A command exit code is evidence, not the entire conclusion. Syndrid captures enough output to diagnose failures, avoids hiding warnings in collapsed logs, and does not convert “could not verify” into “probably works.”

## 10. Interface philosophy

A clean interface is one in which:

- The **objective and current state are immediately understandable**.
- **Active work and required user action are visible** without searching logs.
- **Secondary logs are collapsible** and do not dominate the working view.
- **Defaults are concise**, with progressive disclosure for detail.
- The **layout is stable** as status updates arrive.
- There is **no unnecessary animation**.
- There is **no focus stealing**.
- Navigation is **keyboard-first**, with discoverable shortcuts and accessible alternatives.
- **Detailed evidence remains one action away**, including diff, agents, permissions, usage, artifacts, and verification.

The compact view should answer: What is Syndrid trying to accomplish? What phase is it in? Is it progressing? Is anything blocked? Does the user need to act? What is the current safety and usage posture?

Expanded views should answer diagnostic questions without requiring a separate observability product. Users can inspect a task tree, agent details, permission inheritance, context and memory sources, tool activity, change scope, checkpoints, usage, and completion evidence.

Syndrid does not prescribe a desktop sidebar, marketplace, AI operating-system metaphor, or generic provider dashboard. Additional clients may exist, but they must preserve the same task, safety, and evidence contract. The terminal remains a first-class product surface rather than a reduced fallback.

## 11. What Syndrid is not

Syndrid is not:

- **A generic personal assistant.** Its center of gravity is software engineering work in repositories and controlled execution environments.
- **An AI marketplace.** Discoverability may exist for reviewed capabilities, but product value is not measured by catalog size.
- **A social or messaging platform.** Remote supervision is for task control and evidence, not community feeds or general chat.
- **A thin Codex skin.** Syndrid owns a distinct product layer, lifecycle, orchestration, context, memory, verification, recovery, usage, and UI strategy.
- **A wrapper that merely launches other CLIs.** Provider and tool integrations participate in one Syndrid task and safety contract.
- **An agent-count maximizer.** One reliable agent is better than unnecessary fan-out.
- **A provider-limit bypass.** Syndrid respects provider terms, quotas, billing, and supported authentication.
- **A weakened sandbox.** Friendly permissions must not reduce Codex containment or managed policy.
- **A configuration hobby project.** Configuration exists to make work predictable, not to expose every internal toggle.
- **A terminal filled with decorative telemetry.** Information earns space by helping users understand, decide, verify, or recover.
- **A promise that all models behave identically.** Capabilities, failure modes, tool use, cost, context, and instruction following vary; Syndrid exposes and manages those differences.

## 12. Codex foundation strategy

The Codex fork is an appropriate foundation because it already provides a mature Rust execution core, terminal interface, layered configuration, authentication paths, model and provider plumbing, sessions and durable state, tool execution, approvals, operating-system sandbox integration, MCP lifecycle, app-server surfaces, and existing agent infrastructure. Replacing these foundations would create large compatibility and security risk without directly solving Syndrid's product problems.

The long-term architecture is:

```text
Codex-compatible execution foundation
+ Syndrid-owned product layer
+ Syndrid-owned orchestration
+ Syndrid-owned context, memory, usage, verification, and recovery
+ Syndrid-owned UI and distribution
```

This strategy deliberately separates compatibility from product ownership.

### Keep close to upstream where possible

- **Authentication:** preserve provider-supported flows, credential compatibility, and upstream security fixes.
- **Sandbox enforcement:** retain the existing containment model and platform-specific hardening.
- **Protocol compatibility:** preserve wire values, capability negotiation, and app-server behavior unless a versioned Syndrid extension is justified.
- **Storage formats:** avoid unnecessary forks of rollout, session, history, SQLite, and related durable formats.
- **Core tool execution:** reuse hardened command, filesystem, patch, cancellation, and output behavior.
- **Provider plumbing:** retain model routing, request/response handling, and compatibility work that benefits from upstream changes.
- **Low-level terminal behavior:** keep terminal detection, input, rendering primitives, and process integration close when divergence offers no product advantage.

### Prefer Syndrid-owned seams

- **Branding:** names, copy, presentation, terminology, and public identity.
- **Distribution:** Syndrid packages, channels, signing, installers, update policy, and release lifecycle when separately implemented and reviewed.
- **Product configuration:** understandable profiles and resolved configuration without renaming or destabilizing Codex internals.
- **Orchestration:** roles, task graphs, dependency scheduling, bounded concurrency, review, synthesis, budgets, and cancellation policy.
- **Task graphs:** versioned task state, artifact contracts, replay, and user inspection.
- **Memory:** typed scopes, source metadata, controls, stale review, and safe retrieval.
- **Context optimization:** budgets, retrieval, compaction, handoffs, and stale-state removal.
- **Usage accounting:** provider-aware consumption, task budgets, estimates, limits, and attribution.
- **Verification:** verification plans, evidence types, outcome contracts, and completion reporting.
- **Rollback:** semantic checkpoints, task-aware recovery, and reversible-state metadata.
- **Observability:** agent, task, tool, permission, usage, and failure visibility.
- **TUI presentation:** compact and expanded Syndrid views, status language, task navigation, and evidence surfaces.

Brand selection remains presentation-level. It must not silently become a new protocol identity, provider identity, model identity, authentication scheme, storage namespace, sandbox identifier, or telemetry identity. The `codex` path should remain compatible while the `syndrid` path gains Syndrid-owned product behavior through explicit seams.

Upstream proximity is not passivity. Syndrid should contribute fixes upstream where appropriate, isolate intentional divergence, maintain compatibility tests, and periodically measure merge cost. New durable protocols or storage types should begin as internal or experimental contracts until the product behavior stabilizes.

## 13. Product decision test

Every proposed feature must materially improve at least one of:

- **Correctness**
- **Reduced supervision**
- **Understandability**
- **Context or token efficiency**
- **Long-running safety**
- **Configuration simplicity**
- **Failure recovery**
- **Verification trust**
- **Provider flexibility**
- **Speed**

“Materially” means the improvement can be described, measured, and compared with its cost. A feature proposal must answer:

1. Which outcome improves, for which user and task?
2. What current failure or friction does it remove?
3. What new complexity, context use, permissions, attack surface, or maintenance burden does it introduce?
4. Can the same outcome be achieved by simplifying an existing path?
5. What is the smallest coherent product behavior?
6. How will success and regressions be measured?
7. Does it preserve Codex compatibility and Syndrid provenance boundaries?

Features that primarily add providers, agents, panels, configuration knobs, decorative telemetry, or extension count fail the test unless they produce a demonstrated user outcome. A feature that improves one metric by substantially harming safety, clarity, or reliability must be redesigned rather than justified by the single gain.

## 14. Product success metrics

Syndrid should measure verified engineering outcomes, not only messages, sessions, or agent activity. Metrics should be segmented by task mode, platform, provider, model, repository size, and task class where privacy and sample size permit.

- **Percentage of tasks verified before completion:** Share of completion claims with the relevant required evidence. Target: near 100% for behavioral changes.
- **Average unnecessary diff size:** Lines and files not required by the accepted solution, measured through review sampling and reverted scope. Target: sustained reduction.
- **Repeated-action loop frequency:** Tasks that repeat materially identical tool actions without new evidence. Target: rare, automatically detected, and bounded.
- **Context compaction success:** Interrupted or compacted tasks that retain all critical objective, scope, decision, and verification state in evaluation. Target: high recovery fidelity.
- **Rollback precision:** Rollbacks that restore the intended task state without losing unrelated user changes. Target: near-perfect for locally captured state.
- **Time to diagnose failures:** Median time from failure to a supported classification and actionable next step. Target: continuous reduction.
- **Permission prompts per task:** Prompts normalized by task type and risk, paired with boundary-escape and unsafe-approval metrics. Target: fewer redundant prompts without weaker containment.
- **Long-session recovery success:** Interrupted sessions resumed without redoing completed work or restating the objective. Target: high and improving.
- **Model/provider failure attribution accuracy:** Sampled agreement between reported cause and postmortem evidence. Target: high accuracy with explicit unknowns.
- **Windows-specific failure rate:** Failures attributable to Windows paths, shells, terminals, processes, credentials, or packaging relative to other supported platforms. Target: parity.
- **User intervention frequency:** Required interventions per completed task, distinguished between meaningful product decisions and avoidable harness friction. Target: reduce avoidable intervention.
- **Token cost per verified task:** Total model tokens divided by tasks reaching the required verification state. Target: lower cost without reduced correctness.

Additional supporting metrics include time to first useful action, autonomous-task cancellation latency, agent fan-out efficiency, configuration-resolution failures, stale-memory conflict rate, secret-exposure incidents, unverified warning rate, scope-expansion frequency, and percentage of failures with a valid rollback checkpoint.

No metric should reward premature completion, suppressed warnings, fewer permission prompts through broader grants, or reduced token use through weaker verification. Metric definitions and known blind spots must be inspectable.

## 15. Prioritized capability pillars

These pillars rank product outcomes. They are not a feature backlog, release promise, or instruction to build every item in parallel.

### 1. Reliable task execution

Make the default lifecycle dependable: intent understanding, repository inspection, reuse discovery, proportional planning, minimal implementation, progress detection, cancellation, and clear outcomes. This pillar is first because every other capability exists to improve execution.

### 2. Context and memory

Actively manage context and provide explicit, structured, source-aware memory. Long sessions must compact and resume without silently carrying stale or malicious state.

### 3. Verification and evidence

Turn completion into an evidence contract. Make tests, builds, linting, runtime checks, visual checks, skipped steps, and warnings visible and comparable.

### 4. Agent orchestration and observability

Introduce roles, task dependencies, bounded scheduling, artifact sharing, independent review, synthesis, budgets, and a user-visible task and agent model. Inspectability precedes autonomy.

### 5. Scope control and recovery

Define change boundaries, monitor diff growth, create semantic checkpoints, preserve unrelated work, and support precise rollback and resume.

### 6. Configuration and permissions

Provide named profiles, resolved configuration explanations, permission inspection, scoped grants, and monotonic per-agent overlays over the Codex sandbox.

### 7. Usage and account transparency

Show which provider identity is active, how it is billed, what is consumed, what limits apply, when they reset, and which values are estimated or unavailable.

### 8. Terminal UX and Windows quality

Deliver a concise, stable, expandable TUI and treat Windows paths, shells, terminals, processes, credentials, and packaging as first-class quality concerns.

### 9. Provider flexibility

Support officially permitted subscription flows, API keys, local models, and compatible endpoints through capability-aware profiles. Breadth follows reliability and clarity.

### 10. Remote supervision

Allow secure inspection, approval, pause, cancellation, and evidence review for long-running work without turning Syndrid into a messaging platform or weakening local policy.

### Sequencing implications and open architecture decisions

The initial product wedge should emphasize accurate local status, model and reasoning visibility, role/profile state, separate sandbox and approval indicators, task activity, and honest unavailable states. Named profiles and permission inspection establish the vocabulary needed before broad orchestration. Task graphs, budgets, review, and artifacts then create the durable control plane for autonomy. Memory, skill installation, providers, remote workers, and extensions build on those safety and provenance foundations.

Several architecture decisions remain intentionally open and require experiments or explicit design records:

- Whether the existing legacy agent handlers or the newer multi-agent foundation becomes the sole orchestration base.
- When task-graph state should become a versioned app-server protocol rather than remain internal.
- Which usage and cost units are reliable across providers and how unknown cost is represented.
- Which durable abstraction owns task artifacts while preserving existing session and storage compatibility.
- How monotonic permission overlays are formally represented and proven.
- Which memory implementation is production-ready and how provenance metadata is stored.
- What minimum manifest can safely interoperate with common skill formats.
- Whether untrusted extensions use subprocess RPC, WASM, or both.
- Which container, SSH, and remote execution profiles can reuse existing sandbox helpers.
- How Syndrid distribution, signing, packaging, updates, and rollback mature beyond the current manual channel.

These are not reasons to dilute the vision. They are decisions the vision requires the product to make transparently before durable contracts harden.

## 16. Reference landscape

The references below were studied to understand public product behavior, recurring user problems, and provenance boundaries. Syndrid is not framed as taking the best pieces from every CLI. It independently solves recurring engineering problems using original product design and implementation.

Repository-level license classifications are not legal conclusions about every file. Before any direct reuse, Syndrid requires exact artifact, dependency, notice, attribution, trademark, patent, and provenance review. Behavioral inspiration means observing a user problem or generic outcome and writing an original Syndrid specification, terminology, architecture, prompts, tests, UI, and code.

All studied external references in the table were accessed **2026-07-15**. The links identify the public repository and the exact root license artifact reviewed; mutable branches still require revision pinning before any reuse decision.

| Repository | Product area studied | License and provenance classification | Syndrid reuse posture | Evidence and access date |
| --- | --- | --- | --- | --- |
| [anthropics/claude-code](https://github.com/anthropics/claude-code) | Terminal coding-agent workflows, multiple product surfaces, permissions, extensibility, and agent UX | Repository license notice is Anthropic proprietary/all-rights-reserved and subject to applicable Anthropic terms; not an open-source implementation source | **Behavioral inspiration only.** Do not copy code, prompts, strings, layouts, assets, terminology, or implementation structure. Separately licensed related repositories must be reviewed independently and are not implied by this entry. | Public README/docs and [`LICENSE.md`](https://github.com/anthropics/claude-code/blob/main/LICENSE.md); 2026-07-15 |
| [NousResearch/hermes-agent](https://github.com/NousResearch/hermes-agent) | Bounded persistent memory, session continuity, tools, and autonomous-agent workflows | MIT at repository level; notice retention required; bundled models, prompts, dependencies, and other artifacts require separate review | **Safe to reimplement generic behavior.** Artifact reuse is possible only after exact file-level review and attribution; original Syndrid design remains preferred. | Public README/docs and [`LICENSE`](https://github.com/NousResearch/hermes-agent/blob/main/LICENSE); 2026-07-15 |
| [anomalyco/opencode](https://github.com/anomalyco/opencode) | Terminal, desktop, and IDE coding-agent surfaces; role and permission concepts; provider-facing UX | MIT at repository level; copyright and permission notices must be retained in copies or substantial portions | **Safe to reimplement generic behavior.** Artifact reuse may be legally possible after provenance and dependency review, but Syndrid should not copy UI, strings, prompts, or distinctive interaction design. Public claims about plan-mode restrictions must account for configurable permissions rather than assume absolute read-only behavior. | Public README/docs and [`LICENSE`](https://github.com/anomalyco/opencode/blob/dev/LICENSE) on the primary development branch; 2026-07-15 |
| [open-multi-agent/open-multi-agent](https://github.com/open-multi-agent/open-multi-agent) | Task-DAG orchestration, scheduling, replay, context management, budgets, model routing, tools, checkpoints, and observability | MIT at repository level; direct-file obligations and third-party provenance require review | **Safe to reimplement generic orchestration behavior.** Direct TypeScript reuse is neither necessary nor preferred for the Rust-native Syndrid layer; any artifact reuse requires exact review and attribution. | Public README/docs and [`LICENSE`](https://github.com/open-multi-agent/open-multi-agent/blob/main/LICENSE); 2026-07-15 |
| [openclaw/openclaw](https://github.com/openclaw/openclaw) | Pre-inference tool policy, capability filtering, plugins, sandbox-versus-policy distinctions, and multi-agent tool governance | MIT at repository level with third-party notices; extensions, bundled assets, integrations, and dependencies may carry separate obligations | **Safe to reimplement generic policy behavior.** Artifact reuse is possible only after exact review. Syndrid must preserve its coding-agent focus and should not adopt a generic gateway, messaging, or personal-assistant identity. | Public README/docs, [`LICENSE`](https://github.com/openclaw/openclaw/blob/main/LICENSE), and repository notices; 2026-07-15 |
| [google-gemini/gemini-cli](https://github.com/google-gemini/gemini-cli) | Isolated subagents, independent histories and tools, delegation boundaries, terminal coding workflows, and contribution provenance | Apache-2.0 at repository level; license and NOTICE obligations, change marking, notice retention, and patent terms apply. Contributions also use a CLA process | **Possible implementation reuse after exact review** under Apache-2.0 obligations, but behavioral reimplementation is preferred. Do not infer that every bundled component shares the root license. | Public README/docs and [`LICENSE`](https://github.com/google-gemini/gemini-cli/blob/main/LICENSE); 2026-07-15 |
| [Aider-AI/aider](https://github.com/Aider-AI/aider) | Repository mapping, Git-centered change control and undo, lint/test integration, terminal pair-programming, and provider flexibility | Apache-2.0 at repository level; license/NOTICE retention, modification marking, attribution, and patent terms apply | **Possible implementation reuse after exact review,** though Python implementation is not a direct architectural fit for Syndrid's Rust product layer. Generic behavior may be independently reimplemented. | Public README/docs and [`LICENSE.txt`](https://github.com/Aider-AI/aider/blob/main/LICENSE.txt); 2026-07-15 |
| [charmbracelet/crush](https://github.com/charmbracelet/crush) | Terminal-native TUI, model and session UX, LSP context, MCP, skills, permissions, and cross-client workspaces | FSL-1.1-MIT. Before a version's two-year MIT conversion, competing commercial use is restricted; conversion timing must be verified per version. Redistribution and notice obligations apply | **Inspiration only or needs legal review before conversion is proven.** Do not reuse implementation artifacts in Syndrid on the assumption that the future MIT license is already effective. Do not copy Charm visual language, layouts, strings, or assets. | Public README/docs and [`LICENSE.md`](https://github.com/charmbracelet/crush/blob/main/LICENSE.md); 2026-07-15. Pin the candidate version and verify its availability date before relying on MIT conversion. |

### Internal evidence basis

This vision also relies on `docs/research/external-feature-audit.md`, `docs/research/syndrid-feature-roadmap.md`, `docs/research/provenance-policy.md`, `docs/phase-1-branding-boundary.md`, and `docs/phase-2-distribution.md`. Those documents establish the Codex compatibility strategy, Rust-native seams, inspectability-before-autonomy requirement, manual distribution state, permission constraints, and clean-room classifications.

The repository did not contain a separate local user-feedback corpus for Gemini CLI, Aider, OpenHands, OpenClaw, or Crush at the time of this pass. Public product documentation was used for the lawful repositories above, but absence of local feedback evidence is not treated as proof of user sentiment. OpenHands and Codex feedback themes in the requested problem statement inform the cross-harness problem categories only; no unlisted external implementation repository was used as a source for this document.

### Rejected sources

- `https://github.com/codeaashu/claude-code` — **rejected for provenance reasons.** It was not inspected, quoted, summarized, or used to derive requirements, implementation guidance, terminology, tests, prompts, UI, or product behavior. Its presence is recorded only to enforce the rejection boundary. Mirrors, archives, snippets, source maps, and reconstructions of the same material are likewise excluded.
