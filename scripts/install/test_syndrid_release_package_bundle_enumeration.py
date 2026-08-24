from __future__ import annotations

import importlib.util
from pathlib import Path
import unittest

REPO_ROOT = Path(__file__).resolve().parents[2]
RELEASE_WORKFLOW = REPO_ROOT / ".github/workflows/rust-release.yml"
WINDOWS_RELEASE_WORKFLOW = REPO_ROOT / ".github/workflows/rust-release-windows.yml"
ARCHIVE_HELPER = REPO_ROOT / ".github/scripts/build-codex-package-archive.sh"
CONTRACT_CHECKER = REPO_ROOT / ".github/scripts/check_syndrid_release_contract.py"


def load_contract_checker():
    spec = importlib.util.spec_from_file_location("syndrid_release_contract", CONTRACT_CHECKER)
    if spec is None or spec.loader is None:
        raise RuntimeError("could not load Syndrid release contract checker")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


contract = load_contract_checker()


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
        self.assertTrue(contract.job_builds_syndrid_bundle(workflow, "build-windows"))

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
        self.assertTrue(contract.job_builds_syndrid_bundle(workflow, "build-windows"))

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
        self.assertFalse(contract.job_builds_syndrid_bundle(workflow, "build-windows"))

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
        self.assertFalse(contract.job_builds_syndrid_bundle(workflow, "build-windows"))

    def test_each_producer_job_is_checked_independently(self) -> None:
        release_workflow = """
name: rust-release
jobs:
  build:
    steps:
      - run: build-package --bundle syndrid
  finalize-macos:
    steps:
      - run: build-package --bundle primary
"""
        windows_workflow = """
name: rust-release-windows
jobs:
  build-windows:
    steps:
      - run: |
          printf '%s\\0' primary app-server syndrid |
            xargs -0 -I{} bash build-package --bundle "{}"
"""
        invariants: list[dict[str, str]] = []
        contract.append_package_producer_invariants(
            invariants, release_workflow, windows_workflow
        )
        self.assertEqual(
            [(item["path"], item["needle"]) for item in invariants],
            [(".github/workflows/rust-release.yml", "--bundle syndrid")],
        )

    def test_live_bundle_sets_once_canonical_producer_lands(self) -> None:
        if not canonical_syndrid_producer_exists():
            self.skipTest("canonical Syndrid package producer is not on this lineage yet")

        release_workflow = RELEASE_WORKFLOW.read_text(encoding="utf-8")
        windows_workflow = WINDOWS_RELEASE_WORKFLOW.read_text(encoding="utf-8")
        invariants: list[dict[str, str]] = []
        contract.append_package_producer_invariants(
            invariants, release_workflow, windows_workflow
        )
        self.assertEqual(
            invariants,
            [],
            "Linux, post-sign macOS, and signed Windows packaging must each actually request the canonical syndrid bundle",
        )


if __name__ == "__main__":
    unittest.main()
