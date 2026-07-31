# Syndrid UI rework

This milestone is implemented against the local authoritative designs in
`C:\SyndridBanners\Updated`. The designs are clean-room visual requirements;
their example values are not runtime data.

## Design inventory and production map

| Design file | Production seam | State/input source | Status |
|---|---|---|---|
| `README.txt` | `syndrid_visuals`, BottomPane render root | `PublicBrand::Syndrid`, Ratatui layout | Foundation applied |
| `home.txt` | startup/home transcript and composer | ChatWidget session/model/config state | Existing surface; responsive palette applied |
| `input-regular.txt` | `bottom_pane/chat_composer.rs` | composer textarea, keymap, context snapshot | Existing surface; palette seam applied |
| `input-plan.txt` | same composer and collaboration-mode state | plan mode and keymap | Existing behavior preserved |
| `command-menu-default.txt` | `command_popup.rs` / slash autocomplete | real `SlashCommand` registry and feature flags | Curated popup remains; full-screen browser deferred |
| `command-menu-all.txt` | `command_popup.rs` / slash registry | real command availability and filter | Categorized full-screen browser deferred |
| `session-dashboard.txt` | future Syndrid live renderer | `syndrid_live_state::LiveSessionState` | State foundation added; renderer deferred |
| `session-activity.txt` | transcript/history cells | existing ChatWidget transcript | Raw activity preserved; dedicated renderer deferred |
| `session-changes.txt` | diff model and workspace probes | existing diff/Git seams | Dedicated live renderer deferred |
| `session-verification.txt` | verification events and status cells | existing command/test/build events | Dedicated live renderer deferred |
| `model.txt` | `syndrid_model_view.rs` | authoritative model catalog | Implemented; focused render semantics applied |
| `effort.txt` | `syndrid_effort_view.rs` | provider-supported reasoning values | Implemented; focused render semantics applied |
| `permissions.txt` | existing permissions selection view | real config permission/sandbox enums | Existing behavior preserved; dedicated design pass deferred |
| `status.txt` | `SlashCommand::Status` and status history | real status/token/policy sources | Existing Codex status path preserved; dedicated Syndrid view deferred |
| `usage.txt` | usage menu and typed account activity | real account usage responses | Existing typed path preserved; dedicated Syndrid view deferred |
| `plan.txt` | no file supplied | observable transcript plan progress | Missing authoritative design; no invented layout |
| `goal.txt` | no file supplied | existing goal extension/menu | Missing authoritative design; existing behavior preserved |
| `review-colors.txt` | review history/diff surfaces | existing findings and severity styles | Light treatment deferred |
| `diff-colors.txt` | diff renderer | existing patch model | Light treatment deferred |
| `resume-colors.txt` | resume picker | existing session metadata | Light treatment deferred |

## Architecture

`PublicBrand::Syndrid` remains the only presentation gate. The Codex render
paths, backend protocols, approval enforcement, sandbox behavior, model wire
values, storage, and account terminology are unchanged.

Syndrid visual primitives live in `codex-rs/tui/src/syndrid_visuals.rs` with
three warm surface levels, semantic error/success colors, Unicode-width-safe
truncation, and shared canvas/panel/focused styles. Live presentation state is
centralized in `syndrid_live_state.rs`; its values are observable-only and
support `Exact`, `Derived`, `Estimated`, and `Unavailable` quality semantics.

The existing BottomPane view stack is the focused navigation stack. Model and
effort screens now render as focused screens without re-rendering the session
composer beneath them; Esc cancels and Enter uses the existing session-only
update paths. Approval overlays continue to interrupt the stack through the
existing approval owner.

Live view order is `Dashboard -> Activity -> Changes -> Verification`, with
forward/backward transitions represented by `LiveView::next` and
`LiveView::previous`. Wiring these controls into the running session renderer
is intentionally the next implementation slice.

## Truthful data and performance

Design examples are never runtime constants. Unavailable data renders as `—`.
The live adapter is cache-shaped so future renderers can update from existing
events, render visible rows only, and avoid network/Git work during rendering.
Animations remain deferred; no new animation loop or input delay was added.

## Verification and manual follow-up

Automated verification covers the new palette/live-state unit seams and must be
run with the repository's `just fmt` and focused TUI test workflow. A fresh
binary still requires manual TTY inspection at wide, desktop, medium, narrow,
very narrow, short-height, and resize-while-open sizes. Check model/effort
Enter/Esc behavior, composer text/cursor preservation, slash completion, plan
mode Shift+Tab, approval interruption, and unchanged Codex rendering.

## Deferred surfaces

The dedicated full-screen default/all command browser, dashboard/activity/
changes/verification renderers, status/usage redesigns, permissions redesign,
plan/goal layouts, and review/diff/resume light styling remain deferred. The
missing `plan.txt` and `goal.txt` files must be supplied before their layouts
can be implemented faithfully.
