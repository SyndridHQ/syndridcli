# Experimental Syndrid banners

The `syndrid-banner-test` binary can load these presentation-only templates
when `SYNDRID_UI_RENDERER=templates` is set. Rust continues to own input,
cursor movement, scrolling, terminal bounds, commands, approvals, and all
runtime state.

Supported values are `session_id`, `version`, `workspace`, `model`, `effort`,
`lifetime_tokens`, `context`, `approval`, `access`, `connected_marker`, and
`plan_mode`. Semantic roles use `label:`, `value:`, `active:`, `muted:`,
`warning:`, `success:`, and `error:`.

Invalid external files are ignored and the bundled or built-in renderer is
used instead. Templates cannot execute expressions, commands, scripts, or
filesystem/network operations.
