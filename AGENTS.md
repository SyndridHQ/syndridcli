# SyndridCLI repository instructions

## Product identity

Syndrid is a compatibility-preserving fork of OpenAI Codex. It is a coding-agent CLI and harness, not a generic personal assistant or AI marketplace. Preserve Codex compatibility unless a task explicitly requires divergence. Prefer Syndrid-owned layers and narrow seams over broad Codex-core rewrites.

## Architecture boundaries

Keep close to upstream wherever possible: authentication, sandbox enforcement, approval enforcement, provider protocols, model wire values, serialized schemas, storage and rollout/history formats, core tool execution, and low-level terminal behavior.

Prefer Syndrid-owned seams for branding, distribution, TUI presentation, orchestration, task graphs, memory, context management, usage accounting, verification, rollback, and observability.

## Branding

- Gate public Syndrid behavior with `PublicBrand`.
- Do not use `DistributionChannel` for UI branding or presentation.
- Keep Codex behavior and wording unchanged unless explicitly requested.
- Never imply that OpenAI officially operates or endorses Syndrid.

## Reliability

Research unfamiliar systems before modifying them. Reuse repository conventions and helpers. Never invent models, effort values, usage values, tokens, provider capabilities, account data, or test results. Do not claim completion without evidence; report skipped verification and unresolved warnings honestly. Keep changes narrow and do not silently expand scope.

## Git and safety

Inspect staged and unstaged changes before editing and preserve unrelated work. Never reset, restore, clean, revert, stash, discard, commit, or push without explicit user permission. Use semantic, scoped modifications.

## Resources and verification

- Work in the main agent only; do not spawn subagents, workflows, or review fan-outs.
- Run one Cargo command at a time.
- Prefer `CARGO_BUILD_JOBS=2`, `CARGO_PROFILE_DEV_DEBUG=0`, and `CARGO_PROFILE_TEST_DEBUG=0`.
- Prefer focused tests; do not run the full workspace without explicit permission.
- Windows and PowerShell are first-class environments.

For implementation work: inspect architecture, make the smallest change, run focused tests, run the relevant check, build only affected binaries when needed, manually verify TUI changes, and run `git diff --check`. Snapshots do not replace real visual TUI verification.

In `codex-rs`, run `just fmt` after code changes. Do not run `cargo test` directly; use the repository’s `just test` workflow and project-specific tests first. Ask before a complete workspace test. Follow existing Rust conventions, including inlined `format!` arguments, collapsible `if`s, method references, exhaustive matches, documented new traits, and scoped modules. Do not modify code related to `CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR` or `CODEX_SANDBOX_ENV_VAR`.
