#!/usr/bin/env python3

from pathlib import Path
import re
import unittest


REPO_ROOT = Path(__file__).resolve().parents[2]
RELEASE_WORKFLOW = REPO_ROOT / ".github/workflows/rust-release.yml"


def release_job_block(workflow: str) -> str | None:
    lines = workflow.splitlines()
    start = None
    for index, line in enumerate(lines):
        if line == "  release:":
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


def canonical_package_smoke_is_bound(workflow: str) -> bool:
    """Return whether release smoke executes the canonical Linux Syndrid package."""
    release_block = release_job_block(workflow)
    if release_block is None:
        return False

    smoke_index = release_block.find("smoke_syndrid_release_binary.py")
    if smoke_index < 0:
        return False

    package_name = "syndrid-package-x86_64-unknown-linux-musl.tar.gz"
    before_smoke = release_block[:smoke_index]
    extraction_pattern = re.compile(
        r"tar\s+-xzf\s+[^\n]*" + re.escape(package_name) + r"(?:\s|\\|$)"
    )
    if extraction_pattern.search(before_smoke) is None:
        return False

    smoke_region = release_block[smoke_index : smoke_index + 1200]
    return (
        re.search(r"bin/syndrid(?:\s|\"|'|$)", smoke_region) is not None
        and "--expect-version" in smoke_region
    )


class SyndridReleaseSmokePackageBindingTest(unittest.TestCase):
    def test_smoke_against_unrelated_standalone_binary_is_not_sufficient(self) -> None:
        workflow = """
name: rust-release
jobs:
  release:
    steps:
      - name: Smoke standalone binary
        run: |
          python3 .github/scripts/smoke_syndrid_release_binary.py \\
            dist/syndrid-x86_64-unknown-linux-musl \\
            --expect-version 0.1.0
      - name: Create GitHub Release
        run: echo publish
"""
        self.assertFalse(canonical_package_smoke_is_bound(workflow))

    def test_package_name_without_extraction_is_not_sufficient(self) -> None:
        workflow = """
name: rust-release
jobs:
  release:
    steps:
      - name: Mention package
        run: echo syndrid-package-x86_64-unknown-linux-musl.tar.gz
      - name: Smoke standalone binary
        run: |
          python3 .github/scripts/smoke_syndrid_release_binary.py \\
            dist/bin/syndrid \\
            --expect-version 0.1.0
      - name: Create GitHub Release
        run: echo publish
"""
        self.assertFalse(canonical_package_smoke_is_bound(workflow))

    def test_canonical_package_entrypoint_smoke_is_accepted(self) -> None:
        workflow = """
name: rust-release
jobs:
  release:
    steps:
      - name: Extract canonical Syndrid package for smoke
        run: |
          mkdir -p "${RUNNER_TEMP}/syndrid-smoke"
          tar -xzf dist/syndrid-package-x86_64-unknown-linux-musl.tar.gz \\
            -C "${RUNNER_TEMP}/syndrid-smoke"
      - name: Smoke canonical Syndrid package entrypoint
        run: |
          python3 .github/scripts/smoke_syndrid_release_binary.py \\
            "${RUNNER_TEMP}/syndrid-smoke/bin/syndrid" \\
            --expect-version 0.1.0
      - name: Create GitHub Release
        run: echo publish
"""
        self.assertTrue(canonical_package_smoke_is_bound(workflow))

    def test_live_workflow_uses_canonical_package_once_smoke_is_wired(self) -> None:
        workflow = RELEASE_WORKFLOW.read_text(encoding="utf-8")
        if "smoke_syndrid_release_binary.py" not in workflow:
            self.skipTest("production Syndrid smoke is not wired yet")
        self.assertTrue(
            canonical_package_smoke_is_bound(workflow),
            "release smoke must extract syndrid-package-x86_64-unknown-linux-musl.tar.gz and run --help/--version against its bin/syndrid entrypoint",
        )


if __name__ == "__main__":
    unittest.main()
