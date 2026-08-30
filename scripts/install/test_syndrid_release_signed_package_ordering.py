from __future__ import annotations

from pathlib import Path
import re
import unittest

REPO_ROOT = Path(__file__).resolve().parents[2]
WINDOWS_RELEASE_WORKFLOW = REPO_ROOT / ".github/workflows/rust-release-windows.yml"

def workflow_job_block(workflow: str, job_name: str) -> str | None:
    lines = workflow.splitlines()
    try: start = lines.index(f"  {job_name}:")
    except ValueError: return None
    end = len(lines)
    for index in range(start + 1, len(lines)):
        if re.fullmatch(r"  [A-Za-z0-9_-]+:\\s*", lines[index]): end = index; break
    return "\\n".join(lines[start:end])

def unsigned_package_has_integrity_checks(workflow: str) -> bool:
    job = workflow_job_block(workflow, "build-windows")
    return bool(job and "--bundle syndrid" in job and "Download unsigned Syndrid runtime binaries" in job and "Verify unsigned runtime inputs" in job and "Verify unsigned Syndrid package contents" in job and "Get-AuthenticodeSignature" not in job)

class SyndridReleasePackageOrderingTests(unittest.TestCase):
    def test_windows_package_is_built_from_verified_unsigned_inputs(self) -> None:
        safe = """name: rust-release-windows
jobs:
  build-windows:
    steps:
      - name: Download unsigned Syndrid runtime binaries
      - name: Verify unsigned runtime inputs
      - name: Build canonical unsigned Syndrid package
        run: build-package --bundle syndrid
      - name: Verify unsigned Syndrid package contents
"""
        unsafe = """name: rust-release-windows
jobs:
  build-windows:
    steps:
      - name: Build canonical unsigned Syndrid package
        run: build-package --bundle syndrid
      - name: Download unsigned Syndrid runtime binaries
      - name: Verify unsigned runtime inputs
"""
        self.assertTrue(unsigned_package_has_integrity_checks(safe)); self.assertFalse(unsigned_package_has_integrity_checks(unsafe))

    def test_live_windows_package_is_unsigned_but_integrity_checked(self) -> None:
        workflow = WINDOWS_RELEASE_WORKFLOW.read_text(encoding="utf-8")
        self.assertTrue(unsigned_package_has_integrity_checks(workflow))

if __name__ == "__main__": unittest.main()
