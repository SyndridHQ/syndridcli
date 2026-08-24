#!/usr/bin/env python3
"""Audit SyndridCLI release inputs before a Syndrid-owned tag is published.

This script intentionally reports inherited upstream publication assumptions as blockers.
It does not mutate the repository, publish packages, or infer replacement credentials/scopes.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import sys

DEFAULT_ROOT = Path(__file__).resolve().parents[2]


class Finding:
    def __init__(
        self,
        path: str,
        needle: str,
        reason: str,
        minimum_count: int = 1,
    ) -> None:
        self.path = path
        self.needle = needle
        self.reason = reason
        self.minimum_count = minimum_count


FORBIDDEN = [
    Finding(
        ".github/workflows/rust-release-prepare.yml",
        "github.repository == 'openai/codex'",
        "release preparation is still hard-gated to the upstream repository",
    ),
    Finding(
        ".github/workflows/rust-release.yml",
        'scope: "@openai"',
        "npm publication is still configured for the OpenAI scope",
    ),
    Finding(
        ".github/workflows/rust-release.yml",
        "npm publish",
        "the tag workflow still performs a direct npm publication before a Syndrid-owned package identity and publication authority are established",
    ),
    Finding(
        ".github/workflows/rust-release.yml",
        "developers.openai.com",
        "the tag workflow still targets the upstream developer website",
    ),
    Finding(
        ".github/workflows/rust-release.yml",
        "identifier: OpenAI.Codex",
        "the tag workflow still targets the upstream WinGet package identity",
    ),
    Finding(
        ".github/workflows/rust-release.yml",
        "microsoft/winget-pkgs",
        "the tag workflow still contains a direct WinGet submission path before a Syndrid-owned package identity is established",
    ),
    Finding(
        ".github/workflows/rust-release.yml",
        "https://github.com/openai/codex/releases/",
        "the WinGet publication path still embeds upstream GitHub release URLs",
    ),
    Finding(
        ".github/workflows/rust-release.yml",
        "fork-user: openai-oss-forks",
        "the WinGet publication path still targets an upstream-owned fork account",
    ),
    Finding(
        ".github/workflows/rust-release.yml",
        "git push origin HEAD:main",
        "the tag workflow still contains a direct external documentation push before the Syndrid documentation target is validated",
    ),
    Finding(
        ".github/workflows/rust-release.yml",
        "name: codesigning",
        "the release graph still requires an inherited protected signing environment before GitHub artifact publication",
    ),
    Finding(
        ".github/workflows/rust-release.yml",
        "overwrite_files: true",
        "the GitHub Release step still permits an existing tag's assets to be overwritten on rerun; v0.1 release artifacts must be immutable once published",
    ),
    Finding(
        "codex-cli/package.json",
        '"name": "@openai/codex"',
        "the npm wrapper still has the upstream package identity",
    ),
    Finding(
        "codex-cli/package.json",
        "https://github.com/openai/codex.git",
        "the npm wrapper still points at the upstream repository",
    ),
    Finding(
        "scripts/install/install.sh",
        "openai/codex",
        "the Unix installer still resolves releases from the upstream repository",
    ),
    Finding(
        "scripts/install/install.sh",
        'BIN_PATH="$BIN_DIR/codex"',
        "the Unix installer still exposes codex as its primary installed executable instead of the canonical Syndrid entrypoint",
    ),
    Finding(
        "scripts/install/install.sh",
        'package_asset="codex-package-$vendor_target.tar.gz"',
        "the Unix installer still selects the canonical Codex package archive instead of syndrid-package for the resolved target",
    ),
    Finding(
        "scripts/install/install.sh",
        'checksum_asset="codex-package_SHA256SUMS"',
        "the Unix installer still verifies canonical packages through the Codex checksum manifest instead of the Syndrid-owned manifest",
    ),
    Finding(
        "scripts/install/install.ps1",
        "openai/codex",
        "the Windows installer still resolves releases from the upstream repository",
    ),
    Finding(
        "scripts/install/install.ps1",
        'Join-Path $StandaloneCurrentDir "bin\\codex.exe"',
        "the Windows installer still treats codex.exe as the canonical installed entrypoint instead of syndrid.exe",
    ),
    Finding(
        "scripts/install/install.ps1",
        '$packageAsset = "codex-package-$target.tar.gz"',
        "the Windows installer still selects the canonical Codex package archive instead of syndrid-package for the resolved target",
    ),
    Finding(
        "scripts/install/install.ps1",
        '$checksumAsset = "codex-package_SHA256SUMS"',
        "the Windows installer still verifies canonical packages through the Codex checksum manifest instead of the Syndrid-owned manifest",
    ),
]


TAG_SIDE_EFFECTS = [
    Finding(
        ".github/workflows/rust-release.yml",
        "publish-dotslash:",
        "the tag workflow still publishes DotSlash metadata whose current output contract is Codex-only; explicitly migrate or disable this side effect before a Syndrid v0.1 tag",
    ),
    Finding(
        ".github/workflows/rust-release.yml",
        "repos/${GITHUB_REPOSITORY}/git/refs/heads/latest-alpha-cli",
        "the tag workflow still force-updates the inherited latest-alpha-cli branch; explicitly accept, rename, or disable this moving-ref side effect before a Syndrid v0.1 tag",
    ),
]


PACKAGE_INPUT_BLOCKERS = [
    Finding(
        ".github/workflows/rust-release.yml",
        "CODEX_ZSH_RELEASE_TAG: codex-zsh-",
        "canonical package construction still consumes an inherited Codex shell-manifest release identity; migrate it to a Syndrid-owned input or explicitly validate and accept that compatibility dependency before tagging v0.1",
    ),
]


MACOS_DISTRIBUTION_BLOCKERS = [
    Finding(
        ".github/workflows/rust-release.yml",
        'volname="Codex (${target})"',
        "the primary macOS disk image still presents the inherited Codex volume identity; migrate or explicitly retire that distribution artifact before a Syndrid v0.1 tag",
    ),
    Finding(
        ".github/workflows/rust-release.yml",
        'dmg_path="${release_dir}/codex-${target}.dmg"',
        "the primary macOS disk image is still published under the inherited codex-<target>.dmg asset identity instead of a Syndrid-owned release artifact name",
    ),
]


REQUIRED = [
    Finding(
        ".github/workflows/rust-release.yml",
        'binaries: "codex syndrid',
        "the primary release matrix must continue to build the Syndrid binary",
    ),
    Finding(
        ".github/workflows/rust-release.yml",
        "--bundle syndrid",
        "the non-Windows tag workflow must build canonical syndrid-package archives in both the ordinary non-macOS producer path and the post-sign macOS packaging path",
        minimum_count=2,
    ),
    Finding(
        ".github/workflows/rust-release-windows.yml",
        "--bundle syndrid",
        "the Windows tag workflow must build a canonical syndrid-package archive from the signed Windows binaries",
    ),
    Finding(
        ".github/workflows/rust-release.yml",
        'verify_signed_binary "${package_dir}/bin/syndrid" "syndrid"',
        "post-sign macOS verification must inspect the canonical Syndrid package entrypoint rather than validating only the Codex package",
    ),
    Finding(
        ".github/workflows/rust-release.yml",
        "syndrid-package-*.tar.gz",
        "the GitHub Release checksum manifest must include canonical Syndrid gzip package archives",
    ),
    Finding(
        ".github/workflows/rust-release.yml",
        "Create GitHub Release",
        "v0.1 must preserve GitHub Release artifact publication",
    ),
    Finding(
        ".github/workflows/rust-release.yml",
        "files: dist/**",
        "the GitHub Release step must attach the staged dist artifacts; creating a release record without uploading the staged Syndrid packages is not a valid v0.1 release",
    ),
]

ZSTD_CHECKSUM_REQUIRED = Finding(
    ".github/workflows/rust-release.yml",
    "syndrid-package-*.tar.zst",
    "the GitHub Release checksum manifest must include canonical Syndrid zstd package archives because the canonical producer publishes both archive forms",
)

SYNDRID_CHECKSUM_MANIFEST_REQUIRED = Finding(
    ".github/workflows/rust-release.yml",
    "dist/syndrid-package_SHA256SUMS",
    "the canonical Syndrid package checksums must be published under a Syndrid-owned manifest name rather than inheriting codex-package_SHA256SUMS",
)

TAG_PROVENANCE_REQUIRED = [
    Finding(
        ".github/workflows/rust-release.yml",
        "git fetch --no-tags origin main",
        "the tag gate must fetch main history before validating release provenance; a detached tag checkout alone cannot prove that the tagged commit belongs to the protected release lineage",
    ),
    Finding(
        ".github/workflows/rust-release.yml",
        'git merge-base --is-ancestor "${GITHUB_SHA}" "origin/main"',
        "the tag gate must prove the tagged commit is contained in main history before any release build or publication is allowed",
    ),
]

RELEASE_CONCURRENCY_GROUP_REQUIRED = Finding(
    ".github/workflows/rust-release.yml",
    "tag-scoped release concurrency",
    "the release workflow concurrency group must include github.ref or github.ref_name so distinct release tags cannot share one cancellation domain",
)

RELEASE_CONCURRENCY_CANCEL_REQUIRED = Finding(
    ".github/workflows/rust-release.yml",
    "cancel-in-progress: false",
    "release concurrency must be non-cancelling so a later tag cannot interrupt an earlier release after signing or publication work begins",
)

RELEASE_PUBLICATION_DEPENDENCIES = (
    "tag-check",
    "build",
    "finalize-macos",
    "build-windows",
    "argument-comment-lint-release-assets",
)

AUDIT_REQUIRED = Finding(
    ".github/workflows/rust-release.yml",
    "check_syndrid_release_contract.py",
    "the tag workflow must execute the release-contract audit before publication; a checked-in audit that is never invoked cannot gate an unsafe tag",
)

SMOKE_REQUIRED = Finding(
    ".github/workflows/rust-release.yml",
    "smoke_syndrid_release_binary.py",
    "the tag workflow must execute side-effect-minimal --help/--version smoke checks against a staged Syndrid release binary before publication",
)

SMOKE_VERSION_REQUIRED = Finding(
    ".github/workflows/rust-release.yml",
    "--expect-version",
    "the staged Syndrid --version smoke must bind to the intended release version; accepting any semantic version can publish stale or mismatched binary bytes",
)


def read(root: Path, path: str) -> str:
    full = root / path
    try:
        return full.read_text(encoding="utf-8")
    except FileNotFoundError:
        raise RuntimeError(f"release-contract audit: missing required file: {path}") from None


def append_invariant(invariants: list[dict[str, str]], finding: Finding) -> None:
    invariants.append(
        {"path": finding.path, "needle": finding.needle, "reason": finding.reason}
    )


def is_structured_release_workflow(release_workflow: str) -> bool:
    """Return whether this is the real tag workflow rather than a unit-test fragment."""
    return "name: rust-release" in release_workflow and re.search(
        r"(?m)^jobs:\s*$", release_workflow
    ) is not None


def has_tag_scoped_release_concurrency(release_workflow: str) -> bool:
    """Return whether the top-level release concurrency group is scoped by tag ref."""
    concurrency_match = re.search(
        r"(?ms)^concurrency:\s*\n(?P<body>(?:^[ \t]+.*\n?)*)",
        release_workflow,
    )
    if concurrency_match is None:
        return False
    body = concurrency_match.group("body")
    group_match = re.search(r"(?m)^\s*group:\s*(?P<group>.+?)\s*$", body)
    if group_match is None:
        return False
    group = group_match.group("group")
    return "github.ref" in group or "github.ref_name" in group


def has_non_cancelling_release_concurrency(release_workflow: str) -> bool:
    concurrency_match = re.search(
        r"(?ms)^concurrency:\s*\n(?P<body>(?:^[ \t]+.*\n?)*)",
        release_workflow,
    )
    if concurrency_match is None:
        return False
    return re.search(
        r"(?m)^\s*cancel-in-progress:\s*false\s*(?:#.*)?$",
        concurrency_match.group("body"),
    ) is not None


def workflow_job_block(workflow: str, job_name: str) -> str | None:
    lines = workflow.splitlines()
    start = None
    target = f"  {job_name}:"
    for index, line in enumerate(lines):
        if line == target:
            start = index
            break
    if start is None:
        return None

    end = len(lines)
    for index in range(start + 1, len(lines)):
        if re.fullmatch(r"  [A-Za-z0-9_-]+:\s*", lines[index]):
            end = index
            break
    return "\n".join(lines[start:end])


def append_release_publication_dependency_invariants(
    invariants: list[dict[str, str]], release_workflow: str
) -> None:
    release_block = workflow_job_block(release_workflow, "release")
    if release_block is None:
        append_invariant(
            invariants,
            Finding(
                ".github/workflows/rust-release.yml",
                "release:",
                "the tag workflow must retain a dedicated publication job downstream of validated release producers",
            ),
        )
        return

    needs = set(
        re.findall(r"(?m)^      - ([A-Za-z0-9_-]+)\s*$", release_block)
    )
    for dependency in RELEASE_PUBLICATION_DEPENDENCIES:
        if dependency not in needs:
            append_invariant(
                invariants,
                Finding(
                    ".github/workflows/rust-release.yml",
                    f"release.needs:{dependency}",
                    f"GitHub Release publication must remain structurally downstream of {dependency}",
                ),
            )

        success_predicate = f"needs.{dependency}.result == 'success'"
        if success_predicate not in release_block:
            append_invariant(
                invariants,
                Finding(
                    ".github/workflows/rust-release.yml",
                    success_predicate,
                    f"GitHub Release publication must require {dependency} to succeed rather than merely waiting for it",
                ),
            )


def canonical_syndrid_producer_emits_zstd(root: Path) -> bool:
    archive_helper = root / ".github/scripts/build-codex-package-archive.sh"
    if not archive_helper.is_file():
        return False
    content = archive_helper.read_text(encoding="utf-8")
    return (
        'archive_stem="syndrid-package"' in content
        and 'zstd_archive_path="${archive_dir}/${archive_stem}-${target}.tar.zst"' in content
        and '--archive-output "$zstd_archive_path"' in content
    )


def audit_release_contract(root: Path) -> dict[str, object]:
    blockers: list[dict[str, str]] = []
    invariants: list[dict[str, str]] = []

    for finding in [
        *FORBIDDEN,
        *TAG_SIDE_EFFECTS,
        *PACKAGE_INPUT_BLOCKERS,
        *MACOS_DISTRIBUTION_BLOCKERS,
    ]:
        if finding.needle in read(root, finding.path):
            blockers.append(
                {"path": finding.path, "needle": finding.needle, "reason": finding.reason}
            )

    release_workflow = read(root, ".github/workflows/rust-release.yml")
    required = list(REQUIRED)
    if canonical_syndrid_producer_emits_zstd(root):
        required.extend([ZSTD_CHECKSUM_REQUIRED, SYNDRID_CHECKSUM_MANIFEST_REQUIRED])
    if "tag-check:" in release_workflow:
        required.extend(TAG_PROVENANCE_REQUIRED)

    if is_structured_release_workflow(release_workflow):
        if not has_tag_scoped_release_concurrency(release_workflow):
            append_invariant(invariants, RELEASE_CONCURRENCY_GROUP_REQUIRED)
        if not has_non_cancelling_release_concurrency(release_workflow):
            append_invariant(invariants, RELEASE_CONCURRENCY_CANCEL_REQUIRED)
        append_release_publication_dependency_invariants(invariants, release_workflow)

    audit_present = (root / ".github/scripts/check_syndrid_release_contract.py").is_file()
    smoke_present = (root / ".github/scripts/smoke_syndrid_release_binary.py").is_file()
    if audit_present:
        required.append(AUDIT_REQUIRED)
    if smoke_present:
        required.extend([SMOKE_REQUIRED, SMOKE_VERSION_REQUIRED])

    for finding in required:
        if read(root, finding.path).count(finding.needle) < finding.minimum_count:
            append_invariant(invariants, finding)

    publish_index = release_workflow.find("Create GitHub Release")
    if publish_index >= 0:
        for present, finding in (
            (audit_present, AUDIT_REQUIRED),
            (smoke_present, SMOKE_REQUIRED),
            (smoke_present, SMOKE_VERSION_REQUIRED),
        ):
            if not present:
                continue
            check_index = release_workflow.find(finding.needle)
            if check_index >= 0 and check_index > publish_index:
                append_invariant(invariants, finding)

    return {
        "ok": not blockers and not invariants,
        "blockers": blockers,
        "missing_required_invariants": invariants,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Audit SyndridCLI pre-tag release contract.")
    parser.add_argument(
        "--root",
        type=Path,
        default=DEFAULT_ROOT,
        help=argparse.SUPPRESS,
    )
    args = parser.parse_args()

    try:
        result = audit_release_contract(args.root.resolve())
    except RuntimeError as exc:
        print(str(exc), file=sys.stderr)
        return 1

    print(json.dumps(result, indent=2, sort_keys=True))

    blockers = result["blockers"]
    invariants = result["missing_required_invariants"]
    if blockers:
        print(
            "Syndrid release contract is not safe for tagging: inherited upstream "
            "publication identities/channels remain.",
            file=sys.stderr,
        )
    if invariants:
        print(
            "Syndrid release contract is missing required GitHub artifact/Syndrid-binary invariants.",
            file=sys.stderr,
        )
    return 1 if blockers or invariants else 0


if __name__ == "__main__":
    raise SystemExit(main())
