from __future__ import annotations

from pathlib import Path
import re
import unittest

REPO_ROOT = Path(__file__).resolve().parents[2]
RELEASE_WORKFLOW = REPO_ROOT / ".github/workflows/rust-release.yml"
WINDOWS_RELEASE_WORKFLOW = REPO_ROOT / ".github/workflows/rust-release-windows.yml"


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


def syndrid_package_occurs_after(job_block: str, required_marker: str) -> bool:
    marker_index = job_block.find(required_marker)
    if marker_index < 0:
        return False
    package_index = job_block.find("--bundle syndrid")
    return package_index > marker_index


def windows_syndrid_package_uses_signed_bytes(workflow: str) -> bool:
    job = workflow_job_block(workflow, "build-windows")
    if job is None:
        return False
    return syndrid_package_occurs_after(
        job,
        "Sign Windows binaries with Azure Trusted Signing",
    )


def macos_syndrid_package_uses_signed_bytes(workflow: str) -> bool:
    job = workflow_job_block(workflow, "finalize-macos")
    if job is None:
        return False
    return syndrid_package_occurs_after(job, "Download signed macOS binaries")


class SyndridReleaseSignedPackageOrderingTests(unittest.TestCase):
    def test_windows_syndrid_package_must_follow_signing(self) -> None:
        safe = """name: rust-release-windows
jobs:
  build-windows:
    steps:
      - name: Sign Windows binaries with Azure Trusted Signing
      - name: Build Syndrid package archives
        run: build-package --bundle syndrid
"""
        unsafe = """name: rust-release-windows
jobs:
  build-windows:
    steps:
      - name: Build Syndrid package archives
        run: build-package --bundle syndrid
      - name: Sign Windows binaries with Azure Trusted Signing
"""
        self.assertTrue(windows_syndrid_package_uses_signed_bytes(safe))
        self.assertFalse(windows_syndrid_package_uses_signed_bytes(unsafe))

    def test_macos_syndrid_package_must_follow_signed_binary_download(self) -> None:
        safe = """name: rust-release
jobs:
  finalize-macos:
    steps:
      - name: Download signed macOS binaries
      - name: Build Syndrid package archives
        run: build-package --bundle syndrid
"""
        unsafe = """name: rust-release
jobs:
  finalize-macos:
    steps:
      - name: Build Syndrid package archives
        run: build-package --bundle syndrid
      - name: Download signed macOS binaries
"""
        self.assertTrue(macos_syndrid_package_uses_signed_bytes(safe))
        self.assertFalse(macos_syndrid_package_uses_signed_bytes(unsafe))

    def test_live_windows_ordering_activates_when_syndrid_packaging_is_wired(self) -> None:
        workflow = WINDOWS_RELEASE_WORKFLOW.read_text(encoding="utf-8")
        if "--bundle syndrid" not in workflow:
            self.skipTest("canonical Windows Syndrid packaging is not wired yet")
        self.assertTrue(windows_syndrid_package_uses_signed_bytes(workflow))

    def test_live_macos_ordering_activates_when_syndrid_packaging_is_wired(self) -> None:
        workflow = RELEASE_WORKFLOW.read_text(encoding="utf-8")
        finalize = workflow_job_block(workflow, "finalize-macos") or ""
        if "--bundle syndrid" not in finalize:
            self.skipTest("canonical post-sign macOS Syndrid packaging is not wired yet")
        self.assertTrue(macos_syndrid_package_uses_signed_bytes(workflow))


if __name__ == "__main__":
    unittest.main()
