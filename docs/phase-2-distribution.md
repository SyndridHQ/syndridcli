# Phase 2 SyndridCLI Distribution Boundary

**Date:** 2026-07-14

## Scope of this implementation pass

This pass isolates SyndridCLI from the existing OpenAI Codex update infrastructure. It does not add release packaging, GitHub Actions, installers, npm packages, DotSlash mappings, archive names, or a Syndrid-managed automatic update channel.

SyndridCLI continues to share the upstream-compatible Rust implementation, configuration, authentication, storage, protocol, model-routing, telemetry, and sandbox behavior documented by the Phase 1 boundary.

## Runtime distribution policy

Executable identity and update policy are represented by separate runtime-only types:

- `PublicBrand` controls public presentation such as command names, help, version output, and TUI product text.
- `DistributionChannel` controls update discovery and execution.

`DistributionChannel` is not serialized, persisted, sent over a protocol, or used as an authentication, provider, storage, telemetry, model, or sandbox identifier.

The mapping is:

| Executable identity | Public brand | Distribution channel |
|---|---|---|
| `codex` or compatibility fallback | Codex | `CodexUpstream` |
| `syndrid` / `syndrid.exe` | SyndridCLI | `SyndridManual` |

Unknown or renamed executable names continue to use the Codex-compatible fallback.

## Syndrid update behavior

### Explicit update command

`syndrid update` does not inspect the installation method and cannot construct or execute an upstream update action. It exits with:

```text
SyndridCLI automatic updates are not available yet.
Download the latest release from:
https://github.com/SyndridHQ/syndridcli/releases/latest
```

### TUI startup checks, prompts, and notices

In Syndrid mode, update policy is checked before any update cache or network work. Syndrid therefore does not:

- read the Codex `version.json` cache to determine whether to show an update;
- schedule the background latest-version refresh;
- query OpenAI GitHub Releases;
- query npm metadata for `@openai/codex`;
- query Homebrew Codex cask metadata;
- construct an OpenAI package-manager or standalone-installer action;
- show the OpenAI Codex update prompt or release-notes link;
- insert the OpenAI Codex update-available history notice;
- execute a deferred update action after the TUI exits.

The final CLI action executor also checks the distribution channel as defense in depth.

### Doctor diagnostics

The Syndrid doctor update row is deterministic and local. It reports that SyndridCLI updates are manual and points to the SyndridHQ release page.

It does not:

- inspect an npm update target;
- recommend `@openai/codex`, Bun, pnpm, Homebrew Codex, or OpenAI standalone installers;
- read the Codex update-version cache;
- invoke `curl` for OpenAI GitHub or Homebrew update metadata.

The installation row also skips package-manager update-target validation and remediation under the Syndrid distribution policy.

### Managed daemon updater

The app-server daemon receives only a runtime boolean update policy; it does not depend on branding or serialize the distribution channel.

For Syndrid:

- explicit daemon bootstrap stops any existing managed updater and does not start another one;
- bootstrap output reports `autoUpdateEnabled: false`;
- `remote-control start`, including implicit bootstrap, reconciles the updater to stopped;
- daemon remote-control enable/disable operations reconcile the updater to stopped;
- daemon lifecycle start, restart, and stop reconcile an existing updater to stopped;
- direct `app-server daemon pid-update-loop` invocation returns before entering the update loop;
- the OpenAI installer download and shell execution code is unreachable from the Syndrid channel.

The hidden updater subcommand, managed-install layout, executable version parser, updater timing, OpenAI URL constants, and Codex re-exec behavior remain unchanged for compatibility.

## Codex compatibility guarantees

For `CodexUpstream`:

1. Existing startup update checks continue to use the same GitHub, npm, and Homebrew metadata sources.
2. Existing npm, Bun, pnpm, Homebrew, Unix standalone, and Windows standalone update actions are unchanged.
3. Existing update prompts, notices, release-notes links, and CLI output are unchanged.
4. Existing doctor update probes and npm target validation are unchanged.
5. Existing managed daemon bootstrap continues to stop and restart the updater as before.
6. The managed updater continues to use the existing internal command, install layout, version parsing, download URL, timing, restart, and re-exec behavior.

## Intentionally unchanged

This pass does not change:

- protocols or serialized fields;
- OpenAI/ChatGPT authentication;
- `CODEX_HOME`, `.codex`, `auth.json`, or storage paths;
- provider IDs, model IDs, or model routing;
- telemetry or request-origin identifiers;
- npm packages or package-manager ownership;
- `install.sh` or `install.ps1`;
- GitHub Actions or release workflows;
- Bazel release packaging;
- DotSlash mappings;
- archive names or package layout;
- internal Rust crate/module names;
- the workspace version.

## Validation results

Commands were run on Windows from the repository workspace. `__COMPAT_LAYER=RunAsInvoker` was used for nextest commands where the existing test executable manifest would otherwise trigger Windows elevation error 740.

| Command | Result |
|---|---|
| `cargo nextest run -p codex-cli --test update` | **Passed with Windows compatibility override:** 2 tests passed. |
| `cargo nextest run -p codex-tui updates` | **Broad filter blocked by an unrelated existing Windows stack overflow** in `app::tests::update_memory_settings_persists_and_updates_widget_config`; the new Syndrid update test passed within the run. |
| `cargo nextest run -p codex-tui syndrid_updates_skip_startup_update_checks` | **Passed:** 1 focused test passed. |
| `cargo nextest run -p codex-tui update_` | **Broad filter blocked by unrelated existing Windows stack-overflow tests.** |
| `cargo nextest run -p codex-tui 'update_action::tests'` | **Passed:** 3 focused update-action tests passed, including Codex mappings and Syndrid isolation. |
| `cargo nextest run -p codex-app-server-daemon update_` | **Passed:** 2 tests passed. |
| `cargo nextest run -p codex-utils-cli` | **Passed:** 23 tests passed. |
| focused Syndrid doctor test | **Passed:** 2 binary-target instances passed. |
| `cargo nextest run -p codex-cli --test branding` | **Passed:** 4 tests passed. |
| `cargo build -p codex-cli --bin codex` | **Passed.** |
| `cargo build -p codex-cli --bin syndrid` | **Passed.** |
| `cargo check --release -p codex-cli` | **Passed:** release-only update paths compiled. |
| `target/debug/syndrid.exe update` | **Passed:** exited without running an action and printed the manual SyndridCLI release message. |
| `target/debug/syndrid.exe app-server daemon pid-update-loop` | **Passed:** returned immediately with no updater output or network work. |
| `target/debug/codex.exe update` | **Passed compatibility check:** preserved the existing debug-build Codex message. |
| `git diff --check` | **Passed:** Windows LF-to-CRLF warnings only. |

## Remaining Phase 2 packaging work

Independent distribution still requires separate reviewed work for:

- fork-owned Windows, Linux, and macOS release workflows;
- Syndrid release binaries and archive assembly;
- checksums, signing, and provenance;
- fork-owned Rusty V8 and optional bundled-zsh artifacts;
- Syndrid npm launchers or packages, if desired;
- Syndrid installers and DotSlash mappings;
- README installation instructions;
- a future Syndrid-owned automatic update channel.

Until that work is complete, SyndridCLI updates are manual through:

https://github.com/SyndridHQ/syndridcli/releases/latest
