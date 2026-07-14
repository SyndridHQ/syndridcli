# SyndridCLI Phase 1 Repository Audit

**Audit date:** 2026-07-14  
**Scope:** Read-only architecture, build, branding, authentication, model routing, TUI, CI, Windows, licensing, and fork-maintenance audit.  
**Upstream relationship:** This repository is a fork of `openai/codex`. The Phase 1 branch is `phase-1/syndrid-foundation`.

## Executive summary

The production CLI is a native Rust binary. The package is `codex-cli`, declared in `codex-rs/cli/Cargo.toml`, and its binary target is currently named `codex` with entry point `codex-rs/cli/src/main.rs`. The root `codex-cli` npm package is only a JavaScript launcher that locates and starts a platform-specific native binary; it does not build the Rust program.

The safest Phase 1 boundary is a **public-shell rebrand**, not an internal rename. It can introduce the `syndrid` command, `SyndridCLI` display name, help/version identity, TUI welcome/header text, packaging metadata, and fork documentation while retaining compatibility-sensitive Codex/OpenAI identifiers. In particular, do not globally replace `codex` or `openai`: many instances are persistent paths, environment variables, wire values, OAuth endpoints, model IDs, MCP keys, helper aliases, updater contracts, and serialized protocol names.

A release-quality executable rename is broader than changing the Cargo `[[bin]]`: npm launchers, platform packages, installer layouts, DotSlash mappings, release archive scripts, update behavior, Windows helper packaging, tests, and CI all assume `codex[.exe]`. The current updater is a critical fork hazard because it checks and installs upstream OpenAI Codex; an incompletely branded fork could prompt users to overwrite SyndridCLI with upstream Codex.

---

## 1. Workspace and package structure

### 1.1 Repository-level structure

The repository is a hybrid Rust/Node/Bazel monorepo.

| Path | Role |
|---|---|
| `codex-rs/` | Primary Rust workspace and product implementation. |
| `codex-rs/cli/` | Main multipurpose executable package. |
| `codex-rs/tui/` | Interactive Ratatui application. |
| `codex-rs/core/` | Agent session, turn, tool, request, and orchestration core. |
| `codex-rs/app-server*` | App-server implementation, transport, daemon, client, and protocol. |
| `codex-rs/login/` | Authentication persistence, OAuth/device-code flows, and token refresh. |
| `codex-rs/config/` | Configuration schema, loading, profiles, and writes. |
| `codex-rs/model-provider*`, `models-manager/` | Provider/auth routing and model catalog selection. |
| `codex-rs/protocol/`, `codex-api/` | Internal/public protocol types and Responses API wire structures. |
| `codex-rs/windows-sandbox-rs/` | Native Windows sandbox library and helper binaries; path dependency, not a listed workspace member. |
| `codex-cli/` | npm package and Node launcher for the native CLI. |
| `codex-rs/responses-api-proxy/npm/` | npm launcher package for the native responses proxy. |
| `sdk/typescript/` | TypeScript SDK package. |
| `.github/workflows/`, `.github/actions/` | CI, release, signing, and repository automation. |
| `scripts/` | Install, package staging, release, and maintenance scripts. |
| `bazel/`, `BUILD.bazel`, `MODULE.bazel`, `defs.bzl`, `rbe.bzl` | Bazel build definitions and infrastructure. |
| `docs/` | User and contributor documentation. |
| `third_party/` | Third-party source/license material. |
| `tools/` | Repository tools. |
| `.devcontainer/` | Development-container setup. |

Root orchestration files include `package.json`, `pnpm-workspace.yaml`, `pnpm-lock.yaml`, `justfile`, and the Bazel files above. Root Node requirements are Node `>=22`, pnpm `>=10.33.0`, with pnpm pinned to `10.33.0` (`package.json:34-38`). Rust is pinned to `1.95.0` with `rustfmt`, `clippy`, and `rust-src` (`codex-rs/rust-toolchain.toml:1-3`).

### 1.2 pnpm workspace

`pnpm-workspace.yaml:1-4` contains exactly three packages:

1. `codex-cli`
2. `codex-rs/responses-api-proxy/npm`
3. `sdk/typescript`

The root package is private and provides repository formatting/schema scripts only (`package.json:1-9`).

- `codex-cli/package.json` publishes/defines the `@openai/codex` launcher and maps `codex` to `bin/codex.js`.
- `codex-rs/responses-api-proxy/npm/package.json` provides the proxy launcher.
- `sdk/typescript/package.json` builds and tests the SDK; it does not produce the CLI.

### 1.3 Rust workspace

The Rust workspace is defined by `codex-rs/Cargo.toml:1-128`. Its listed members are:

```text
aws-auth
analytics
agent-graph-store
agent-identity
backend-client
bwrap
ansi-escape
async-utils
app-server
app-server-transport
app-server-daemon
app-server-client
app-server-protocol
app-server-test-client
apply-patch
arg0
feedback
features
install-context
codex-backend-openapi-models
code-mode
code-mode-host
code-mode-protocol
codex-home
cloud-config
cloud-tasks
cloud-tasks-client
cloud-tasks-mock-client
cli
collaboration-mode-templates
connectors
config
context-fragments
shell-command
shell-escalation
skills
core
core-api
core-plugins
core-skills
hooks
http-client
secrets
exec
file-system
exec-server-protocol
exec-server
execpolicy
ext/agent
ext/connectors
ext/extension-api
ext/goal
ext/guardian
ext/image-generation
ext/items
ext/memories
ext/mcp
ext/skills
ext/web-search
external-agent-migration
keyring-store
file-search
file-watcher
linux-sandbox
lmstudio
login
codex-mcp
mcp-server
memories/read
memories/write
model-provider-info
models-manager
network-proxy
ollama
process-hardening
protocol
realtime-webrtc
prompts
rollout
rollout-trace
rmcp-client
responses-api-proxy
response-debug-context
sandboxing
stdio-to-uds
otel
tui
tools
v8-poc
websocket-client
utils/absolute-path
utils/path-uri
utils/cargo-bin
git-utils
utils/cache
utils/image
utils/json-to-toml
utils/home-dir
utils/pty
utils/readiness
utils/rustls-provider
utils/string
utils/cli
utils/elapsed
utils/sandbox-summary
utils/sleep-inhibitor
utils/approval-presets
utils/oss
utils/output-truncation
utils/path-utils
utils/plugins
utils/fuzzy-match
utils/stream-parser
utils/template
codex-client
codex-api
state
terminal-detection
test-binary-support
thread-manager-sample
thread-store
uds
codex-experimental-api-macros
plugin
model-provider
```

`codex-rs/windows-sandbox-rs` is intentionally notable: it is referenced as a Windows path dependency (`codex-rs/cli/Cargo.toml:90-95`) but is not in the workspace member array. Package/build changes must test it explicitly rather than assuming every workspace-wide command covers it.

### 1.4 Executable packages

Important native executable packages include:

- Main CLI: `codex-rs/cli`, binary `codex`.
- App server: `codex-rs/app-server`, including `codex-app-server` and test/exec server binaries.
- Code-mode host: `codex-rs/code-mode-host`, binary `codex-code-mode-host`.
- Responses proxy: `codex-rs/responses-api-proxy`, binary `codex-responses-api-proxy`.
- Windows helpers: `codex-rs/windows-sandbox-rs`, binaries `codex-command-runner` and `codex-windows-sandbox-setup` (`Cargo.toml:13-19`).
- Linux sandbox and utility/test binaries in their respective crates.

No generated `target/` executable or checked-in Windows `.exe` was found during the audit.

---

## 2. Main executable production path

The authoritative production executable is:

- **Cargo package:** `codex-cli` (`codex-rs/cli/Cargo.toml:1-6`)
- **Binary target:** `codex` (`codex-rs/cli/Cargo.toml:8-10`)
- **Entry point:** `codex-rs/cli/src/main.rs`
- **Library:** `codex_cli`, `codex-rs/cli/src/lib.rs` (`Cargo.toml:12-15`)

`codex-rs/cli/src/main.rs:91-121` defines the top-level Clap command and subcommands. `main`/the async dispatcher begin around `:956-964`. With no subcommand, the dispatcher enters the interactive TUI. It also dispatches `exec`, review, login/logout, MCP, app-server, update, doctor, sandbox, resume, fork, and other modes.

The production TUI path is `cli/src/main.rs` -> `run_interactive_tui` (`:2236-2313`) -> `codex_tui::run_main` (`tui/src/lib.rs:848-853`). `tui/src/main.rs` is a small standalone wrapper, not the primary packaged executable.

Bazel defines the CLI crate and `multiplatform_binaries(name = "codex")` in `codex-rs/cli/BUILD.bazel:5-16`. The root `justfile` delegates to `codex-rs` and wraps Cargo/Bazel commands.

The npm path is separate:

- `codex-cli/package.json` maps command `codex` to `codex-cli/bin/codex.js`.
- `codex-cli/bin/codex.js:16-23` selects platform packages.
- It resolves `vendor/<target>/bin/codex` or `codex.exe` (`:79-110`) and spawns it (`:179-198`).
- It is a launcher only; native binaries are staged by release/package scripts.

---

## 3. Current Windows build, test, and run commands

### 3.1 Support status

`docs/install.md:3-8` presents Windows 11 through WSL2 as the documented/supported path. The source and CI nevertheless contain real native Windows MSVC support for x64 and ARM64, including native sandbox helpers, release packaging, signing, and test matrices. Phase 1 documentation should distinguish **officially supported** from **technically buildable/CI-tested** rather than claiming parity prematurely.

### 3.2 Prerequisites

In PowerShell:

```powershell
# Install Rust using rustup first, then:
rustup component add rustfmt clippy
cargo install --locked just
cargo install --locked cargo-nextest

# JavaScript workspace, when needed:
corepack enable
pnpm install --frozen-lockfile
```

Use Node 22+ and pnpm 10.33.0. Python is required by packaging and some repository recipes. Native Rust builds require the Visual Studio/MSVC build tools appropriate for the target.

### 3.3 Exact native Windows commands

From `C:\SyndridCLI\codex-rs`:

```powershell
# Build the main debug executable.
cargo build --bin codex
# Output: C:\SyndridCLI\codex-rs\target\debug\codex.exe

# Build the main release executable.
cargo build --release --bin codex
# Output: C:\SyndridCLI\codex-rs\target\release\codex.exe

# Run interactive TUI.
cargo run --bin codex

# Run with an initial prompt.
cargo run --bin codex -- "explain this codebase to me"

# Run non-interactively.
cargo run --bin codex -- exec "your prompt"

# Package-specific tests through the repository-required nextest wrapper.
just test -p codex-cli

# Full Rust workspace suite.
just test

# Direct PowerShell equivalent of the Windows test recipe.
$env:RUST_MIN_STACK = "8388608"
$env:NEXTEST_PROFILE = "local"
cargo nextest run --no-fail-fast -p codex-cli

# Checks.
just fmt-check
just clippy -p codex-cli
```

The Windows `just test` recipe sets `RUST_MIN_STACK=8388608` and `NEXTEST_PROFILE=local`, then invokes `cargo nextest run --no-fail-fast` (`justfile:84-86`). `just fmt` and `just fix` modify files and therefore were not run for this audit.

Equivalent convenience commands from `codex-rs` are:

```powershell
just codex -- "prompt"
just exec -- "prompt"
```

From the repository root, the Bazel paths are:

```powershell
just bazel-codex -- "prompt"
just build-for-release

# Direct forms:
bazel run //codex-rs/cli:codex -- "prompt"
bazel build //codex-rs/cli:release_binaries
```

`just bazel-codex` contains Windows-specific current-directory and PowerShell argument handling (`justfile:123-125`).

Other relevant native commands:

```powershell
# Windows helpers (manifest is outside the main workspace member list).
cargo build --manifest-path .\windows-sandbox-rs\Cargo.toml --bins

# App server.
cargo run -p codex-app-server --bin codex-app-server

# Responses API proxy.
cargo run -p codex-responses-api-proxy
```

JavaScript/SDK checks from the root:

```powershell
pnpm install --frozen-lockfile
pnpm --dir sdk/typescript run build
pnpm --dir sdk/typescript test
pnpm --dir sdk/typescript run lint
```

The `codex-cli` npm launcher package has no native build or test script of its own.

---

## 4. Minimum safe user-facing branding boundary

### 4.1 Recommended boundary

Phase 1 should change the **public command and presentation layer** while preserving the internal compatibility layer:

- Public product name: `SyndridCLI`.
- Primary executable/command: `syndrid` / `syndrid.exe`.
- Help usage and version prefix: `syndrid ...` and `SyndridCLI <version>`.
- TUI welcome/header/onboarding text: SyndridCLI.
- README/install/build documentation and public package metadata: SyndridCLI/fork-owned locations.
- Release artifact and launcher lookup: add or switch to the `syndrid` native binary in a coordinated commit series.
- Prefer retaining a temporary `codex` compatibility alias during Phase 1 if packaging permits it.

Do **not** rename Rust crate/module names in Phase 1. The `codex-*` crate graph is large and largely invisible to users; changing it creates merge conflicts without delivering the requested product identity.

### 4.2 Smallest runtime display changes

For a locally built Rust binary, the smallest set of runtime identity surfaces is:

1. `codex-rs/cli/Cargo.toml:8-10` — binary target name (`codex` -> `syndrid`, or add a second compatibility target/wrapper).
2. `codex-rs/cli/src/main.rs:91-105` — Clap command name, `bin_name`, usage, help, and version presentation.
3. `codex-rs/tui/src/onboarding/welcome.rs:94-99` — welcome text.
4. `codex-rs/tui/src/status/card.rs:708-716` — main TUI product/version header.
5. `codex-rs/login/src/device_code_auth.rs:149-157` — device-code login welcome/product text, without changing OpenAI/ChatGPT authentication identity.
6. User-visible recovery/error text such as `codex-rs/cli/src/state_db_recovery.rs:37-82`.
7. Tests/snapshots that assert those exact strings.

Other version-bearing TUI surfaces should be checked for consistency: `tui/src/app/history_ui.rs:94-115`, `tui/src/chatwidget.rs:1492-1504`, and `tui/src/history_cell/session.rs:149-156`. They may only need the display label changed, not the embedded numeric version source.

### 4.3 Files required for a distributable `syndrid` executable

A real packaged command requires coordinated changes beyond the runtime minimum:

- `codex-rs/cli/Cargo.toml`
- `codex-rs/cli/src/main.rs`
- `codex-rs/cli/BUILD.bazel`
- root `justfile`
- `codex-cli/package.json`
- `codex-cli/bin/codex.js` (or a new launcher path)
- `codex-cli/scripts/build_npm_package.py`
- `scripts/stage_npm_packages.py`
- `scripts/install/install.ps1`
- `scripts/install/install.sh`
- `.github/dotslash-config.json`
- `.github/scripts/build-codex-package-archive.sh`
- Windows and cross-platform release workflows under `.github/workflows/`
- packaging/install/update tests and snapshots
- `README.md`, `codex-rs/README.md`, `docs/install.md`, getting-started/authentication docs, and command examples

Current Windows package layouts explicitly expect `bin\codex.exe`, plus `codex-code-mode-host.exe`, `codex-command-runner.exe`, and `codex-windows-sandbox-setup.exe` (`scripts/install/install.ps1:529-552`). The helper binaries should not be renamed just because the main command changes.

### 4.4 Version-output contract

`codex-rs/app-server-daemon/src/managed_install.rs:38-99` invokes `--version` and parses the second whitespace token. `SyndridCLI 1.2.3` preserves that shape; an arbitrary decorative string may break managed update logic. Treat version output as a machine-consumed compatibility contract and add a test before changing it.

---

## 5. Internal identifiers that must remain unchanged

The following are not ordinary branding strings.

### 5.1 Authentication and account identity

Keep:

- `CODEX_HOME`, `~/.codex`, `.codex`, and `auth.json`.
- `OPENAI_API_KEY`, `CODEX_API_KEY`, `CODEX_ACCESS_TOKEN`.
- Keyring service/account identifiers such as `Codex Auth`, `CODEX_AUTH`, and the `cli|<hash>` key derived from canonical `CODEX_HOME`.
- OpenAI/ChatGPT OAuth issuer/token URLs, callback/open-app routes, JWT claim URLs, account IDs, workspace IDs, FedRAMP flags, and authorization headers.
- Serialized auth values such as `apiKey`, `chatgpt`, `chatgptDeviceCode`, `chatgptAuthTokens`, and `amazonBedrock` in app-server protocol types.

Changing these would log users out, create split credential stores, break OAuth/JWT interpretation, or violate app-server compatibility.

### 5.2 Configuration and persistence paths

Keep:

- `%ProgramData%\OpenAI\Codex` system configuration path unless a deliberate migration/fallback design is approved.
- `${CODEX_HOME}/config.toml`, profile files, project `.codex/config.toml`, and `.codex/.env`.
- SQLite/state/log/history filenames and schema identifiers.
- `sessions/YYYY/MM/DD/rollout-<timestamp>-<UUID>.jsonl[.zst]` layout.
- `models_cache.json`, `version.json`, thread UUIDs, rollout IDs, response IDs, and stored model preset IDs/slugs.

A future migration may support new locations, but Phase 1 should read/write existing Codex locations to preserve user state.

### 5.3 Protocol and MCP

Keep:

- App-server JSON-RPC method names, request/response field names, enum wire values, and protocol versions.
- MCP TOML keys: `mcp_servers`, `mcp_oauth_credentials_store`, `mcp_oauth_callback_port`, and `mcp_oauth_callback_url`.
- MCP server names, OAuth storage/lock identities, and command protocol.
- Public SDK and responses-proxy API/package identifiers unless intentionally versioned as a compatibility break.

The visible command can become `syndrid mcp`, but the configuration/wire vocabulary must remain stable.

### 5.4 Model routing

Keep:

- Provider IDs, `requires_openai_auth`, auth-mode classifications, base URL selection rules, and header names.
- Model slugs and stable model preset IDs, including `openai.gpt-*`/first-party IDs and auto-selection slugs.
- Config keys `model`, `review_model`, `model_provider`, `model_providers`, `model_reasoning_effort`, `plan_mode_reasoning_effort`, `model_reasoning_summary`, `model_verbosity`, and `model_catalog_json`.
- Protocol distinctions: config `model_reasoning_effort`, app-server `effort`, and Responses wire `reasoning.effort`.
- The intentional internal `Ultra` -> wire-level `max` mapping.

These values select providers/models and are API contracts, not product display text.

### 5.5 Sandboxing and helper dispatch

Keep:

- `--codex-home` and `CODEX_HOME` passed to Windows helpers.
- Helper executable aliases such as `codex-linux-sandbox`, `codex-command-runner`, and `codex-windows-sandbox-setup` unless every arg0, packaging, and setup reference changes together.
- `codex/sandbox-state-meta`, sandbox setup marker versions, ACL/security-principal names, IPC/exec-server names, and JSON metadata fields.
- `arg0` dispatch aliases and its `.codex/.env` behavior (`codex-rs/arg0/src/lib.rs:93-207`).

Sandbox naming is coupled to security-sensitive process dispatch and package materialization.

### 5.6 Update and install behavior

Keep stable until the updater is deliberately forked or disabled:

- Internal updater subcommands such as `app-server daemon pid-update-loop`.
- Managed-install version parsing shape.
- Existing install-layout lookup needed to detect/migrate old installations.

However, the upstream update endpoints themselves are **not safe as Syndrid defaults**. They currently target Homebrew cask `codex`, npm `@openai/codex`, GitHub `openai/codex`, and `chatgpt.com/codex` install scripts. They must be isolated or disabled before claiming an independent Syndrid update channel.

---

## 6. Authentication flow

### 6.1 Entry points

CLI login/logout dispatch is in `codex-rs/cli/src/main.rs:1339-1393`. The implementation is `codex-rs/cli/src/login.rs`:

- Browser OAuth clears prior auth, constructs server options, and starts a local callback server (`:137-195`).
- API-key login reads/persists a key (`:198-225`).
- Access-token/PAT/agent-identity input is handled at `:228-303`.
- Device-code auth and browser fallback are at `:305-422`.
- Status and logout/revoke are at `:424-503`.

TUI onboarding uses `codex-rs/tui/src/onboarding/auth.rs` and `tui/src/local_chatgpt_auth.rs`. The underlying auth-mode enum is `codex-rs/protocol/src/auth.rs:6-55`.

### 6.2 Storage

`codex-rs/login/src/auth/storage.rs:38-61` defines the stored schema. File storage is `${CODEX_HOME}/auth.json` (`:150-223`) and uses Unix mode `0600` where applicable. Backends include file, keyring, auto, encrypted secrets, and ephemeral storage (`:498-540`).

`codex-rs/login/src/auth/manager.rs` controls load, cache, restrictions, and refresh:

1. `CODEX_API_KEY`, when environment-key auth is enabled.
2. Ephemeral/external auth.
3. `CODEX_ACCESS_TOKEN`, classified as PAT or Agent Identity JWT.
4. Persistent file/keyring/auto storage.

See `load_auth` around `:1215-1299`. Forced login method/workspace checks can invalidate mismatched auth (`:1081-1163`). Managed ChatGPT auth is proactively refreshed; refresh is semaphore-protected and checks account identity before persisting (`:2019-2037`, `:2362-2439`).

OAuth code exchange and persistence are in `codex-rs/login/src/server.rs:778-903`. Token/JWT claims are represented in `login/src/token_data.rs:10-42,137-160`.

### 6.3 Provider authentication

`codex-rs/model-provider-info/src/lib.rs:35-52,241-298` defines built-in provider URLs and converts configured provider data into an API provider. ChatGPT-backed auth selects the ChatGPT Codex backend; API-key/custom provider auth selects the configured/OpenAI API base URL.

`codex-rs/model-provider/src/provider.rs:96-317` owns provider capabilities, account state, and auth resolution. Header behavior is in `model-provider/src/auth.rs:80-254`, including authorization, `ChatGPT-Account-ID`, FedRAMP headers, configured headers, and Agent Identity bootstrap.

### 6.4 Branding constraint

Authentication screens may say “SyndridCLI” as the client product, but must continue to accurately identify OpenAI/ChatGPT when the user is authenticating with those services. Do not relabel the identity provider as Syndrid.

---

## 7. Model selection and reasoning-effort flow

### 7.1 Configuration layers

Config precedence is documented in `codex-rs/config/src/loader/mod.rs:94-106`:

1. Admin/MDM configuration.
2. System configuration (`/etc/codex/config.toml` or Windows ProgramData).
3. Cloud configuration bundle.
4. User `${CODEX_HOME}/config.toml`.
5. Selected profile.
6. Trusted project `.codex/config.toml`.
7. Runtime/CLI/UI overrides.

Project-local config cannot override provider/base-URL-sensitive fields (`loader/mod.rs:58-74`). The main keys are in `config/src/config_toml.rs:151-161,285-288,352-380`; profile equivalents are in `profile_toml.rs:24-40`.

### 7.2 Model discovery/defaulting

Model metadata and effort presets are defined in `protocol/src/openai_models.rs:37-69,170-245`. `models-manager/src/manager.rs:26-275` loads/sorts the catalog, filters it by auth/backend, preserves explicit selections where possible, chooses defaults, applies overrides, and caches to `models_cache.json`. Unknown slugs use fallback metadata (`models-manager/src/model_info.rs:121-164`).

Session startup refreshes models, resolves the configured/default model, and obtains model metadata in `core/src/session/mod.rs:563-635`.

### 7.3 TUI selection and persistence

The model popup is `tui/src/chatwidget/model_popups.rs:10-269`. It emits model and reasoning-effort update events. Keyboard effort stepping is in `chatwidget/reasoning_shortcuts.rs:42-154`. Normal and plan effort state are maintained separately (`chatwidget/settings.rs:137-278`).

Events are routed through `tui/src/app/event_dispatch.rs`; thread settings are synchronized and persisted via `tui/src/config_update.rs` and `app/config_persistence.rs`. Persistent keys are `model`, `model_reasoning_effort`, and `plan_mode_reasoning_effort`.

### 7.4 Per-turn and wire flow

The flow is:

```text
config/profile/CLI/TUI
  -> selected model + collaboration-mode effort
  -> TUI app-server turn/start (model, effort, summary, service tier, etc.)
  -> thread settings / core TurnContext
  -> ModelClient
  -> Responses request { model, reasoning: { effort, summary, ... } }
  -> HTTP SSE /responses or Responses WebSocket
```

Relevant files:

- `tui/src/app_server_session.rs:786-835`
- `app-server-protocol/src/protocol/v2/turn.rs:71-155`
- `tui/src/app/thread_settings.rs:15-195`
- `core/src/session/turn_context.rs:201-210,438-575`
- `core/src/client.rs:803-907,1395-1824`
- `codex-api/src/common.rs:138-263`
- `codex-api/src/endpoint/responses.rs:60-163`

The public TOML key is `model_reasoning_effort`; app-server v2 serializes `effort`; the Responses API receives `reasoning.effort`. Preserve that distinction.

---

## 8. TUI entry points and major components

### 8.1 Entry/lifecycle

- `codex-rs/cli/src/main.rs` — top-level command and no-subcommand TUI dispatch.
- `codex-rs/tui/src/lib.rs:848-853` — public `run_main`.
- `tui/src/lib.rs:1237-1761` — terminal setup, update prompt, app-server startup, onboarding/trust/login, resume/fork/session selection, and `App::run`.
- `tui/src/lib.rs:1782-1811` — terminal restoration guard.
- `tui/src/tui.rs:375-769` — TTY validation, raw mode, paste/keyboard enhancements, color probing, event stream, and alternate screen.

### 8.2 Main application

`tui/src/app.rs:503-587` contains the principal `App` state. `App::run` starts around `:759`; startup, replay, and model migration occupy `:793-1008`; the main asynchronous event loop is around `:1169-1231`; shutdown and exit data are around `:1233-1266`; rendering/event translation follows at `:1269-1365`.

The app coordinates:

- `ChatWidget`
- configuration and model state
- state DB and rollout/session history
- app-server target and events
- file search
- transcript/history
- overlays and keymap
- telemetry
- pending updates
- Windows sandbox onboarding/status
- side threads/multi-agent state

### 8.3 Chat surface

`tui/src/chatwidget.rs` is the primary interactive surface. Its module tree includes bottom pane/composer, streaming, transcript, permissions/approvals, review, safety buffering, plugins, skills, hooks, model/reasoning popups, settings, Windows sandbox UX, status/title, and turn lifecycle. The `ChatWidget` state begins around `:517`; submit logic is around `:1807-1829`; live-tail/overlay cache behavior is around `:1915-1987`.

A Phase 1 rebrand should alter strings and snapshots only. It should not restructure `App`, `ChatWidget`, the event loop, transcript model, or terminal behavior.

---

## 9. CI workflows and Windows support

### 9.1 Main CI

- `.github/workflows/blocking-ci.yml:1-80` aggregates Bazel, blob size, cargo-deny, codespell, repository checks, Rust checks, and SDK checks.
- `.github/workflows/rust-ci.yml:1-269` provides changed-path gating, formatting/bench smoke, cargo-shear, argument-comment lint across Linux/macOS/Windows, and an aggregate result.
- `.github/workflows/rust-ci-full.yml` contains full nextest matrices including Windows x64 and ARM64 MSVC; postmerge calls it from `postmerge-ci.yml`.
- `.github/workflows/bazel.yml` includes Windows test shards, native-main, clippy, and release-build matrices.
- `.github/workflows/rust-release-windows.yml` builds x64/ARM64 MSVC release bundles, stages PDBs, signs binaries, packages helpers, and builds Python runtime wheels; `rust-release.yml` invokes it.

### 9.2 Fork-specific CI risks

CI and release definitions contain OpenAI assumptions:

- Hard-coded upstream workflow/action URLs and package/release names.
- Private secrets, signing infrastructure, runners, and BuildBuddy dependencies unavailable to the fork.
- Fork fallback paths that may skip work or behave differently when secrets are absent.
- `repo-checks.yml` includes a hard-coded Codex version/package staging flow.

Phase 1 should not claim green-equivalent CI until every required workflow is classified as: usable unchanged, fork-configured, intentionally disabled, or replaced.

### 9.3 Native Windows details

The CLI maps Windows to the Windows command sandbox (`cli/src/main.rs:421-426,1438-1488`). `windows-sandbox-rs` implements restricted tokens, ACLs, UAC setup, firewall/WFP, ConPTY/process execution, helper materialization, and cancellation/timeout behavior. Setup uses versioned markers and sandbox users and excludes sensitive profile directories.

This is a security-critical subsystem. Branding must not alter sandbox identities, helper dispatch, ACL behavior, setup markers, or path normalization. Any future functional change requires dedicated security review and x64/ARM64 testing.

---

## 10. Licensing and attribution requirements

The repository is Apache License 2.0 (`LICENSE`). Redistribution obligations relevant to the fork include:

- Provide recipients a copy of the license (`LICENSE:89-96`).
- Add prominent notices to modified files stating that they were changed (`:97-99`).
- Retain applicable copyright, patent, trademark, and attribution notices (`:100-104`).
- Preserve a readable copy of applicable NOTICE attributions (`:106-121`).
- Do not imply trademark permission or OpenAI endorsement; Apache 2.0 does not grant trademark rights beyond customary origin/NOTICE use (`:138-141`).

`NOTICE:1-6` currently preserves OpenAI Codex copyright and Ratatui-derived-code attribution, including Florian Dehau and Ratatui Developers copyright notices. These must remain. Syndrid may add its own derivative-work attribution alongside, not replace upstream attribution.

Third-party license surfaces include `third_party/wezterm/LICENSE`, `codex-rs/vendor/bubblewrap/LICENSE`, sample skill licenses, and other vendored/pinned dependencies. Release packaging must continue to include required notices.

The upstream contribution policy and CLA in `docs/contributing.md:3-27,80-93` are governance text, not automatically Syndrid policy. They should be replaced or clearly qualified only after Syndrid decides its own contribution/CLA process. Security contact and OpenAI-specific governance text also need an ownership review.

---

## 11. Long-lived fork risks

1. **Upstream merge conflict volume.** The Rust workspace is large and high-churn; `cli`, `core`, `tui`, config, app-server protocol, provider routing, and packaging are tightly coupled. Broad internal renames would make every upstream sync harder.
2. **Update-channel self-overwrite.** Current startup update checks and actions point to upstream GitHub, npm, Homebrew, and OpenAI standalone installers. Syndrid users could be prompted to install Codex over the fork.
3. **Protocol/config drift.** Renaming serialized fields, auth values, model IDs, MCP keys, or storage paths would silently fragment compatibility and user state.
4. **Authentication coupling.** OAuth/JWT/account/workspace behavior is OpenAI/ChatGPT-specific. Display branding must not misrepresent the identity provider or break token refresh/restrictions.
5. **Windows release burden.** Native Windows uses x64/ARM64 builds, two privileged/security-sensitive helpers, signing, PDB staging, UAC, WFP, ACLs, and strict package layout. Documentation currently emphasizes WSL2, creating a support mismatch.
6. **CI infrastructure dependence.** Some upstream checks rely on OpenAI secrets, runners, signing, BuildBuddy, action URLs, and release conventions unavailable to the fork.
7. **Trademark and attribution.** The fork must preserve Apache/NOTICE obligations while avoiding language that suggests OpenAI sponsorship or that Syndrid owns upstream code.
8. **Package ecosystem collisions.** `@openai/codex`, Homebrew `codex`, DotSlash entries, archive names, and updater detection are externally consumed contracts. A partial rename can install or launch the wrong binary.
9. **Version/update parser coupling.** Human-facing version output is parsed by managed-install code; packaging and updater protocols depend on command names and directory layouts.
10. **Security regression during merges.** Sandbox, process execution, auth storage, secrets, MCP OAuth, and provider headers are sensitive. Upstream security fixes must be tracked and merged quickly, with dedicated ownership.
11. **Model-catalog drift.** Model metadata, effort presets, image/tool capabilities, fallback rules, and first-party routing evolve upstream. Forked routing logic can become incompatible with service behavior.
12. **Observability/data-flow ambiguity.** OpenAI endpoints and telemetry assumptions are embedded in the product. Syndrid needs an explicit, documented policy for which network services remain upstream and which are fork-owned.

Recommended fork discipline:

- Maintain an `upstream` remote and record the upstream base for each release.
- Rebase/merge frequently in small batches instead of occasional large syncs.
- Keep branding changes shallow and isolated.
- Add compatibility tests for config, auth storage, protocol schemas, updater isolation, and command aliases.
- Require security review for sandbox/auth/provider changes.
- Maintain a written endpoint/package/update inventory.

---

## 12. Proposed Phase 1 implementation plan

Each commit should build independently and avoid mixing compatibility changes with visual branding.

### Commit 1 — `docs: record fork identity, attribution, and compatibility boundary`

- Add SyndridCLI fork statement and upstream attribution.
- Preserve `LICENSE` and existing `NOTICE`; add Syndrid derivative-work notice if counsel/maintainers approve wording.
- Document which Codex identifiers remain intentionally compatible.
- Clarify that authentication may still use OpenAI/ChatGPT.

### Commit 2 — `test: lock current command, version, storage, and updater contracts`

- Add characterization tests for current `--help` and `--version` parsing.
- Add assertions for `CODEX_HOME`, `auth.json`, config paths, protocol/MCP keys, and helper aliases.
- Add tests proving fork builds do not invoke an unintended update channel once Commit 5 lands.

### Commit 3 — `build: add syndrid native executable identity`

- Introduce `syndrid` as the primary Cargo/Bazel command.
- Prefer a compatibility `codex` alias/wrapper for Phase 1 rather than breaking scripts immediately.
- Update `just` run/build recipes.
- Do not rename Rust crates, modules, helpers, or internal environment variables.

### Commit 4 — `ui: brand CLI help, version, startup, and TUI as SyndridCLI`

- Change Clap command/usage display.
- Preserve machine-parseable `<product> <semver>` version output.
- Update welcome, status card, onboarding, recovery/error strings, and snapshots.
- Do not redesign layout or event behavior.

### Commit 5 — `update: isolate SyndridCLI from upstream Codex updates`

- Safest initial behavior: disable automatic upstream update prompts/actions for Syndrid-branded builds unless a Syndrid-owned channel is configured.
- Preserve internal updater subcommand/version contracts for compatibility.
- Add explicit tests ensuring no OpenAI Codex installer is offered as a Syndrid update.

### Commit 6 — `package: produce syndrid launchers and release artifacts`

- Update npm command metadata/launcher lookup or create a fork-owned package.
- Update platform archives, DotSlash mappings, stage scripts, installers, and Windows payload layout.
- Retain legacy Codex install detection/migration and helper filenames where required.
- Keep package namespace/repository/download URLs fork-owned and explicit.

### Commit 7 — `ci: make fork checks and release builds self-contained`

- Remove or replace inaccessible OpenAI-private dependencies.
- Update artifact names and smoke tests for `syndrid`.
- Preserve Linux/macOS/Windows x64/ARM64 coverage.
- Separate unsigned development artifacts from signed production releases.

### Commit 8 — `docs: publish Windows build, install, and support matrix`

- Document the exact commands in this audit.
- State whether native Windows is supported, experimental, or CI-only; retain WSL2 guidance accordingly.
- Document helper/security prerequisites and package layout without changing sandbox behavior.

### Commit 9 — `test: add end-to-end branding and compatibility smoke coverage`

- Verify `syndrid --help`, `syndrid --version`, and no-subcommand TUI startup.
- Verify optional `codex` compatibility alias.
- Verify existing auth/config/session state is discovered unchanged.
- Verify npm/installer resolves the Syndrid binary.
- Verify no upstream self-update prompt appears in Syndrid builds.
- Verify Windows bundles contain the primary executable and required unchanged helpers.

### Explicitly deferred beyond Phase 1

- Multi-account support.
- Authentication redesign or new identity providers.
- Provider/model routing changes.
- TUI redesign.
- Internal crate/module-wide rename.
- Migration away from `CODEX_HOME`/`~/.codex`.
- Sandbox behavior changes.

---

## Final recommendation

Treat `SyndridCLI` as a public product shell over a compatibility-preserving Codex core in Phase 1. Add a `syndrid` command and brand the visible CLI/TUI, but retain Codex/OpenAI identifiers wherever they encode persisted state, service identity, protocols, routing, helper dispatch, or updater compatibility. Isolate upstream update behavior before distributing the fork, and require the packaging/CI work to prove that a Windows installation launches `syndrid.exe` while still finding the existing auth, config, sessions, and sandbox helpers.
