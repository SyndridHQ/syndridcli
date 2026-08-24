from __future__ import annotations

from pathlib import Path
import re
import unittest

REPO_ROOT = Path(__file__).resolve().parents[2]
RELEASE_WORKFLOW = REPO_ROOT / ".github/workflows/rust-release.yml"

REQUIRED_TARGET_TOKENS = (
    "aarch64",
    "x86_64",
    "apple-darwin",
    "unknown-linux-musl",
    "pc-windows-msvc",
)


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


def named_step_block(job_block: str, step_name: str) -> str | None:
    lines = job_block.splitlines()
    target = f"      - name: {step_name}"
    try:
        start = lines.index(target)
    except ValueError:
        return None

    end = len(lines)
    for index in range(start + 1, len(lines)):
        if lines[index].startswith("      - name:") or lines[index].startswith(
            "      - uses:"
        ):
            end = index
            break
    return "\n".join(lines[start:end])


def missing_target_tokens(workflow: str) -> list[str]:
    release_block = workflow_job_block(workflow, "release")
    if release_block is None:
        return list(REQUIRED_TARGET_TOKENS)

    download_block = named_step_block(release_block, "Download target artifacts")
    if download_block is None:
        return list(REQUIRED_TARGET_TOKENS)

    return [token for token in REQUIRED_TARGET_TOKENS if token not in download_block]


class SyndridReleaseArtifactCollectionTests(unittest.TestCase):
    def test_live_release_job_collects_every_release_target_family(self) -> None:
        workflow = RELEASE_WORKFLOW.read_text(encoding="utf-8")
        self.assertEqual(missing_target_tokens(workflow), [])

    def test_missing_architecture_or_platform_is_rejected(self) -> None:
        complete_step = (
            "name: rust-release\n"
            "jobs:\n"
            "  release:\n"
            "    steps:\n"
            "      - name: Download target artifacts\n"
            "        with:\n"
            '          pattern: "{aarch64,x86_64}-{apple-darwin,unknown-linux-musl,pc-windows-msvc}"\n'
            "      - name: Create GitHub Release\n"
        )
        self.assertEqual(missing_target_tokens(complete_step), [])

        for token in REQUIRED_TARGET_TOKENS:
            with self.subTest(token=token):
                incomplete = complete_step.replace(token, "omitted-target")
                self.assertIn(token, missing_target_tokens(incomplete))

    def test_missing_target_artifact_download_step_is_rejected(self) -> None:
        workflow = (
            "name: rust-release\n"
            "jobs:\n"
            "  release:\n"
            "    steps:\n"
            "      - name: Create GitHub Release\n"
        )
        self.assertEqual(missing_target_tokens(workflow), list(REQUIRED_TARGET_TOKENS))


if __name__ == "__main__":
    unittest.main()
