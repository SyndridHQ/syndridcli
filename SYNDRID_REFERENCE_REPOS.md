# Syndrid External Reference Repository Manifest

This manifest is an architectural reference and access policy. It does not make any repository a dependency or authorize direct code copying. Repository conclusions must be revalidated against a recorded commit before implementation. The default adaptation method is clean-room architectural adaptation. Direct reuse requires explicit approval, license review, provenance review, attribution review, and compatibility review. External source must not be executed merely to inspect architecture. The prohibited Claude Code source collection below must not be cloned or inspected.

## External reference workspace

The approved external reference workspace is `C:\SyndridReferences`. Reference clones must remain outside `C:\SyndridCLI-ui`; never add them as submodules, vendor an entire repository, or modify a reference clone.

Before accessing a reference:

1. Confirm the current implementation phase genuinely needs it.
2. Use the smallest relevant reference set.
3. Check whether it already exists under `C:\SyndridReferences`.
4. Verify the canonical URL, current license, and provenance.
5. Record the inspected commit.
6. Prefer static inspection.
7. Do not run setup scripts, installers, package managers, binaries, or tests without explicit user approval.
8. Do not give untrusted code access to secrets or credentials.

Preferred clone when GitHub CLI is installed: `gh repo clone OWNER/REPO C:\SyndridReferences\NAME -- --depth 1`.

Fallback: `git clone --depth 1 https://github.com/OWNER/REPO.git C:\SyndridReferences\NAME`.

A future agent may consider `git clone --depth 1 --filter=blob:none URL PATH` for a smaller initial transfer. Do not repeatedly clone the same repository. If history is required, fetch only what the audit needs. If network access is unavailable, report that fact and do not pretend inspection occurred. README-only review is insufficient for implementation decisions.

## Inspection record

For each inspection record repository name, canonical URL, local path, default branch, inspected commit SHA, inspection date, primary language, detected license, provenance and maintenance status, latest relevant release, relevant files/modules/tests, concepts approved for clean-room adaptation, interfaces approved for integration, any code approved for direct reuse, attribution requirements, security and performance concerns, rejected concepts, and unresolved questions.

Useful static commands include `git -C <path> remote -v`, `git -C <path> branch --show-current`, `git -C <path> rev-parse HEAD`, `git -C <path> log -1 --format="%H%n%cI%n%s"`, `git -C <path> status --short`, `Get-ChildItem <path> -Filter "LICENSE*" -Force`, `Get-ChildItem <path> -Filter "COPYING*" -Force`, `Get-ChildItem <path> -Filter "NOTICE*" -Force`, and `Get-Content <path>\README.md`. Use `rg` or repository-native search for actual source and tests. Record conclusions against a commit and date; classifications are not permanent.

## Adaptation categories

- **Clean-room architectural adaptation:** understand a public concept and implement it independently; do not mechanically copy source structure. This is the preferred default.
- **External subprocess integration:** invoke an installed external CLI through a bounded interface, such as GitHub CLI, with explicit approvals.
- **API-compatible integration:** use a documented external API or protocol without copying implementation internals.
- **Direct code reuse:** only with explicit user approval, exact license/provenance/attribution/compatibility review, and recorded source files plus commit SHA.
- **Prohibited source:** do not clone, inspect, copy, or derive implementation details.

## Repository inventory

### 1. OpenAI Codex

- Repository: <https://github.com/openai/codex>
- Local source: `C:\SyndridCLI-ui`
- Possible upstream reference: `C:\SyndridReferences\openai-codex`
- Classification: Runtime foundation.
- Access: inspect the local fork first; use `git remote -v` to determine whether an upstream exists; do not add or modify remotes without approval; compare public upstream only when a phase requires it; do not assume the fork matches current upstream.
- Inspect: thread/turn creation, child threads/agents and forking, model/effort overrides, events and correlation, cancellation, approvals, sandboxing, MCP, tools, token/context accounting, rollout persistence/resume, app-server v2, configuration, and verification-related events.
- Approved: actual Syndrid runtime, existing Codex abstractions, compatible extensions, existing security and persistence mechanisms.
- Reject: a second model/tool runtime, sandbox, persistence engine, token truth, or unnecessary thread/session duplication.
- Relevant phases: O0–O14.

### 2. GitHub CLI

- Repository: <https://github.com/cli/cli>
- Suggested path: `C:\SyndridReferences\github-cli`
- Classification: Optional external subprocess integration.
- Inspect: `gh auth status`, JSON output, PRs, checks, workflow runs, issues/API, exit codes, authentication, coding-agent documentation, and subprocess safety.
- Approved: invoke installed `gh`, detect authentication, inspect PRs/checks/workflows, create PRs only after approval, and use APIs through `gh` where useful.
- Do not: embed its Go runtime, copy large Go modules, replace local Git, push/merge/force-push/delete branches, or open PRs without approval.
- Relevant phase: O12.

### 3. Open Multi Agent

- Repository: <https://github.com/open-multi-agent/open-multi-agent>
- Suggested path: `C:\SyndridReferences\open-multi-agent`
- Classification: Orchestration architecture reference.
- Inspect: coordinator, decomposition, explicit workflows, task graphs, scheduler/dependencies/state, progress/traces/checkpoints, token tracking, retries, cancellation, MCP, and replay.
- Approved concepts: typed workflow plans, explicit dependencies, deterministic scheduling, progress events, checkpoints, per-task usage attribution, replayable state, and observability.
- Reject initially: uncontrolled DAGs, default swarming, unbounded expansion, unrestricted recursive delegation, TypeScript runtime embedding, and a second runtime beside Codex.
- Relevant phases: O1, O2, O4, O6, O7, O8, O11.

### 4. Historical OpenCode

- Repository supplied by the user: <https://github.com/opencode-ai/opencode>
- Suggested path: `C:\SyndridReferences\opencode-legacy`
- Classification: Historical architecture and terminal UX reference.
- The supplied repository may be archived or historical; future agents must verify current status before relying on it.
- Inspect: historical Go architecture, sessions, SQLite, Bubble Tea, LSP, providers, tools, and commands.
- Approved: historical session architecture, persistence, terminal UX, and command/provider separation.
- Do not treat it as current, use it as the runtime, assume it represents current OpenCode, or embed the Go app.
- Relevant phases: O0, O2, O13, and TUI work.

### 5. Current OpenCode candidate

- Candidate identified by supplied prior research: <https://github.com/anomalyco/opencode>
- Suggested path: `C:\SyndridReferences\opencode-current`
- Classification: Candidate active architecture, provider, session, and UX reference.
- Verify the current canonical repository before cloning and record its canonical URL and inspected commit.
- Inspect: client/server separation, agents, build/plan mode, subagents, providers, model switching, sessions, compaction, context, permissions, tools, MCP, GitHub integration, and performance instrumentation.
- Approved concepts: client/server boundaries, provider-independent descriptions, session configuration, build-versus-plan behavior, compaction ideas, and extensible tool boundaries.
- Reject replacing Codex sandboxing, prompt-only permissions as sole enforcement, TypeScript runtime embedding, and premature provider-layer reconstruction.
- Relevant phases: O0, O5, O7, O8, and future routing.

### 6. Crush

- Repository: <https://github.com/charmbracelet/crush>
- Suggested path: `C:\SyndridReferences\crush`
- Classification: TUI, session, provider, MCP, and client/server reference.
- Inspect: terminal layout, modals, command discovery, model/session switching, provider/MCP configuration, LSP, shell safety, usage accounting, client/server design, narrow terminals, and rendering performance.
- Approved concepts: polished terminal interaction, session UX, model switching, discovery, responsive layout, usage safeguards, and modal behavior.
- License: verify the exact current license before reuse inspection; supplied prior research indicated FSL-1.1-MIT. Prefer conceptual adaptation; direct copying is not approved by default.
- Reject replacing Codex runtime/sandbox or importing unrelated provider complexity early.
- Relevant phases: TUI work, O7, O8, O9, O10.

### 7. Kimi Code

- Repository: <https://github.com/MoonshotAI/kimi-code>
- Suggested path: `C:\SyndridReferences\kimi-code`
- Classification: Subagent isolation, role, lifecycle-hook, context, and TUI reference.
- Inspect: coder/explore/plan roles, isolated contexts, hooks, delegation, handoffs, sessions, approvals, MCP, tool results, startup, and TUI.
- Approved concepts: narrow roles, isolated contexts, compact handoffs, hooks, read-only exploration, and keeping the main conversation clean.
- Reject embedding its runtime, assuming provider behavior maps to Codex, copying prompts without review, or complete transcript handoffs.
- Relevant phases: O1, O4, O5, O7, O8, O11.

### 8. Grok Build

- Repository: <https://github.com/xai-org/grok-build>
- Suggested path: `C:\SyndridReferences\grok-build`
- Classification: Rust architecture, runtime separation, TUI, tools, workspace, VCS, and checkpoint reference.
- Prioritize composition root, binary, runtime, shell, tools, workspace/VCS, configuration, MCP, sandbox, checkpoints, ACP, headless mode, tracing/telemetry, upload, transfer, and remote storage. Possible paths such as `crates/codegen/xai-grok-pager-bin`, `xai-grok-pager`, `xai-grok-shell`, `xai-grok-tools`, and `xai-grok-workspace` must be verified before use.
- Approved concepts: Rust crate boundaries, composition-root and runtime/TUI separation, workspace/VCS abstraction, checkpoints, headless execution, and ACP concepts.
- Security: audit upload, telemetry, tracing, and bundle paths; never automatically upload a full repository, `.git`, environment files, secrets, or unrelated files; do not copy telemetry by default.
- Reject forking as runtime, assuming snapshots are complete production behavior, blindly copying generated workspace configuration, or copying vendored code without license review.
- Relevant phases: O0, O1, O2, O4, O13, TUI, performance.

### 9. Hermes Agent

- Repository: <https://github.com/nousresearch/hermes-agent>
- Suggested path: `C:\SyndridReferences\hermes-agent`
- Classification: Memory, skills, retrieval, compression, and historical-learning reference.
- Inspect: persistent memory, retrieval, skills, trajectory compression, tool registry, providers, task history, learning, privacy/safety, and relevance.
- Approved concepts: bounded task summaries, relevant prior sessions, reusable skills, compressed trajectories, local forecasting inputs, and overhead measurement.
- Reject embedding Python, unbounded injection, secret storage, unnecessary complete transcripts, unverified memory as truth, or stale memory overriding current evidence.
- Relevant phases: O9, O10, O13.

### 10. Claux

- Repository: <https://github.com/ducks/claux>
- Suggested path: `C:\SyndridReferences\claux`
- Classification: Small Rust terminal UX, permission-mode, and testing reference.
- Inspect: terminal architecture, permission presentation, command flow, sessions, visual regression, snapshots, approvals, and modules.
- Approved concepts: concise permission UX, isolated Rust modules, visual regression, and terminal interaction.
- Reject treating it as a full runtime, copying Claude-specific behavior without review, or replacing Syndrid TUI foundations.
- Relevant phases: TUI, permission display, snapshots.

### 11. Claude Code Rust

- Repository: <https://github.com/srothgan/claude-code-rust>
- Suggested path: `C:\SyndridReferences\claude-code-rust`
- Classification: Rust frontend, runtime-adapter, and session-UX reference.
- Inspect: frontend/runtime boundary, terminal events, sessions, tools, approvals, external bridges, and adapters.
- Approved concepts: frontend organization, session visualization, adapter boundaries, and runtime separation.
- Reject Claude dependency, external bridge as Syndrid runtime, proprietary-internal inference, and unreviewed copying.
- Relevant phases: TUI and adapter architecture.

### 12. Claude Code source collection

- Repository supplied by the user: <https://github.com/chauncygu/collection-claude-code-source-code>
- Classification: Prohibited source and provenance risk.
- Do not clone, download, inspect implementation files, unpack, copy, transform, summarize implementation for Syndrid, reconstruct Claude Code, derive architecture from leaked/decompiled source, or add it to `C:\SyndridReferences`.
- Permitted: retain this warning URL; use official Anthropic documentation, independently observable public behavior, and clean-room implementation.
- Relevant phase: none.

### 13. Claw Code

- Repository: <https://github.com/ultraworkers/claw-code>
- Suggested path: `C:\SyndridReferences\claw-code`
- Classification: Experimental/reference-only; provenance and license review required.
- Before inspection verify license, provenance, README, LICENSE, PHILOSOPHY/provenance statements, independence, maintenance, and production-use guidance.
- Potential concepts after review: Rust modules, workflow/activity presentation, verification UX, and context management.
- Reject runtime foundation, dependency, unreviewed code, unverified claims, or experimental behavior treated as production-ready.
- Relevant phase: optional UX research only.

## Reference selection by phase

Future agents must use the smallest relevant set, not inspect every repository for every task:

| Phase or surface | Relevant references |
|---|---|
| O0 local audit | Local Codex; upstream Codex only if necessary; Grok Build for crate boundaries |
| O1 typed model | Local Codex; Open Multi Agent; Grok Build; Kimi Code |
| O2 events | Local Codex; Open Multi Agent; Grok Build |
| O3 configuration | Local Codex; Kimi Code; current OpenCode where relevant |
| O4 workflow | Local Codex; Open Multi Agent; Kimi Code; Grok Build |
| O5 handoffs | Local Codex; Kimi Code; Hermes Agent; current OpenCode |
| O6 budgets | Local Codex token events; Open Multi Agent accounting; Crush concepts |
| O7 recommendations | Local Codex; Open Multi Agent; current OpenCode; Kimi Code |
| O8 automatic mode | Local Codex; Open Multi Agent; Kimi Code; current OpenCode |
| O9 forecasting | Local history; Hermes Agent; Open Multi Agent |
| O10 Adaptive Efficiency | Local history; Hermes Agent; Crush; Open Multi Agent |
| O11 read-only exploration | Local Codex; Kimi Code; Open Multi Agent |
| O12 GitHub | GitHub CLI; local Git |
| O13 persistence/recovery | Local Codex; Open Multi Agent; Grok Build; historical OpenCode where useful |
| TUI views | Existing Syndrid TUI first; Crush; Kimi Code; Grok Build; Claux; Claude Code Rust where useful |

The prohibited source collection is excluded from every phase.

## Future prompt preamble

Future orchestration prompts should begin:

> Before changing code, read and obey:
> - `AGENTS.md`
> - `SYNDRID_ORCHESTRATION.md`
> - `SYNDRID_REFERENCE_REPOS.md`
> - every nested `AGENTS.md` applicable to touched files.

They must require the agent to state current phase, intended scope, applicable instruction files, relevant references, whether each was actually inspected, inspected commit SHAs, planned files, validation plan, expected compatibility impact, and expected usage/performance impact. Prompts must not claim Codex has access to the original ChatGPT Web research conversation; these root documents are the durable context.

## Reference extraction template

```text
Reference source:
Repository:
Canonical URL:
Local path:
Inspected commit:
Inspection date:
Relevant files:
Relevant tests:
Concept adapted:
Adaptation category:
License:
Provenance:
Attribution:
Why it fits Codex:
What was not adopted:
Security considerations:
Performance considerations:
Syndrid implementation files:
Validation evidence:
```

The default adaptation category is **Clean-room architectural adaptation**.
