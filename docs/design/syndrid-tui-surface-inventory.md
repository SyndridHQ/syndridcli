# Syndrid TUI surface inventory

Statuses reflect the current implementation and test coverage. They do not imply fresh-binary or
interactive TTY verification unless explicitly marked complete.

| Surface | Source path | Status | Syndrid difference | Automatic verification | Manual verification |
| --- | --- | --- | --- | --- | --- |
| Startup, landing, session header | `codex-rs/tui/src/history_cell/session.rs` | IMPLEMENTED | Branded card, mascot, live model/effort, supported `/` invitation | Session render tests | Fresh binary and resize pending |
| Terminal title and status line | `codex-rs/tui/src/chatwidget` | PARTIAL | Existing Syndrid seams; technical values retained | Focused status tests | TTY pending |
| Composer and footer | `bottom_pane/chat_composer.rs`, `bottom_pane/syndrid_status.rs` | IMPLEMENTED | Responsive footer with live metrics and bounded rows | Footer/layout tests | Fresh binary and resize pending |
| Slash autocomplete | `slash_command.rs`, `command_popup.rs`, `slash_input.rs` | IMPLEMENTED | Syndrid-owned descriptions and `/` entry path | Composer autocomplete tests | TTY pending |
| `/model` and `/effort` | `model_popups.rs`, `syndrid_model_view.rs`, `syndrid_effort_view.rs` | IMPLEMENTED | Catalog-backed, session-only selectors with Escape cancellation | Selector/dispatch tests | Fresh binary pending |
| `/status` | `status/card.rs`, status controls | IMPLEMENTED | Syndrid status framing with live session/policy/usage values | Status render tests | Fresh binary pending |
| `/usage` daily/weekly/cumulative | `chatwidget/tokens.rs`, usage paths | IMPLEMENTED | `Syndrid account usage · Codex account activity` framing | Usage/account tests | Fresh account-state check pending |
| Permissions and access | permission views, `bottom_pane/mod.rs` | PARTIAL | Syndrid framing; Codex semantics and enforcement remain authoritative | Existing permission tests | TTY pending |
| Approval dialogs | `bottom_pane/approval_overlay.rs` | IMPLEMENTED | Syndrid approval title, unchanged command/path/host details and controls | Approval render tests | Fresh binary pending |
| Login, account, links, and limits | onboarding, status, usage, rate-limit modules | RETAINED | Branding only where owned; official terminology remains factual | Existing focused tests | Manual account-state check pending |
| MCP, tools, shell, patches, transcript | lifecycle modules and history cells | PARTIAL | Representative gated Syndrid markers only | Focused event coverage incomplete | TTY pending |
| Plan, review, collaboration, agent state | runtime, review, selection paths | PARTIAL | Restrained markers and titles; no new orchestration | Focused coverage incomplete | TTY pending |
| Errors and empty states | slash dispatch and shared error paths | PARTIAL | Syndrid unrecognized-command marker; other shared paths retained | Focused coverage incomplete | TTY pending |
| CLI help and version | `cli/src/main.rs`, `utils/cli` | IMPLEMENTED | Syndrid root identity with compatible command schema | Branding tests | Fresh binary smoke pending |
| Update/distribution messaging | CLI/TUI update paths | RETAINED | Syndrid manual-update policy; infrastructure remains unchanged | Existing update tests | Manual check pending |

## Live-state contract

- Model: `ChatWidget::current_model()`.
- Effort: `ChatWidget::effective_reasoning_effort()`.
- Context: active `TokenUsageInfo` context usage divided by `model_context_window`.
- Tokens Sparked: `GetAccountTokenUsageResponse.summary.lifetime_tokens` only.
- Approval: live effective approval policy.
- Access: live effective/active permission profile and sandbox summary.
- Running subagents: authoritative current activity counter.

Configured defaults are not live display values. Context and account usage are independent. Missing
values display `—`; account failures retain only the last trustworthy account value. Codex rendering,
wire values, providers, authentication, storage, sandboxing, approval enforcement, and update
behavior remain unchanged.

## Manual verification checklist

- Inspect landing, header, composer, footer, status, and usage at wide, medium, narrow, and short sizes.
- Type `/` and verify supported commands, Syndrid descriptions, scrolling, and footer visibility.
- Exercise `/status`, `/usage daily`, `/usage weekly`, `/usage cumulative`, `/permissions`, `/model`, and `/effort` through real dispatch.
- Confirm model and effort agree across header, footer, status, and reopened selectors.
- Confirm Context never changes Tokens Sparked and account usage never changes Context.
- Exercise approval, shell, file-edit, MCP, plan, review, error, and empty-state paths.
- Resize with selectors and approval dialogs open; verify no clipping or panic.
- Run `syndrid --help`/`--version` and compare unchanged `codex --help`/`--version`.
- Compare the fresh binary with the approved references in `C:\SyndridBanners` and Codex mode.

## Deferred

This milestone does not add provider routing or abstraction, orchestration, memory, account
switching, context-management systems, multi-agent capabilities, or new execution behavior.
## Phase 4 UI rework inventory

The authoritative local design inventory and production mapping are maintained
in [`syndrid-ui-rework.md`](syndrid-ui-rework.md). The source directory currently
contains 19 files; `plan.txt` and `goal.txt` are referenced by its README but are
not present, so their visual layouts are intentionally not inferred.

Phase 4 adds the shared warm palette in `codex-rs/tui/src/syndrid_visuals.rs`,
focused model/effort render semantics through the existing BottomPane stack,
and the shared observable live-state adapter in
`codex-rs/tui/src/syndrid_live_state.rs`.
