# SyndridCLI Phase 3 External Feature Audit

**Audit date:** 2026-07-14
**Branch:** `phase-3/feature-extraction-audit`
**Scope:** Read-only product, architecture, license, and provenance research. No external source was copied, executed, installed, or merged.

## Executive summary

SyndridCLI already has a substantially stronger foundation than a new agent CLI: a Rust-native Ratatui application, layered configuration, model catalogs and reasoning controls, session/rollout storage, app-server and daemon transports, MCP lifecycle management, skills/plugins/hooks, subagent registries, and a security model that separates approval policy from OS sandboxing. Phase 3 should build on those boundaries rather than importing another application's runtime or replacing Codex behavior.

The strongest safe inspirations are:

- **Open Multi-Agent:** inspectable goal decomposition, task DAGs, dependency scheduling, worker/reviewer roles, plan preview, synthesis, replay, and budget accounting.
- **Hermes Agent:** visible subagent/task UX, searchable session history, progressive skill disclosure, explicit memory controls, execution-environment profiles, and extension trust warnings.
- **OpenCode:** agent profiles, ordered allow/ask/deny policy UX, project/global discovery, session-centric clients, provider capability display, MCP/LSP visibility, and TUI/server separation.

The three Claude Code-derived repositories require strict provenance controls:

- `yasasbanukaofficial/claude-code` has no visible reuse license and describes proprietary Anthropic-derived material.
- `Gitlawb/openclaude` expressly says that derived Claude Code material remains subject to Anthropic rights and was not authorized for distribution; its MIT grant applies only to contributor modifications where legally permissible.
- `codeaashu/claude-code` identifies itself as leaked source and uses an unlicensed/not-for-redistribution notice.

Those repositories are **not implementation sources**. For the Yasas and OpenClaude repositories, only generic behavior independently corroborated by lawful public sources may enter an **IDEAS_ONLY_CLEAN_ROOM** specification. The codeaashu leaked-source archive is retained only as a provenance warning and is `REJECT`, not an ideas source. No code, prompts, assets, distinctive strings, tests, file organization, or internal interfaces from any of the three may be used.

The recommended first implementation is a **presentation-only Phase 3A TUI status foundation**: a Syndrid-owned header/status strip and overlay that expose current model, reasoning effort, active agent/profile, sandbox/approval state, context usage, and running subagent/task activity using already available runtime state. It should not change protocol schemas, authentication, storage formats, model routing, approval decisions, sandbox enforcement, or core turn behavior.

## Classification legend

| Classification | Meaning |
|---|---|
| `SAFE_TO_REUSE_WITH_ATTRIBUTION` | Direct reuse may be legally possible after exact file, dependency, asset, and notice review. Attribution and license preservation are mandatory. |
| `SAFE_TO_REIMPLEMENT` | Generic behavior or architecture may be independently implemented in Rust without copying protected expression. |
| `IDEAS_ONLY_CLEAN_ROOM` | Only high-level publicly observable behavior may inform an independent specification and implementation. |
| `REJECT` | Do not copy, port, package, execute, depend on, or use as implementation provenance. |
| `NEEDS_LEGAL_REVIEW` | Rights, provenance, trademark, reverse-engineering, or derivative-work questions must be resolved before use. |

## Repository-by-repository findings

### 1. open-multi-agent/open-multi-agent

**Source record:** default branch `main`, commit [`120478d87990e20afc7ef8a917887bd583dcf6aa`](https://github.com/open-multi-agent/open-multi-agent/commit/120478d87990e20afc7ef8a917887bd583dcf6aa), accessed 2026-07-14. Sources: [repository](https://github.com/open-multi-agent/open-multi-agent), [LICENSE](https://github.com/open-multi-agent/open-multi-agent/blob/main/LICENSE), [documentation](https://open-multi-agent.com/), [tool configuration](https://open-multi-agent.com/reference/tool-configuration/), [releases](https://github.com/open-multi-agent/open-multi-agent/releases). Project-site pages are unversioned and were accessed on the same date.

1. **Purpose:** A multi-agent orchestration framework centered on converting a goal into an inspectable dependency graph, executing ready tasks concurrently, and synthesizing results.
2. **Language/runtime:** TypeScript/JavaScript monorepo; Node.js package/runtime, not a candidate runtime for SyndridCLI.
3. **License/provenance:** Root MIT License, copyright 2025 open-multi-agent contributors. No root third-party license inventory was identified; dependencies, integrations, and assets remain separate review items.
4. **User-facing features:** Single agents, teams, explicit graphs, generated plans, parallel work, progress events, tracing, structured output, verification/consensus, checkpoints, resume, and replay.
5. **Architecture:** Coordinator, task DAG/scheduler, agent runners, tool registry, shared state/memory, provider adapters, synthesis, and observability.
6. **Syndrid already has:** Agent/subagent lifecycle, concurrency reservations, model/provider infrastructure, MCP, tools, approvals, session storage, and app-server events.
7. **Syndrid partially has:** Multi-agent activity, roles, cancellation, task ownership, progress display, limits, and session continuation.
8. **Syndrid lacks:** A first-class persisted task DAG, inspectable generated plans, dependency scheduling, durable artifacts, explicit synthesis stages, and orchestration replay.
9. **Potential differentiators:** Rust-native inspectable orchestration that remains inside Codex sandbox/approval boundaries; plan preview before spawning; reviewer quorum; task-level cost/context budgets.
10. **Reject:** Treating a working directory or allowlist as a shell sandbox; unbounded generated graphs; best-effort redaction as a security guarantee; TypeScript runtime adoption.
11. **Classification:** Concepts `SAFE_TO_REIMPLEMENT`; individual MIT files potentially `SAFE_TO_REUSE_WITH_ATTRIBUTION` only after file/dependency review.
12. **Complexity:** DAG MVP `L`; durable replay and side-effect-safe recovery `XL`.
13. **Affected Syndrid components:** `codex-rs/core`, `protocol`, `tui`, `app-server`, `state`/rollout/thread store, model/provider and tool subsystems.
14. **Recommended phase:** Phase 5; visualization groundwork in Phase 3A.
15. **Risks/compatibility:** Duplicate side effects after retries, graph schema drift, token explosion, provider differences, task/session semantic mismatch, and accidental bypass of existing approvals/sandbox.

### 2. yasasbanukaofficial/claude-code

**Source record:** default branch `main`, commit [`a371abbe75ffa0d0a3c92290e2bbf56a7ef54367`](https://github.com/yasasbanukaofficial/claude-code/commit/a371abbe75ffa0d0a3c92290e2bbf56a7ef54367), accessed 2026-07-14. Sources: [repository](https://github.com/yasasbanukaofficial/claude-code), public [README provenance notice](https://raw.githubusercontent.com/yasasbanukaofficial/claude-code/main/README.md), [GitHub license guidance](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/licensing-a-repository), [official Claude Code repository](https://github.com/anthropics/claude-code), [Anthropic terms](https://www.anthropic.com/legal/commercial-terms). No implementation files were used.

1. **Purpose:** An unofficial educational/archive mirror that describes a terminal AI coding assistant and related agent features.
2. **Language/runtime:** GitHub identifies TypeScript; repository material mentions Bun/Node, but runtime validity was not tested.
3. **License/provenance:** No visible top-level `LICENSE`, `COPYING`, or `NOTICE`; README identifies underlying material as proprietary Anthropic material. Default copyright restrictions therefore apply.
4. **User-facing concepts:** Terminal assistant, tool use, agent workflows, multi-agent coordination, IDE connectivity, memory-like behavior, and interactive UI.
5. **Architecture:** Not inspected or recorded; architecture and source organization are prohibited provenance inputs.
6. **Syndrid already has:** Terminal agent workflow, tools, TUI, MCP, app-server/IDE integration seams, sessions, subagents, and configurable models.
7. **Syndrid partially has:** Specialized profiles, task visualization, transparent memory, and richer model/effort UX.
8. **Syndrid lacks:** No gap may be established from protected implementation. Only generic public product expectations may be compared.
9. **Potential differentiators:** Transparent provenance, Rust-native implementation, explicit safety layering, inspectable memory, and independently designed orchestration.
10. **Reject:** All code, prompts, assets, tests, distinctive strings, identifiers, file structures, recovered interfaces, and direct behavioral imitation based on repository study.
11. **Classification:** Repository `REJECT`; generic public behavior `IDEAS_ONLY_CLEAN_ROOM`; any direct study-to-implementation path `NEEDS_LEGAL_REVIEW`.
12. **Complexity:** Not estimated from the repository; independent Syndrid features range `M` to `XL`.
13. **Affected components:** None directly. Clean-room features would use the same Syndrid components listed elsewhere.
14. **Recommended phase:** No repository-derived implementation phase. Generic independently sourced ideas may enter normal roadmap phases.
15. **Risks/compatibility:** Copyright, trade-secret, contract, DMCA, contributor contamination, malware/supply-chain, and substantial-similarity risk.

### 3. Gitlawb/openclaude

**Source record:** default branch `main`, commit [`2ff93a10bf88ab6d7030fc4ade5316a7424fa2f9`](https://github.com/Gitlawb/openclaude/commit/2ff93a10bf88ab6d7030fc4ade5316a7424fa2f9), accessed 2026-07-14. Sources: [repository](https://github.com/Gitlawb/openclaude), [LICENSE](https://github.com/Gitlawb/openclaude/blob/main/LICENSE), public [README provenance/product notice](https://github.com/Gitlawb/openclaude/blob/main/README.md), [project documentation](https://openclaude.gitlawb.com/). Project-site pages are unversioned; no implementation details were used.

1. **Purpose:** A terminal coding-agent CLI advertising cloud/local providers, sessions, agents, MCP, skills, remote access, and editor integration.
2. **Language/runtime:** Predominantly TypeScript/ESM; npm distribution with Node runtime and Bun development tooling.
3. **License/provenance:** Not safely MIT as a whole. The license limits MIT treatment to contributor modifications where legally permissible and acknowledges Claude Code-derived material without Anthropic distribution authorization.
4. **User-facing concepts:** Streaming terminal workflow, provider profiles, permissions, agents/tasks, MCP, skills, sessions/forks, background work, headless access, and VS Code integration.
5. **Architecture:** Broad platform categories are observable, but implementation structure is excluded from Syndrid provenance.
6. **Syndrid already has:** Rust TUI/CLI, providers, model catalog, tools, sandbox/approval, MCP, skills/plugins/hooks, sessions/forks, app-server, and daemon.
7. **Syndrid partially has:** Named user-facing agent profiles, provider capability UX, permission profile UX, memory visibility, and remote worker experience.
8. **Syndrid lacks:** A polished provider-profile workflow, first-class task DAG, broad extension catalog, and IDE-facing agent-profile controls.
9. **Potential differentiators:** Stronger sandbox guarantees, provenance-gated extensions, protocol-compatible Rust clients, and transparent agent/model policy.
10. **Reject:** Code, prompts, assets, source layout, internal interfaces, derived tests, naming, and implementation translation.
11. **Classification:** Repository implementation `REJECT`; high-level public behavior `IDEAS_ONLY_CLEAN_ROOM`; any reuse claim `NEEDS_LEGAL_REVIEW`.
12. **Complexity:** Provider/profile UX `M`; remote/background platform parity `XL`.
13. **Affected components:** Conceptually `tui`, `core/config`, model providers, `protocol`, `app-server`, MCP, skills, session storage.
14. **Recommended phase:** Generic independent provider controls Phase 3B/7; no OpenClaude-derived implementation phase.
15. **Risks/compatibility:** Provenance contamination, misleading MIT assumptions, provider normalization, permission bypass through remote/MCP paths, secrets persistence, and trademark/confusion concerns.

### 4. codeaashu/claude-code

**Source record:** default branch `main`, commit [`6a2590911df240ff5ea56aa355696cfb94d128cb`](https://github.com/codeaashu/claude-code/commit/6a2590911df240ff5ea56aa355696cfb94d128cb), accessed 2026-07-14. Only public provenance notices were reviewed: [repository](https://github.com/codeaashu/claude-code), [README provenance notice](https://raw.githubusercontent.com/codeaashu/claude-code/main/README.md), [LICENSE notice](https://raw.githubusercontent.com/codeaashu/claude-code/main/LICENSE), [official Claude Code license](https://github.com/anthropics/claude-code/blob/main/LICENSE.md), and [Anthropic terms](https://www.anthropic.com/legal/commercial-terms). No product or implementation material was used as inspiration.

1. **Purpose:** The repository presents itself as an archive of allegedly leaked Claude Code source.
2. **Language/runtime:** Not relevant to reuse; implementation was not inspected.
3. **License/provenance:** License notice says unlicensed/not for redistribution and describes proprietary Anthropic material.
4. **User-facing concepts:** None are adopted from this repository. Any generally known terminal coding-assistant behavior must be sourced independently from lawful public documentation.
5. **Architecture:** Prohibited research input; not documented here.
6. **Syndrid already has:** A mature independent Codex-derived Rust architecture under Apache-2.0.
7. **Syndrid partially has:** Product areas are evaluated from lawful sources, not this repository.
8. **Syndrid lacks:** No gap analysis may be derived from protected repository internals.
9. **Potential differentiators:** Verifiable provenance and strict clean-room governance.
10. **Reject:** Downloading for development, copying, porting, packaging, executing, training on, mirroring, redistributing, or deriving implementation.
11. **Classification:** Entire repository `REJECT`; any proposed exception or access beyond the minimal provenance notices is `NEEDS_LEGAL_REVIEW`.
12. **Complexity:** Not applicable.
13. **Affected components:** None directly.
14. **Recommended phase:** None.
15. **Risks/compatibility:** Maximum legal/provenance risk, possible takedown/enforcement, contributor contamination, and malicious mirror risk.

### 5. nousresearch/hermes-agent

**Source record:** default branch `main`, commit [`6997dc81cd21dc88c6cb808a1fb3626b6ce71254`](https://github.com/NousResearch/hermes-agent/commit/6997dc81cd21dc88c6cb808a1fb3626b6ce71254), accessed 2026-07-14. Sources: [repository](https://github.com/NousResearch/hermes-agent), [LICENSE](https://github.com/NousResearch/hermes-agent/blob/main/LICENSE), [architecture](https://github.com/NousResearch/hermes-agent/blob/main/website/docs/developer-guide/architecture.md), [TUI guide](https://github.com/NousResearch/hermes-agent/blob/main/website/docs/user-guide/tui.md), [delegation](https://github.com/NousResearch/hermes-agent/blob/main/website/docs/user-guide/features/delegation.md), [memory](https://github.com/NousResearch/hermes-agent/blob/main/website/docs/user-guide/features/memory.md), [skills](https://github.com/NousResearch/hermes-agent/blob/main/website/docs/user-guide/features/skills.md), [MCP](https://github.com/NousResearch/hermes-agent/blob/main/website/docs/user-guide/features/mcp.md), [security](https://raw.githubusercontent.com/NousResearch/hermes-agent/main/SECURITY.md).

1. **Purpose:** A persistent personal agent with terminal work, memory, skills, messaging/gateway integrations, MCP, scheduled jobs, and delegated child agents.
2. **Language/runtime:** Python 3.11–3.13 backend; TypeScript/React/Ink TUI and web/desktop surfaces.
3. **License/provenance:** MIT License, copyright 2025 Nous Research. Bundled skills, plugins, catalogs, assets, generated UI, and dependencies require separate review.
4. **User-facing features:** Streaming TUI, approvals, session switching/search, attachments, subagent tree, costs/tokens, skills, memory, cron, profiles, gateways, and multiple execution environments.
5. **Architecture:** Shared agent core; registry/toolsets; SQLite sessions/search; separate durable memory; delegation; provider routing; optional registries for MCP/plugins/memory; gateway adapters.
6. **Syndrid already has:** Rust agent core, model/provider routing, tools, session storage, MCP, skills/plugins/hooks, subagents, TUI, daemon/app-server, and sandboxing.
7. **Syndrid partially has:** Searchable history, memory APIs, subagent visibility, role/profile controls, skill trust, execution environment abstraction, and cost/token display.
8. **Syndrid lacks:** Polished task/subagent overlay, explicit memory save/forget/review UX, skill provenance/install review, cron product surface, SSH/container/cloud execution profiles.
9. **Potential differentiators:** Security-correct execution profiles that preserve Codex sandboxing; provenance-signed skills; transparent memory lineage; Rust-native low-overhead orchestration.
10. **Reject:** Treating approvals/classifiers as containment; in-process privileged plugins; automatic bootstrap execution from catalogs; implicit credential inheritance; literal Python/TypeScript port.
11. **Classification:** Generic concepts `SAFE_TO_REIMPLEMENT`; individual MIT files `SAFE_TO_REUSE_WITH_ATTRIBUTION` only after exact review; bundled content often `NEEDS_LEGAL_REVIEW`.
12. **Complexity:** TUI/task visibility `M`; memory/skills `L`; remote execution and safe extension installation `XL`.
13. **Affected components:** `tui`, `core`, `state`/rollout, skills/plugins/hooks, MCP, config, app-server/daemon, sandboxing and future execution adapters.
14. **Recommended phase:** UI Phase 3A, profiles Phase 3B/4, orchestration Phase 5, skills/memory Phase 6, execution environments Phase 7.
15. **Risks/compatibility:** Main-process extension privilege, credential inheritance, untrusted skill scripts, backend isolation variance, memory leakage, and excessive provider breadth.

### 6. anomalyco/opencode

**Source record:** default branch `dev`, commit [`05c3e40a4e641732b991499000ca479e5dad4b02`](https://github.com/anomalyco/opencode/commit/05c3e40a4e641732b991499000ca479e5dad4b02), accessed 2026-07-14. Sources: [repository](https://github.com/anomalyco/opencode), [LICENSE](https://github.com/anomalyco/opencode/blob/dev/LICENSE), [agents](https://opencode.ai/docs/agents/), [permissions](https://opencode.ai/docs/permissions/), [providers](https://opencode.ai/docs/providers/), [CLI](https://opencode.ai/docs/cli/), [MCP](https://opencode.ai/docs/mcp-servers/), [skills](https://opencode.ai/docs/skills), [server](https://opencode.ai/docs/de/server/). Documentation pages are unversioned and were accessed on the same date.

1. **Purpose:** Open-source coding agent with shared local server, TUI, web/desktop clients, IDE integration, SDK, and CLI.
2. **Language/runtime:** Bun/TypeScript monorepo with native dependencies and multiple clients; not suitable as Syndrid's runtime.
3. **License/provenance:** MIT License, copyright “opencode” (2025). Marks, logos, screenshots, fonts/assets, provider services, native dependencies, and package patches require separate review.
4. **User-facing features:** Full-screen TUI, selectable agents, sessions/forks/import/export/share, provider selection, ordered permissions, commands, skills, MCP, LSP, plugins, and attached/headless clients.
5. **Architecture:** Session-centric local server with event streams and multiple clients; configurable agent profiles; provider metadata/adapters; extension discovery; permission rules.
6. **Syndrid already has:** TUI, app-server/daemon/client, sessions/forks, model providers/catalog, agent/subagent modes, approvals/sandbox, MCP, skills/plugins/hooks.
7. **Syndrid partially has:** User-facing named profiles, permission-rule UX, project/global discovery clarity, session timeline/export UX, provider capability display, remote attach, LSP integration.
8. **Syndrid lacks:** Broad client ecosystem, first-class LSP tool surface, stable public extension ABI, and polished attached-server workflow.
9. **Potential differentiators:** Preserve Codex compatibility and sandbox while adding clearer profiles, provenance-aware skills, safer extension isolation, and Rust-native clients.
10. **Reject:** Copying visual identity/assets; unrestricted JavaScript plugins; permissions as sandbox replacement; immediate 75+ provider parity; premature desktop/web ecosystem.
11. **Classification:** Concepts `SAFE_TO_REIMPLEMENT`; exact MIT code potentially `SAFE_TO_REUSE_WITH_ATTRIBUTION` after file/dependency review; assets/marks `NEEDS_LEGAL_REVIEW`.
12. **Complexity:** Profiles/permission UX `M`; LSP/MCP enhancements `L`; stable plugin host and multi-client ecosystem `XL`.
13. **Affected components:** `tui`, `core/config`, `protocol`, `app-server`, provider/model crates, skills/plugins/hooks, MCP, sessions/state, potential LSP/extension host.
14. **Recommended phase:** TUI/profile concepts Phase 3A/3B; permissions Phase 4; skills Phase 6; provider/remote/extensions Phase 7.
15. **Risks/compatibility:** App-server authentication/exposure, plugin arbitrary code, transcript privacy, provider metadata drift, LSP auto-download trust, asset/trademark reuse, and upstream merge burden.

## License and provenance matrix

| Repository | Top-level finding | File/dependency caveat | Overall classification | Legal review trigger |
|---|---|---|---|---|
| `open-multi-agent/open-multi-agent` | MIT; 2025 contributors | npm dependencies, provider SDKs, MCP/ACP integrations, external assets | `SAFE_TO_REIMPLEMENT`; direct files may be `SAFE_TO_REUSE_WITH_ATTRIBUTION` | Copying files, assets, or package code |
| `yasasbanukaofficial/claude-code` | No visible license; proprietary-material disclaimer | Full tree not cleared; alleged recovered material | `REJECT` / `IDEAS_ONLY_CLEAN_ROOM` | Any implementation exposure or reuse |
| `Gitlawb/openclaude` | Mixed/restricted: MIT only for modifications where legally permissible; derived material not authorized | `vendor`, generated artifacts, extension/assets, prompts/templates | `REJECT` / `IDEAS_ONLY_CLEAN_ROOM` | Any code, prompt, asset, structure, or behavior derived from direct study |
| `codeaashu/claude-code` | Unlicensed/not for redistribution; proprietary claim | Entire repository provenance disputed | `REJECT` | Any access beyond provenance notices or any proposed use |
| `NousResearch/hermes-agent` | MIT; © 2025 Nous Research | skills, plugins, optional MCP catalogs, UI/gateway assets, dependencies | `SAFE_TO_REIMPLEMENT`; direct files may be `SAFE_TO_REUSE_WITH_ATTRIBUTION` | Bundled content, scripts, generated assets, or copied files |
| `anomalyco/opencode` | MIT; © opencode 2025 | logos, screenshots, marks, dependencies, native packages, patches | `SAFE_TO_REIMPLEMENT`; direct files may be `SAFE_TO_REUSE_WITH_ATTRIBUTION` | Assets/marks, copied code, plugin/package reuse |

## Complete feature comparison matrix

**Legend:** `E` existing in Syndrid; `P` partial or internal but not a complete product surface; `L` materially lacking. External columns use `D` documented, `p` partial/adjacent, `—` not established, and `R` provenance-restricted source (ideas only). External entries describe public claims, not independent runtime certification.

### A. TUI and interaction design

| Feature | Syndrid | OMA | Yasas | OpenClaude | Hermes | OpenCode |
|---|---:|---:|---:|---:|---:|---:|
| Startup screen | E | — | R | R | D | D |
| Command palette | P | — | R | R | D | D |
| Slash-command completion | E/P | p | R | R | D | D |
| Collapsible tool calls | P | — | R | R | D | D |
| Streaming output | E | D | R | R | D | D |
| Progress animations | E/P | D | R | R | D | D |
| Activity indicators | E/P | D | R | R | D | D |
| Task tree | P | D | R | R | D | p |
| Subagent panel | P | D | R | R | D | D |
| Context/token meter | P | D | R | R | D | D |
| Model/effort display | E/P | D | R | R | D | D |
| Permission prompts | E | D | R | R | D | D |
| Interrupt and redirect | E | D | R | R | D | D |
| Session timeline | E/P | D | R | R | D | D |
| Compact/expanded view | P | p | R | R | D | D |

### B. Agent profiles

| Feature | Syndrid | OMA | Yasas | OpenClaude | Hermes | OpenCode |
|---|---:|---:|---:|---:|---:|---:|
| Named primary agents | P | D | R | R | D | D |
| Named subagents | E/P | D | R | R | D | D |
| Per-agent model | P | D | R | R | D | D |
| Per-agent reasoning effort | P | p | R | R | p | p |
| Per-agent instructions | E/P | D | R | R | D | D |
| Per-agent tool rules | E/P | D | R | R | D | D |
| Per-agent sandbox policy | P | p | R | R | p | p |
| Per-agent context budget | P | D | R | R | p | p |
| Per-agent maximum steps | L/P | D | R | R | p | p |
| Read-only planning/review agents | E/P | D | R | R | D | D |
| Profile switching | P | D | R | R | D | D |

### C. Multi-agent orchestration

| Feature | Syndrid | OMA | Yasas | OpenClaude | Hermes | OpenCode |
|---|---:|---:|---:|---:|---:|---:|
| Goal decomposition | P | D | R | R | p | p |
| Task DAG | L | D | R | R | p | p |
| Dependencies | L/P | D | R | R | p | p |
| Parallel execution | E/P | D | R | R | D | D |
| Coordinator | P | D | R | R | p | p |
| Worker roles | E/P | D | R | R | D | D |
| Reviewer roles | P | D | R | R | p | p |
| Retries | E/P | D | R | R | D | D |
| Cancellation | E | D | R | R | D | D |
| Task ownership | P | D | R | R | D | p |
| Result synthesis | P | D | R | R | p | p |
| Artifact passing | L/P | D | R | R | p | p |
| Live visualization | P | D | R | R | D | p |
| Concurrency limits | E | D | R | R | D | D |
| Token/cost accounting | P | D | R | R | D | D |

### D. Permissions and safety

| Feature | Syndrid | OMA | Yasas | OpenClaude | Hermes | OpenCode |
|---|---:|---:|---:|---:|---:|---:|
| Allow/ask/deny | E | D | R | R | D | D |
| Command-pattern rules | E | D | R | R | D | D |
| File-path rules | E | D | R | R | D | D |
| Tool-specific rules | E | D | R | R | D | D |
| MCP-specific rules | E/P | D | R | R | D | D |
| Subagent inheritance | E/P | D | R | R | D | D |
| Immutable safety limits | E | p | R | R | p | p |
| Sandbox interaction | E | p | R | R | D | p |
| Audit log | P | D | R | R | D | D |
| Temporary grants | E/P | p | R | R | p | p |
| Session grants | E | D | R | R | p | D |
| Project grants | P | D | R | R | p | D |

### E. Skills and reusable workflows

| Feature | Syndrid | OMA | Yasas | OpenClaude | Hermes | OpenCode |
|---|---:|---:|---:|---:|---:|---:|
| Local skills | E | p | R | R | D | D |
| Global skills | E/P | p | R | R | D | D |
| Project skills | E | p | R | R | D | D |
| Skill metadata | E/P | p | R | R | D | D |
| Skill permissions | P | D | R | R | p | D |
| Skill versioning | L/P | p | R | R | p | p |
| Skill discovery | E | p | R | R | D | D |
| Skill installation | P | p | R | R | D | p |
| Generated skills | P | p | R | R | D | p |
| Review before installation | L/P | p | R | R | p | p |
| Provenance/licensing | L/P | p | R | R | p | p |
| Open skill-format compatibility | P | p | R | R | D | D |

### F. Memory and sessions

| Feature | Syndrid | OMA | Yasas | OpenClaude | Hermes | OpenCode |
|---|---:|---:|---:|---:|---:|---:|
| Project memory | P | D | R | R | D | p |
| User memory | P | D | R | R | D | p |
| Session search | P | D | R | R | D | D |
| Searchable history | E/P | D | R | R | D | D |
| Automatic summarization | E/P | D | R | R | D | D |
| Explicit save/forget | P | p | R | R | D | p |
| Memory provenance | L/P | p | R | R | p | p |
| Memory visibility | P | D | R | R | D | D |
| Stale-memory review | L | p | R | R | p | — |
| Import/export | P | D | R | R | p | D |
| Privacy boundaries | E/P | p | R | R | D | p |

### G. Providers and execution environments

| Feature | Syndrid | OMA | Yasas | OpenClaude | Hermes | OpenCode |
|---|---:|---:|---:|---:|---:|---:|
| Provider profiles | E/P | D | R | R | D | D |
| OpenAI-compatible APIs | E | D | R | R | D | D |
| Local models | E | D | R | R | D | D |
| Capability detection | E/P | D | R | R | D | D |
| Model discovery | E | D | R | R | D | D |
| Docker execution | L/P | p | R | R | D | p |
| SSH execution | L | — | R | R | D | — |
| Remote workers | P | D | R | R | D | p |
| IDE integration | P | D | R | R | D | D |
| App-server integration | E | p | R | R | D | D |
| Shared sessions | E/P | D | R | R | D | D |

### H. Extensibility

| Feature | Syndrid | OMA | Yasas | OpenClaude | Hermes | OpenCode |
|---|---:|---:|---:|---:|---:|---:|
| Plugins | E/P | p | R | R | D | D |
| Hooks | E | D | R | R | D | D |
| Custom tools | E | D | R | R | D | D |
| Custom commands | E/P | D | R | R | D | D |
| MCP | E | D | R | R | D | D |
| LSP tools | L/P | — | R | R | p | D |
| Extension manifests | P | p | R | R | D | p |
| Compatibility versions | P | D | R | R | p | p |
| Trusted/untrusted extensions | P | p | R | R | D | p |

## Syndrid architecture mapping

| Existing boundary | Evidence in current tree | Phase 3 use | Compatibility rule |
|---|---|---|---|
| CLI/TUI entry | `codex-rs/cli/src/main.rs:94-214`; `codex-rs/tui/src/lib.rs:88-180` | Add visual identity, status, profile and task controls | Preserve no-subcommand dispatch, command schemas, terminal lifecycle, and Codex fallback behavior |
| Configuration | `codex-rs/core/src/config/mod.rs:664-1008,1289-1464` | Extend profiles and UI over existing layered config | Preserve precedence, managed requirements, locks, credential stores, and project restrictions |
| Permission profiles | `codex-rs/core/src/config/permissions.rs:48-192`; `resolved_permission_profile.rs:36-220` | Present named policy/profile state and agent overlays | Agent rules may narrow or request escalation; never silently broaden immutable limits |
| Sandbox/approvals | `codex-rs/core/src/tools/sandboxing.rs:40-260`; `codex-rs/protocol/src/permissions.rs:23-225` | Improve prompts, grants, audit trail, inheritance display | Never replace sandboxing with prompts; preserve deny-read protections and explicit dangerous bypass |
| Model/effort | `codex-rs/protocol/src/openai_models.rs:37-214`; `codex-rs/core/src/thread_manager.rs:263-272` | Profile model/effort controls and status display | Route through existing provider factory, model catalog, auth, and exact effort wire values |
| Sessions/storage | `codex-rs/core/src/thread_manager.rs:121-200,274-301`; `codex-rs/core/src/state/session.rs:25-123`; `codex-rs/tui/src/session_resume.rs:1-144` | Timeline, search, task artifacts, memory provenance | Preserve rollout JSONL/SQLite formats unless versioned through existing storage abstractions |
| Agents/tasks | `codex-rs/core/src/agent/registry.rs:16-323`; `codex-rs/core/src/tools/handlers/multi_agents_v2.rs:1-66` | Profiles, task tree, ownership, DAG scheduler | Preserve atomic slot reservation, depth limits, cancellation, encrypted communication, and v1/v2 differences |
| MCP | `codex-rs/core/src/mcp.rs:29-220`; `codex-rs/core/src/tools/handlers/mcp.rs:29-230`; `codex-rs/app-server/src/request_processors/mcp_processor.rs:7-224` | Trust UX, per-tool policy, provenance, refresh status | Preserve ordered extension/plugin overlays, per-step context, hooks, OAuth stores, and atomic runtime publication |
| App server | `codex-rs/app-server/src/transport.rs:39-237`; `codex-rs/app-server/src/request_processors/initialize_processor.rs:18-155`; `codex-rs/app-server/src/request_processors/turn_processor.rs:108-203` | Expose UI/task/profile data to clients only when necessary | Preserve initialization, auth/capability filtering, slow-client handling, experimental gating, and remote image restrictions |
| Daemon | `codex-rs/app-server-daemon/src/lib.rs:28-245` | Future remote workers/shared sessions | Preserve operation locks, socket/version lifecycle, and Syndrid update isolation |
| Extensions/skills | Existing workspace crates `codex-rs/skills`, `codex-rs/core-skills`, `codex-rs/plugins`, `codex-rs/hooks`, `codex-rs/ext/*` | Provenance manifests, installation review, isolated extension host | Do not let extensions bypass tool executor, sandbox, approval, credential, or MCP policy |

## Recommended features

1. Syndrid TUI identity and persistent status strip.
2. Named agent profiles layered over existing model/effort/instruction/tool configuration.
3. Task/subagent tree with ownership, state, elapsed time, model, and cancellation.
4. Permission-profile inspector showing sandbox plus approval state separately.
5. Inspectable task DAG with explicit approval before execution.
6. Coordinator/worker/reviewer roles using existing subagent registry.
7. Central concurrency, token, context, and optional cost budgets.
8. Session timeline and searchable history using existing thread/rollout/state abstractions.
9. Transparent user/project memory with source, timestamp, visibility, save/forget, and stale review.
10. Provenance-aware skills with review-before-install and explicit permissions.
11. MCP trust and capability panel with per-server/tool rules and refresh state.
12. Provider profile/capability UX without replacing model-provider routing.
13. Container/SSH execution profiles only as additional sandboxed execution adapters.
14. Stable extension manifests and isolated untrusted extension execution.
15. App-server task/profile events after UI/core semantics are stable.

## Rejected features

- Any code, prompt, asset, identifier, test, source organization, or distinctive UI copied from the three provenance-restricted Claude Code repositories.
- Converting SyndridCLI to TypeScript, Python, Bun, or Node.
- Replacing Codex sandbox enforcement with allow/ask/deny prompts.
- Treating agent profiles as authority to bypass managed policy or immutable safety restrictions.
- In-process execution of untrusted plugins or downloaded skills.
- Automatic execution of installer/bootstrap commands from skill, plugin, or MCP catalogs.
- Unbounded agent spawning, DAG generation, retries, memory growth, or token use.
- Broad provider parity before capability detection, credential isolation, and compatibility tests exist.
- New protocol fields solely for presentation when local TUI state already contains the data.
- Immediate web/desktop/Slack ecosystem expansion before the Rust core UI and orchestration model stabilize.

## Clean-room requirements

1. Record the external URL, access date, commit/tag when available, and exact classification before implementation.
2. For provenance-restricted repositories, requirements authors may record only generic public behavior. They must not provide code-level notes, prompts, names, strings, file paths, screenshots, or structural descriptions to implementers.
3. Implementers must work from Syndrid requirements, current Syndrid architecture, standards, official public APIs, and permissively licensed lawful references.
4. Use Syndrid-native names, types, schemas, layouts, keybindings, prompts, tests, and assets.
5. Preserve a decision log explaining independent design choices and compatibility constraints.
6. Require contributor provenance attestation for externally inspired features.
7. Run similarity and provenance review before merge; escalate uncertainty to legal review.
8. Never clone, install, execute, vendor, or package a provenance-restricted repository in the Syndrid development or release environment.

## Technical risks

- **Security regression:** profile or orchestration layers could route around sandbox/approval checks.
- **Protocol drift:** new task/profile structures could accidentally change app-server or SDK compatibility.
- **Storage drift:** task artifacts or memory could fragment rollout/state formats.
- **Concurrency correctness:** cancellation, slot reservation, retries, and parent/child teardown can race.
- **Duplicate effects:** retry/resume can repeat external writes or commands.
- **Credential propagation:** child agents, MCP servers, remote workers, and extensions can receive excessive secrets.
- **Prompt injection:** skills, memory, MCP output, and task artifacts can become persistent injection vectors.
- **Provider mismatch:** models differ in effort values, tool calling, streaming, context, and structured outputs.
- **Observability leakage:** task traces, token/cost records, memory, and exports may expose source or secrets.
- **Fork maintenance:** invasive changes to high-churn `core`, `tui`, protocol, provider, and storage code increase upstream merge cost.

## Open questions

1. Which current multi-agent implementation—legacy handlers or multi-agent v2—should be the sole Phase 5 foundation?
2. Can task-DAG state remain internal until stable, or does app-server need an experimental versioned resource early?
3. Which model/provider metrics are reliable enough for cost display without changing routing?
4. What storage abstraction should own task artifacts while retaining rollout/state compatibility?
5. How should agent-specific permission overlays be represented so they can narrow access but cannot bypass managed constraints?
6. Which existing memory crates are production-ready, and what user-visible provenance metadata is already stored?
7. What is the minimum compatible skill manifest that can interoperate with common `SKILL.md` formats while adding Syndrid provenance and permissions?
8. Should untrusted extensions use a subprocess RPC boundary, WASM component model, or both?
9. Which execution profiles can reuse existing Codex sandbox helpers rather than creating parallel isolation systems?
10. What legal review and contributor-attestation process will be required before any externally inspired implementation merges?
