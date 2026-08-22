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
    def __init__(self, path: str, needle: str, reason: str) -> None:
        self.path = path
        self.needle = needle
        self.reason = reason


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
        "fork-user: openai-oss-forks",
        "the WinGet publication path still targets an upstream-owned fork account",
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


REQUIRED = [
    Finding(
        ".github/workflows/rust-release.yml",
        'binaries: "codex syndrid',
        "the primary release matrix must continue to build the Syndrid binary",
    ),
    Finding(
        ".github/workflows/rust-release.yml",
        "Create GitHub Release",
        "v0.1 must preserve GitHub Release artifact publication",
    ),
]


def read(root: Path, path: str) -> str:
    full = root / path
    try:
        return full.read_text(encoding="utf-8")
    except FileNotFoundError:
        raise RuntimeError(f"release-contract audit: missing required file: {path}") from None


def audit_release_contract(root: Path) -> dict[str, object]:
    blockers: list[dict[str, str]] = []
    invariants: list[dict[str, str]] = []

    for finding in FORBIDDEN:
        if finding.needle in read(root, finding.path):
            blockers.append(
                {"path": finding.path, "needle": finding.needle, "reason": finding.reason}
            )

    for finding in REQUIRED:
        if finding.needle not in read(root, finding.path):
            invariants.append(
                {"path": finding.path, "needle": finding.needle, "reason": finding.reason}
            )

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
