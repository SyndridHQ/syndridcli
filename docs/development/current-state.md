# SyndridCLI current state

## Repository and branch

- Repository: `C:\SyndridCLI` (`SyndridHQ/syndridcli`)
- Branch: `phase-3b/model-effort-controls`
- Base: Phase 1 branding, Phase 2 distribution isolation, and Phase 3A status foundation are merged.

## Phase 3B scope

Phase 3B is a Syndrid-only presentation and live-session-state milestone. It does not add
orchestration, memory, provider abstraction, account switching, new execution systems, or new
slash commands beyond the approved `/model` and `/effort` controls.

The current worktree contains the newer implementation on top of the earlier staged snapshot.
The index is intentionally not updated in this pass; final staging remains a user review decision.

## Authoritative live-state contract

All Syndrid presentation surfaces must derive from these sources:

| Value | Authority | Lifecycle |
| --- | --- | --- |
| Model | `ChatWidget::current_model()` | Updated by live model/session events; configured defaults remain separate. |
| Reasoning effort | `ChatWidget::effective_reasoning_effort()` | Updated by live reasoning/session events and collaboration-mode changes. |
| Context | `TokenUsageInfo::last_token_usage.tokens_in_context_window()` over `model_context_window` | Refreshes on token updates, compaction, and model changes; missing data displays `—`. |
| Tokens Sparked | `GetAccountTokenUsageResponse.summary.lifetime_tokens` | Populated by the typed account usage response; failures retain the last trustworthy value or `—`. |
| Approval | Live `Config.permissions.approval_policy` | Reflects the effective approval policy used by Codex enforcement. |
| Access | Live effective/active permission profile and sandbox summary | Reflects the effective access boundary, not a configured label alone. |
| Running subagents | `ChatWidget`'s current activity counter | Updated by authoritative activity events; no activity is fabricated. |

Context never populates Tokens Sparked, and account usage never populates Context. Header, footer,
`/status`, selectors, and usage framing must preserve these distinctions. Required caches are
presentation adapters only: the owner is `ChatWidget`, updates happen during the corresponding
event/result lifecycle, and account-scoped values are invalidated on account-state changes.

## Implemented and automatically verified

- `PublicBrand`-gated Syndrid landing/session presentation.
- Syndrid composer and responsive footer rendering.
- Real Syndrid `/model` and `/effort` dispatch.
- Catalog-backed model and provider-supported effort choices.
- Session-only model/effort event paths with Escape cancellation coverage.
- Live model/effort propagation into the session header and footer snapshot.
- Independent Context and Tokens Sparked state paths.
- Syndrid status and usage framing, approval title branding, and representative gated markers.
- Codex-mode compatibility branches and existing Codex terminology preservation.

The focused tests and checks in the current worktree must be rerun before commit; historical
validation claims in earlier versions of this document are not evidence for the current tree.

## Manually verified

Earlier manual work verified the landing screen, composer, and footer in a prior binary. The fresh
current Phase 3B binary has not yet been verified after the latest live-state and selector repairs.
Therefore selector hierarchy, footer allocation, responsive resizing, status/usage framing, and
Codex-versus-Syndrid visual comparison remain unverified.

## Partially implemented

- Permissions/access: Syndrid framing exists, but underlying Codex selection and enforcement remain shared.
- Terminal title/status line: existing Syndrid seams exist, but full surface coverage is not claimed.
- MCP, shell, file, transcript, plan, review, error, and empty-state markers: representative only.
- Distribution: the Cargo `syndrid` target exists, but packaged release/install infrastructure remains Codex-compatible and deferred.

## Intentionally retained or deferred

OpenAI/ChatGPT authentication, Codex storage and protocol identifiers, provider routing, model wire
values, sandbox enforcement, approval enforcement, update mechanics, account terminology, and
official diagnostic links remain unchanged. Backend work—provider abstraction, orchestration,
memory, account switching, new execution systems, and durable context-management systems—has not
started and is outside Phase 3B.

## Stable manual build path

Use the existing repository checkout and one stable target directory. From Command Prompt:

```text
cd /d C:\SyndridCLI\codex-rs
cargo build -p codex-cli --bin syndrid --target-dir ..\target-syndrid-dev
```

Do not create additional uniquely named Syndrid target directories. The untracked
`build-syndrid.cmd` is a local helper from an earlier pass and still points at an obsolete target
directory; it is not part of the Phase 3B deliverable.

## Remaining manual TTY checklist

- Fresh `syndrid` landing, header, composer, and footer.
- `/`, `/model`, and `/effort` through real dispatch.
- Selector Escape cancellation, current selection, supported efforts, and no duplicated numbering.
- Wide, medium, narrow, short, zero-width, and resize behavior with selectors and approval dialogs open.
- `/status`, `/usage daily`, `/usage weekly`, `/usage cumulative`, and truthful Context/Tokens Sparked values.
- Permissions, approval, shell/file/MCP, plan/review, error, and empty-state representative markers.
- `syndrid --help`/`--version` versus unchanged `codex --help`/`--version`.
- Comparison with the approved references in `C:\SyndridBanners` and a Codex-mode run.

## Review boundary

Before commit, the current worktree should contain one coherent Phase 3B implementation, no
obsolete staged-only repair path, no generated target artifacts in the review set, and documentation
that distinguishes automatic verification from manual TTY verification. No backend implementation
should begin until this milestone is reviewed and committed.
## Phase 4 UI rework checkpoint

The worktree was already dirty before this milestone, including broad Bazel
file edits and an untracked `build-syndrid.cmd`; those changes were preserved.
The expected Phase 3B commit is `6b276aaad`, but a Phase 4 branch was not
created because switching branches could overwrite unrelated work.

The TUI already owns a BottomPane navigation stack, Syndrid-gated model and
effort screens, typed account usage and token/context state, transcript/history,
approval interruption, and resize-aware rendering. Phase 4 builds on those
seams. The centralized palette is warm and three-level; model and effort views
are focused render states; and live view cycling/state is represented by the
Syndrid-owned `LiveView`/`LiveSessionState` adapter.

The dedicated full-screen command browser and live Dashboard/Activity/Changes/
Verification renderers are not yet wired. Status, usage, permissions, plan,
goal, review, diff, and resume retain their existing compatible production
paths pending their focused design slices. The example values in the external
designs remain placeholders and are not used as runtime data.
