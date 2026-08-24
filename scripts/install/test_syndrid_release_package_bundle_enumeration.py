from __future__ import annotations

from pathlib import Path
import re
import unittest

REPO_ROOT = Path(__file__).resolve().parents[2]
RELEASE_WORKFLOW = REPO_ROOT / ".github/workflows/rust-release.yml"
WINDOWS_RELEASE_WORKFLOW = REPO_ROOT / ".github/workflows/rust-release-windows.yml"
ARCHIVE_HELPER = REPO_ROOT / ".github/scripts/build-codex-package-archive.sh"


def workflow_job_block(workflow: str, job_name: str) -> str | None:
    lines = workflow.splitlines()
    target = f"  {job_name}:"
    try:
        start = lines.index(target)
    except ValueError:
        return None

    end = len(lines)
    for index in range(start + 1, len(lines)):
        if re.fullmatch(r"  [A-Za-z0-9_-]+:\s*", lines[index]):
            end = index
            break
    return "\n".join(lines[start:end])


def job_builds_syndrid_bundle(workflow: str, job_name: str) -> bool:
    """Accept literal or dynamically enumerated canonical Syndrid package builds."""
    job = workflow_job_block(workflow, job_name)
    if job is None:
        return False

    if re.search(r"--bundle(?:=|\s+)[\"']?syndrid[\"']?(?:\s|\\|$)", job):
        return True

    # The Windows release job already builds canonical variants by enumerating
    # bundle names and passing each one through `--bundle "{}"`. A Syndrid
    # migration should extend that producer set instead of needing a special
    # one-off command solely to satisfy an audit substring.
    dynamic_bundle = re.search(
        r"printf\s+['\"]%s\\0['\"]\s+(?P<bundles>[^\n|]+)\|(?P<pipeline>.*?--bundle\s+[\"']?\{\}[\"']?)",
        job,
        flags=re.DOTALL,
    )
    if dynamic_bundle is None:
        return False

    bundles = re.findall(r"[A-Za-z0-9_-]+", dynamic_bundle.group("bundles"))
    return "syndrid" in bundles


def canonical_syndrid_producer_exists() -> bool:
    if not ARCHIVE_HELPER.is_file():
        return False
    helper = ARCHIVE_HELPER.read_text(encoding="utf-8")
    return 'archive_stem="syndrid-package"' in helper


class SyndridReleasePackageBundleEnumerationTests(unittest.TestCase):
    def test_literal_bundle_invocation_is_accepted(self) -> None:
        workflow = """
name: rust-release
jobs:
  build-windows:
    steps:
      - run: build-package --bundle syndrid
"""
        self.assertTrue(job_builds_syndrid_bundle(workflow, "build-windows"))

    def test_dynamic_bundle_enumeration_is_accepted(self) -> None:
        workflow = """
name: rust-release-windows
jobs:
  build-windows:
    steps:
      - run: |
          printf '%s\\0' primary app-server syndrid |
            xargs -0 -I{} bash build-package --bundle "{}"
"""
        self.assertTrue(job_builds_syndrid_bundle(workflow, "build-windows"))

    def test_dynamic_enumeration_without_syndrid_is_rejected(self) -> None:
        workflow = """
name: rust-release-windows
jobs:
  build-windows:
    steps:
      - run: |
          printf '%s\\0' primary app-server |
            xargs -0 -I{} bash build-package --bundle "{}"
"""
        self.assertFalse(job_builds_syndrid_bundle(workflow, "build-windows"))

    def test_unrelated_syndrid_text_does_not_satisfy_bundle_contract(self) -> None:
        workflow = """
name: rust-release-windows
jobs:
  build-windows:
    steps:
      - run: echo syndrid-package
      - run: |
          printf '%s\\0' primary app-server |
            xargs -0 -I{} bash build-package --bundle "{}"
"""
        self.assertFalse(job_builds_syndrid_bundle(workflow, "build-windows"))

    def test_live_windows_bundle_set_once_canonical_producer_lands(self) -> None:
        if not canonical_syndrid_producer_exists():
            self.skipTest("canonical Syndrid package producer is not on this lineage yet")
        workflow = WINDOWS_RELEASE_WORKFLOW.read_text(encoding="utf-8")
        self.assertTrue(
            job_builds_syndrid_bundle(workflow, "build-windows"),
            "signed Windows packaging must actually enumerate the canonical syndrid bundle",
        )


if __name__ == "__main__":
    unittest.main()
