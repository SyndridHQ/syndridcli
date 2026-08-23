#!/usr/bin/env python3
"""Audit SyndridCLI release inputs before a Syndrid-owned tag is published.

This script intentionally reports inherited upstream publication assumptions as blockers.
It does not mutate the repository, publish packages, or infer replacement credentials/scopes.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
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
        "scripts/install/install.ps1",
        "openai/codex",
        "the Windows installer still resolves releases from the upstream repository",
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
    Finding(
        ".github/workflows/rust-release.yml",
        "group: ${{ github.workflow }}\n  cancel-in-progress: true",
        "the release workflow can cancel an in-progress tag release when another tag is pushed; use tag-scoped non-cancelling release concurrency so one release cannot interrupt another after signing or publication has started",
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
        "the GitHub Release checksum manifest must include canonical Syndrid package archives",
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


def audit_release_contract(root: Path) -> dict[str, object]:
    blockers: list[dict[str, str]] = []
    invariants: list[dict[str, str]] = []

    for finding in [*FORBIDDEN, *TAG_SIDE_EFFECTS]:
        if finding.needle in read(root, finding.path):
            blockers.append(
                {"path": finding.path, "needle": finding.needle, "reason": finding.reason}
            )

    release_workflow = read(root, ".github/workflows/rust-release.yml")
    required = list(REQUIRED)
    if "tag-check:" in release_workflow:
        required.extend(TAG_PROVENANCE_REQUIRED)

    audit_present = (root / ".github/scripts/check_syndrid_release_contract.py").is_file()
    smoke_present = (root / ".github/scripts/smoke_syndrid_release_binary.py").is_file()
    if audit_present:
        required.append(AUDIT_REQUIRED)
    if smoke_present:
        required.append(SMOKE_REQUIRED)

    for finding in required:
        if read(root, finding.path).count(finding.needle) < finding.minimum_count:
            append_invariant(invariants, finding)

    publish_index = release_workflow.find("Create GitHub Release")
    if publish_index >= 0:
        for present, finding in (
            (audit_present, AUDIT_REQUIRED),
            (smoke_present, SMOKE_REQUIRED),
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
    parser = argparse.ArgumentParser(description="Audit the SyndridCLI pre-tag release contract.")
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
