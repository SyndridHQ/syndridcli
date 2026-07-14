# Phase 1 SyndridCLI Branding Boundary

**Date:** 2026-07-14

## Purpose

This implementation adds the smallest runtime branding layer needed to launch the existing Codex CLI as `syndrid` while retaining the existing `codex` executable and compatibility behavior.

SyndridCLI is built from OpenAI Codex. OpenAI and ChatGPT authentication remains provider-owned and continues to use the existing OpenAI/ChatGPT authentication services, identifiers, and storage.

The repository's Apache License 2.0 `LICENSE`, existing `NOTICE`, OpenAI attribution, and third-party attribution remain unchanged and must continue to be distributed with the fork.

## What changed

### Dual Rust binaries

The existing `codex-cli` Cargo package now declares two binary targets that share `cli/src/main.rs`:

- `codex`
- `syndrid`

No main implementation was copied. Bazel, npm packages, installers, release workflows, and packaged artifacts were intentionally not changed.

### Runtime brand selection

`codex_utils_cli::PublicBrand` selects presentation from the original executable name in `argv[0]`:

- `syndrid` or `syndrid.exe` selects Syndrid mode.
- Every other executable name defaults to Codex mode.

Defaulting unknown and platform-specific executable names to Codex preserves existing behavior and avoids treating internal helper aliases or renamed binaries as SyndridCLI.

`PublicBrand` is presentation-only. It is not used as a protocol, provider, authentication, telemetry, storage, model, or sandbox identifier.

### CLI help and version

| Invocation | Behavior |
|---|---|
| `codex --help` | Preserves the existing Codex title and `codex` usage. |
| `codex --version` | Preserves `codex-cli <version>`. |
| `syndrid --help` | Displays `SyndridCLI` and root usage beginning with `syndrid`. |
| `syndrid --version` | Displays `SyndridCLI <version>`. |

The version output remains two whitespace-delimited tokens so existing managed-install version parsing remains compatible.

Only the root help identity is dynamically branded. Nested compatibility examples and subcommand schemas remain unchanged in this pass.

### TUI presentation

Syndrid mode carries an immutable presentation brand into the TUI. The changed surfaces are limited to:

- onboarding welcome text;
- the initial/session TUI product header;
- `/status` product/header text;
- the onboarding device-code phishing warning.

Syndrid onboarding states that SyndridCLI is built from OpenAI Codex. Syndrid device-code authentication states that authentication is provided by OpenAI/ChatGPT.

Codex remains the default presentation for the standalone TUI and existing tests. Existing Codex snapshots should therefore remain unchanged.

### Device-code authentication

The existing `run_device_code_login` API remains Codex-default. A branded wrapper is used by the `syndrid login --device-auth` path.

Only the printed prompt changes. The following remain unchanged:

- OAuth and device-auth URLs;
- client IDs and claims;
- request and polling behavior;
- token exchange and refresh behavior;
- workspace restrictions;
- credential persistence and keyring behavior;
- app-server authentication protocol values.

### State database recovery

Only user-facing recovery sentences and the displayed `doctor` command follow the public brand. Database detection, lock/corruption classification, paths, filenames, backup behavior, and recovery APIs remain unchanged.

## What intentionally remains named Codex

The following identifiers and behaviors remain unchanged by design:

- `CODEX_HOME`;
- `~/.codex` and project `.codex` directories;
- `auth.json`;
- `OPENAI_API_KEY`, `CODEX_API_KEY`, and `CODEX_ACCESS_TOKEN`;
- OpenAI/ChatGPT OAuth URLs, client IDs, claims, account IDs, and workspace IDs;
- keyring service/account identifiers;
- MCP configuration keys and OAuth storage identifiers;
- app-server method names, fields, enum wire values, and protocol versions;
- provider IDs, model IDs/slugs, and routing behavior;
- `model_reasoning_effort`, app-server `effort`, and Responses API `reasoning.effort`;
- session, SQLite, history, rollout, model-cache, and update-cache formats;
- sandbox helper names, flags, aliases, metadata, and setup markers;
- internal Rust crate, module, and folder names;
- telemetry and request-origin identifiers;
- update infrastructure and upstream update behavior;
- npm package names and launchers;
- installers, archive names, DotSlash configuration, and release workflows.

This is intentional compatibility, not incomplete internal renaming.

## Compatibility guarantees

1. Existing `codex` builds and command invocations remain available.
2. Both public binaries execute the same Rust runtime and subcommand implementation.
3. Existing Codex configuration, authentication, sessions, history, and database files are shared and remain readable.
4. No configuration or serialized protocol schema changed.
5. No authentication, provider selection, model routing, reasoning, or sandbox behavior changed.
6. Codex remains the fallback brand for unknown executable names.
7. Existing authentication accurately remains identified as OpenAI/ChatGPT-owned in Syndrid mode.

## Focused tests added

- executable-name brand detection, including `.exe` and safe Codex fallback;
- `codex --help` and `codex --version` compatibility;
- `syndrid --help` and `syndrid --version` branding;
- Codex and Syndrid device-code prompt wording;
- Syndrid onboarding welcome rendering;
- Syndrid TUI status/header rendering.

## Known future packaging work

The `syndrid` binary is currently a Cargo target only. A distributable SyndridCLI package still requires separate, reviewed work for:

- Bazel multiplatform/release targets;
- npm package ownership and launchers;
- native artifact and archive naming;
- Windows/macOS/Linux installers;
- DotSlash mappings;
- signing and release workflows;
- update-channel isolation or replacement;
- a packaged `codex` compatibility alias policy.

None of that infrastructure was modified in this implementation pass.

## Validation commands and results

Commands were invoked from `C:\SyndridCLI\codex-rs` on 2026-07-14.

| Command or check | Result |
|---|---|
| `cargo build --bin codex` | **Passed.** |
| `cargo build --bin syndrid` | **Passed.** |
| `syndrid --help` | **Passed:** root branding is `SyndridCLI`. |
| `syndrid --version` | **Passed:** outputs `SyndridCLI 0.0.0`. |
| `codex --help` | **Passed:** remains `Codex CLI`. |
| `codex --version` | **Passed:** remains `codex-cli 0.0.0`. |
| `cargo nextest run -p codex-cli --test branding` | **Passed:** 4 tests passed. |
| `codex-login` `device_code_prompt` tests | **Passed:** 2 tests passed. |
| `codex-tui` `welcome_` tests | **Passed:** 3 tests passed. |
| `codex-tui` `syndrid_status_header_uses_syndrid_branding` | **Passed:** 1 test passed. |
| `codex-utils-cli` `public_brand` tests | **Passed:** 2 tests passed. |
| `just fmt` | **Passed.** |
| `git diff --check` | **Passed:** Windows LF-to-CRLF warnings only. |

The broad `just test -p codex-cli` run was blocked on Windows by the existing update test executable requiring elevation (`os error 740`). This environment-specific test-runner restriction is not a branding implementation failure.
