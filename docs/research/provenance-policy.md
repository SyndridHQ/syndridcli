# SyndridCLI External Feature Provenance Policy

**Effective for:** Phase 3 planning and all later externally inspired work
**Policy date:** 2026-07-14

## 1. Purpose

SyndridCLI is an Apache-2.0 fork of OpenAI Codex. It must preserve upstream attribution and license obligations while ensuring that new features have lawful, auditable provenance. Public repository visibility, popularity, a top-level permissive license, or an “educational” disclaimer is not sufficient evidence that every file, asset, prompt, skill, plugin, dependency, or generated artifact may be reused.

This policy governs:

- source code and generated code;
- documentation and prompts;
- UI text, screenshots, icons, fonts, themes, and other assets;
- tests, fixtures, snapshots, schemas, and examples;
- skills, plugins, hooks, MCP catalogs, and extension bundles;
- vendored or copied dependencies;
- architecture and feature ideas learned from external projects;
- contributions from people who have viewed provenance-restricted material.

This policy is an engineering governance document, not legal advice. Uncertainty must be escalated rather than resolved by optimistic interpretation.

## 2. Mandatory classifications

Every externally inspired feature or imported artifact must receive one of these classifications before implementation or merge.

### `SAFE_TO_REUSE_WITH_ATTRIBUTION`

Use only when:

- the exact file or artifact has an identifiable license that permits the intended use;
- the source has a credible chain of provenance;
- attribution, NOTICE, source-offer, copyleft, patent, and modification-marking obligations are understood;
- bundled dependencies/assets have been reviewed separately;
- no trademark, confidential-source, export, service-terms, or contributor-rights issue remains.

This classification permits only the reviewed artifact and use. It does not clear neighboring files or the repository as a whole.

### `SAFE_TO_REIMPLEMENT`

Use for generic product behavior, architecture patterns, algorithms, standards, or public APIs that can be independently designed and implemented without copying protected expression.

Requirements:

- write a Syndrid-specific requirement and design;
- use Syndrid naming, schemas, prompts, UI, tests, and structure;
- cite inspiration and lawful sources;
- preserve Codex compatibility and security boundaries;
- do not translate or mechanically port external code.

### `IDEAS_ONLY_CLEAN_ROOM`

Use when a source has uncertain, disputed, leaked, reconstructed, decompiled, confidential, or unauthorized provenance, but generic publicly observable behavior may still be considered.

Only high-level behavior may pass into the clean-room specification. The following must not pass:

- source code or pseudocode that tracks source structure;
- prompts, system instructions, hidden text, or distinctive strings;
- tests, snapshots, fixtures, error messages, or schemas;
- names of internal types, functions, modules, files, events, or commands;
- screenshots, assets, themes, icons, sounds, or layout measurements;
- source-tree organization, implementation sequencing, or inferred private interfaces.

### `REJECT`

Use when material must not enter the project. This includes:

- leaked or allegedly leaked proprietary source;
- repositories that deny redistribution rights;
- code without a usable license;
- copied credentials, secrets, private keys, personal data, or confidential material;
- malicious or suspicious packages/mirrors;
- assets or code whose provenance cannot be established;
- content offered under terms incompatible with Syndrid distribution;
- any artifact that counsel or maintainers determine creates unacceptable risk.

Rejected material must not be cloned into the repository, copied into notes, installed, executed, vendored, packaged, uploaded, or used as training/reference material for implementation.

### `NEEDS_LEGAL_REVIEW`

Use whenever a competent engineering review cannot resolve:

- copyright ownership or chain of title;
- mixed or conflicting licenses;
- reverse-engineering, DMCA, trade-secret, contract, or confidentiality issues;
- trademark or product-confusion concerns;
- ambiguous generated-code ownership;
- license compatibility or distribution obligations;
- whether prior exposure has contaminated clean-room implementation;
- whether a skill/plugin/asset may be redistributed.

No implementation or merge proceeds until the review is documented.

## 3. Acceptable code reuse

Code reuse is acceptable only after an artifact-level review.

Required checks:

1. Record repository URL, exact commit/tag, file path, retrieval date, and author/copyright notice.
2. Read the operative license text; do not rely only on a GitHub license badge or package metadata.
3. Check file-level SPDX headers and adjacent license/NOTICE files.
4. Determine whether the file is original, generated, vendored, copied, patched, or derived.
5. Identify dependencies and assets required for the reused portion.
6. Confirm compatibility with Apache-2.0 distribution and Syndrid's intended packaging.
7. Preserve required copyright, permission, NOTICE, patent, source, and modification notices.
8. Prefer a small, isolated reuse over importing an entire package.
9. Record why reuse is preferable to independent Rust implementation.
10. Obtain maintainer and legal approval where required.

Direct external code reuse should be exceptional. For architectural inspiration, `SAFE_TO_REIMPLEMENT` is the default.

## 4. Attribution requirements

For every reused artifact:

- retain the full required license text;
- retain applicable copyright notices;
- update `NOTICE` or a third-party notices inventory where required;
- mark modified files when the license requires prominent modification notices;
- include source and version information in the provenance record;
- document significant transformations;
- preserve upstream Apache-2.0 and existing Ratatui/third-party notices already present in SyndridCLI;
- do not imply endorsement, affiliation, sponsorship, or trademark permission.

Attribution in a commit message alone is not sufficient when distribution notices are required.

## 5. Clean-room reimplementation

### 5.1 When clean-room controls are required

Clean-room controls are mandatory for:

- alleged Claude Code leaks, mirrors, reconstructed bundles, source maps, or decompiled material;
- sources whose license disclaims redistribution or acknowledges unauthorized derivation;
- behavior studied from proprietary software beyond ordinary public use/documentation;
- contributors who have direct implementation knowledge from a restricted source.

### 5.2 Clean-room roles

Where feasible, separate:

- **Requirements researchers:** review lawful public behavior and produce a high-level functional specification.
- **Implementers:** receive only the approved specification and current Syndrid architecture constraints.
- **Reviewers:** verify independent design, provenance records, and absence of copied expression.

If role separation is not feasible, the exposed contributor must disclose the exposure and legal/maintainer review must decide whether reassignment, cooling-off, or additional similarity review is necessary.

### 5.3 Clean-room specification rules

A clean-room specification may contain:

- user problem and expected generic outcome;
- inputs, outputs, state transitions, and safety constraints;
- interoperability requirements based on public standards or official APIs;
- acceptance criteria written in original language;
- explicit Syndrid compatibility boundaries.

It must not contain:

- copied or paraphrased proprietary prompts;
- distinctive text, UI sequences, or error messages;
- private/internal names or organization;
- source-derived test cases that reveal implementation;
- code-shaped descriptions that amount to translation.

### 5.4 Implementation evidence

The implementation record must include:

- approved specification revision;
- implementer provenance attestation;
- independently chosen Rust types/modules/names;
- design rationale tied to Syndrid architecture;
- tests derived from Syndrid acceptance criteria;
- similarity/provenance review result;
- legal approval reference when required.

## 6. Prohibited sources

The following are prohibited implementation sources unless the rights holder provides written authorization and legal review approves use:

- `https://github.com/yasasbanukaofficial/claude-code`
- `https://github.com/Gitlawb/openclaude` for all Claude Code-derived material
- `https://github.com/codeaashu/claude-code`
- mirrors, forks, archives, torrents, package caches, source maps, paste sites, screenshots, snippets, or generated reconstructions of the same material
- any source described as leaked, stolen, confidential, internal, reconstructed, decompiled, reverse engineered, or “not for redistribution”
- code received privately without documented authority to license it

For `codeaashu/claude-code` and direct leak/reconstruction mirrors, engineers may retain only minimal provenance records: URL, access date, public license/disclaimer finding, and classification. For the Yasas and OpenClaude repositories, a provenance audit may additionally record high-level product claims only when those claims are independently corroborated by lawful public sources; the repository itself must not supply implementation requirements. Do not mirror source contents into tickets, chat, design docs, or the repository.

## 7. File-level license review

A permissive root license does not clear every file.

Review must cover:

- SPDX headers and per-directory licenses;
- vendored directories and git submodules;
- copied or patched upstream files;
- generated source and checked-in build output;
- examples, fixtures, snapshots, test corpora, and benchmarks;
- documentation copied from vendors;
- fonts, icons, logos, themes, screenshots, audio, images, and data;
- embedded WASM/native binaries;
- release archives and package contents, not only the source tree;
- lockfiles and transitive dependency licenses;
- optional features that change the shipped dependency set.

The reviewer must record whether each item is:

- original project work;
- third-party permissive;
- copyleft or source-requiring;
- proprietary/restricted;
- generated, with generator and input provenance;
- unknown and therefore blocked.

## 8. Generated-code handling

Generated code is not automatically unencumbered.

Before merge, record:

1. Generator name, version, URL, and license.
2. Input schemas/templates/data and their licenses.
3. Command or reproducible process used.
4. Whether generated output contains copied comments, templates, runtime code, or assets.
5. Whether the generator's terms impose attribution or distribution obligations.
6. Whether an AI model generated or transformed the output, including the source material supplied to it.
7. A review that output does not reproduce restricted material.

Generated files should contain a clear generated-file marker when appropriate. Do not hand-edit generated output unless the regeneration/patch policy is documented.

AI-generated code must be treated like any contribution: the submitter is responsible for provenance, license compatibility, security, and originality. “AI generated” is not a license or a defense to copying.

## 9. Prompt and asset provenance

### 9.1 Prompts

Prompts, system instructions, agent profiles, role templates, rubrics, and skill instructions are copyrightable expression in many contexts and can contain confidential product logic.

Requirements:

- write Syndrid prompts from original product requirements;
- record author/source and review date;
- do not copy or paraphrase hidden prompts from proprietary agents;
- do not use prompts from leaked/reconstructed repositories;
- license third-party prompt collections explicitly;
- treat generated prompts as generated-code artifacts;
- review for embedded secrets, personal data, unsafe instructions, and provider-specific confidential material.

### 9.2 Assets and visual identity

Do not reuse external:

- logos, product names, icons, mascots, screenshots, terminal captures, fonts, color systems, themes, animation frames, or sounds;
- distinctive layout measurements or ornamental elements intended as brand identity;
- provider or project marks without permission.

SyndridCLI must use independently created visual identity and original UI text. A source-code license may not grant trademark or asset rights.

## 10. Third-party skill review

A skill is an executable content bundle, not merely documentation.

Before installation, bundling, or distribution, review:

- source URL and pinned revision;
- author and license for every file;
- manifest and compatibility version;
- prompts/instructions and referenced files;
- scripts, binaries, templates, examples, and assets;
- requested tools, network, filesystem paths, MCP access, and credentials;
- install/build/bootstrap commands;
- transitive downloads or package-manager actions;
- update mechanism and integrity verification;
- data sent to model providers or external services;
- conflict with existing skill names or commands.

Required product behavior:

1. Stage the skill outside active search paths.
2. Show a file/license/capability diff before installation.
3. Execute nothing during review.
4. Require explicit approval for requested capabilities.
5. Install atomically with content hashes and provenance manifest.
6. Keep untrusted skills disabled by default.
7. Allow complete removal and revocation.
8. Re-review meaningful updates; never auto-trust a moving branch.

Bundled skills require the same review as source code and must appear in release attribution/SBOM material where applicable.

## 11. Dependency review

Before adding or enabling a dependency:

- identify direct and transitive licenses;
- check source/repository integrity and maintainer history;
- prefer pinned, reproducible versions;
- review build scripts, native code, proc macros, install hooks, and network behavior;
- check vulnerability and malicious-package advisories;
- confirm supported platforms and MSVC/Windows implications;
- assess binary size, startup, performance, and upstream maintenance cost;
- identify optional features that expand code or licenses;
- confirm the dependency does not replace or weaken existing security boundaries;
- update SBOM and third-party notices.

No dependency may be introduced merely to reproduce a TypeScript/Python reference architecture when an existing Rust crate or current Syndrid subsystem is sufficient.

## 12. MCP, plugin, hook, and extension provenance

Extensions may execute code or influence model/tool behavior and therefore require both provenance and capability review.

Minimum requirements:

- versioned manifest and compatibility range;
- source and distribution identity;
- file hashes or signatures;
- declared tools, hooks, commands, prompts, network endpoints, paths, and credential needs;
- trusted/untrusted classification;
- explicit user/project/admin enablement;
- sandbox/process boundary documentation;
- audit logging and revocation;
- no automatic activation solely because files exist in a project;
- no bypass of existing tool executor, approval policy, sandbox, MCP overlays, auth, or managed configuration.

Untrusted extensions must not run in the main Syndrid process. Prefer a constrained subprocess RPC or WASM boundary with resource and capability limits.

## 13. Required documentation before merging an externally inspired feature

Every merge request must include or link to:

### Source register

- feature name;
- external source URLs;
- exact commit/tag/version and access date;
- relevant license URLs and exact findings;
- file-level and asset-level findings;
- classification for each source/artifact.

### Independent design record

- user problem and benefit;
- Syndrid-specific behavior and terminology;
- architecture mapping to current crates/modules;
- compatibility constraints for protocol, auth, storage, models, sandbox, and approvals;
- rejected external behaviors and reasons;
- non-goals.

### Security record

- threat model;
- permission and sandbox interaction;
- credential, memory, log, and telemetry handling;
- extension/MCP/remote-worker trust model where applicable;
- rollback and failure behavior.

### Implementation provenance

- contributor attestation;
- copied files, if any, with approval and attribution;
- generated-code record;
- dependency/SBOM updates;
- prompt/asset authorship record;
- clean-room evidence where applicable.

### Validation record

- tests and acceptance criteria;
- backward-compatibility checks;
- sandbox/approval security tests;
- license/NOTICE checks;
- `git diff --check` and repository-status review;
- legal approval reference for `NEEDS_LEGAL_REVIEW` items.

A merge must be blocked if any required record is missing.

## 14. Review workflow

1. **Discover:** Record source and initial provenance warning before deep inspection.
2. **Classify:** Assign one of the five mandatory classifications.
3. **Minimize:** Prefer high-level behavior research and independent reimplementation.
4. **Specify:** Write original Syndrid requirements and non-goals.
5. **Design:** Map to existing Rust boundaries and compatibility rules.
6. **Implement:** Use approved sources only; preserve attribution.
7. **Verify:** Run functional, security, compatibility, and similarity review.
8. **Document:** Update source register, notices, SBOM, and decision record.
9. **Approve:** Obtain maintainer/security/legal sign-off as required.
10. **Monitor:** Revisit provenance when dependencies, upstream licenses, or external claims change.

## 15. Repository-specific determinations from the Phase 3 audit

| Repository | Determination |
|---|---|
| `open-multi-agent/open-multi-agent` | MIT at root. Concepts are `SAFE_TO_REIMPLEMENT`. Direct code is potentially `SAFE_TO_REUSE_WITH_ATTRIBUTION` after exact file/dependency/asset review. |
| `yasasbanukaofficial/claude-code` | No visible reuse license and alleged proprietary recovered material. Code/assets/prompts are `REJECT`; generic behavior is `IDEAS_ONLY_CLEAN_ROOM`; direct exposure-to-implementation requires `NEEDS_LEGAL_REVIEW`. |
| `Gitlawb/openclaude` | Mixed/restricted provenance; MIT does not clear acknowledged Claude Code-derived material. Implementation is `REJECT`; only generic behavior is `IDEAS_ONLY_CLEAN_ROOM`; legal review required for any proposed use. |
| `codeaashu/claude-code` | Explicit leaked-source and not-for-redistribution posture. Entire implementation is `REJECT`; provenance notices only may be retained. |
| `NousResearch/hermes-agent` | MIT at root. Concepts are `SAFE_TO_REIMPLEMENT`; direct files require attribution and exact review; bundled skills/plugins/catalogs/assets require separate review. |
| `anomalyco/opencode` | MIT at root. Concepts are `SAFE_TO_REIMPLEMENT`; exact code requires attribution/file review; marks, screenshots, assets, native dependencies, and plugins are separately controlled. |

## 16. Enforcement and exception process

- Maintainers may block a contribution solely for incomplete provenance.
- Security reviewers may require isolation or reject an extension even when its license is permissive.
- Legal review is mandatory where this policy says `NEEDS_LEGAL_REVIEW`.
- Exceptions must be written, narrowly scoped, time-bounded where appropriate, and approved by designated maintainers and counsel.
- An exception for one artifact or version does not apply to later versions, forks, bundled assets, or adjacent files.
- If a provenance determination is later found wrong, stop distribution/use, remove or replace the material, update notices, and document remediation.

## 17. Contributor attestation template

Every externally inspired contribution should include a statement equivalent to:

> I created this contribution from the documented SyndridCLI requirements and approved sources. I did not copy or translate code, prompts, assets, tests, distinctive strings, or source structure from provenance-restricted repositories. All third-party material is listed with its exact source, version, license, and required attribution. I disclosed any prior exposure that could affect clean-room independence.

## 18. Final rule

When provenance is unclear, **do not import first and investigate later**. Stop, classify the material as `NEEDS_LEGAL_REVIEW` or `REJECT`, and proceed through an independent Rust implementation using lawful public sources and SyndridCLI's existing architecture.
