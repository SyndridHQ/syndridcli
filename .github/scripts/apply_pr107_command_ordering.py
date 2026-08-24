#!/usr/bin/env python3

from __future__ import annotations

from pathlib import Path
import subprocess


def replace_once(text: str, old: str, new: str, label: str) -> str:
    if text.count(old) != 1:
        raise SystemExit(f"{label} anchor drifted: expected exactly one match")
    return text.replace(old, new)


checker_path = Path(".github/scripts/check_syndrid_release_contract.py")
checker = checker_path.read_text(encoding="utf-8")

helper_anchor = '''    return "\\n".join(lines[start:end])


def job_builds_syndrid_bundle(workflow: str, job_name: str) -> bool:
'''
helper_replacement = '''    return "\\n".join(lines[start:end])


def workflow_step_blocks(job_block: str) -> list[str]:
    """Split a workflow job into top-level step blocks in execution order."""
    lines = job_block.splitlines()
    starts = [index for index, line in enumerate(lines) if line.startswith("      - ")]
    blocks: list[str] = []
    for position, start in enumerate(starts):
        end = starts[position + 1] if position + 1 < len(starts) else len(lines)
        blocks.append("\\n".join(lines[start:end]))
    return blocks


def step_run_script(step_block: str) -> str | None:
    """Return only the shell script attached to one workflow step."""
    lines = step_block.splitlines()
    block_markers = {"|", ">", "|-", ">-", "|+", ">+"}
    for index, line in enumerate(lines):
        match = re.match(r"^(?:      - run:|        run:)\\s*(?P<value>.*)$", line)
        if match is None:
            continue
        value = match.group("value").strip()
        if value and value not in block_markers:
            return value

        body: list[str] = []
        for continuation in lines[index + 1 :]:
            if continuation.strip():
                indent = len(continuation) - len(continuation.lstrip())
                if indent <= 8:
                    break
                body.append(continuation[10:] if indent >= 10 else continuation.lstrip())
            else:
                body.append("")
        return "\\n".join(body)
    return None


def step_invokes_python_script(
    step_block: str, script_name: str, required_arg: str | None = None
) -> bool:
    """Return whether a run step actually invokes a Python helper command."""
    run_script = step_run_script(step_block)
    if run_script is None:
        return False

    normalized = re.sub(r"\\\\\\s*\\n\\s*", " ", run_script)
    runner = (
        r"(?:python(?:3(?:\\.\\d+)?)?|"
        r"uv\\s+run(?:\\s+\\S+)*\\s+python(?:3(?:\\.\\d+)?)?)"
    )
    invocation = re.compile(
        rf"^{runner}\\s+[^#\\n]*{re.escape(script_name)}(?:\\s|$)"
    )
    for raw_line in normalized.splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        if invocation.search(line) is None:
            continue
        if required_arg is None or required_arg in line:
            return True
    return False


def release_job_invokes_python_before_publication(
    release_workflow: str, script_name: str, required_arg: str | None = None
) -> bool:
    """Prove a real release-job helper invocation precedes GitHub publication."""
    release_block = workflow_job_block(release_workflow, "release")
    if release_block is None:
        return False
    steps = workflow_step_blocks(release_block)
    publish_index = next(
        (
            index
            for index, step in enumerate(steps)
            if re.search(r"(?m)^      - name:\\s*Create GitHub Release\\s*$", step)
            is not None
        ),
        None,
    )
    if publish_index is None:
        return False
    return any(
        step_invokes_python_script(step, script_name, required_arg)
        for step in steps[:publish_index]
    )


def job_builds_syndrid_bundle(workflow: str, job_name: str) -> bool:
'''
checker = replace_once(checker, helper_anchor, helper_replacement, "checker helper insertion")

ordering_old = '''    audit_present = (root / ".github/scripts/check_syndrid_release_contract.py").is_file()
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
'''
ordering_new = '''    audit_present = (root / ".github/scripts/check_syndrid_release_contract.py").is_file()
    smoke_present = (root / ".github/scripts/smoke_syndrid_release_binary.py").is_file()
    if not structured_release:
        if audit_present:
            required.append(AUDIT_REQUIRED)
        if smoke_present:
            required.extend([SMOKE_REQUIRED, SMOKE_VERSION_REQUIRED])

    for finding in required:
        if read(root, finding.path).count(finding.needle) < finding.minimum_count:
            append_invariant(invariants, finding)

    if structured_release:
        if audit_present and not release_job_invokes_python_before_publication(
            release_workflow, "check_syndrid_release_contract.py"
        ):
            append_invariant(invariants, AUDIT_REQUIRED)
        if smoke_present and not release_job_invokes_python_before_publication(
            release_workflow, "smoke_syndrid_release_binary.py"
        ):
            append_invariant(invariants, SMOKE_REQUIRED)
        if smoke_present and not release_job_invokes_python_before_publication(
            release_workflow,
            "smoke_syndrid_release_binary.py",
            required_arg="--expect-version",
        ):
            append_invariant(invariants, SMOKE_VERSION_REQUIRED)
    else:
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
'''
checker = replace_once(checker, ordering_old, ordering_new, "checker ordering replacement")
checker_path.write_text(checker, encoding="utf-8")

test_path = Path("scripts/install/test_syndrid_release_gate_ordering.py")
tests = test_path.read_text(encoding="utf-8")
marker = '\n\nif __name__ == "__main__":\n    unittest.main()\n'
additions = r'''

    def test_comment_or_echo_before_release_cannot_mask_late_audit(self) -> None:
        workflow = """
name: rust-release
jobs:
  release:
    steps:
      - name: Inert text
        run: |
          # python3 .github/scripts/check_syndrid_release_contract.py
          echo "python3 .github/scripts/check_syndrid_release_contract.py"
      - name: Create GitHub Release
        run: echo publish
      - name: Too-late audit
        run: python3 .github/scripts/check_syndrid_release_contract.py
"""
        self.assertFalse(
            contract.release_job_invokes_python_before_publication(
                workflow, "check_syndrid_release_contract.py"
            )
        )

    def test_real_audit_command_before_release_is_accepted(self) -> None:
        workflow = """
name: rust-release
jobs:
  release:
    steps:
      - name: Audit release contract
        run: python3 .github/scripts/check_syndrid_release_contract.py
      - name: Create GitHub Release
        run: echo publish
"""
        self.assertTrue(
            contract.release_job_invokes_python_before_publication(
                workflow, "check_syndrid_release_contract.py"
            )
        )

    def test_expect_version_must_belong_to_prepublication_smoke_command(self) -> None:
        workflow = """
name: rust-release
jobs:
  release:
    steps:
      - name: Smoke without version binding
        run: python3 .github/scripts/smoke_syndrid_release_binary.py staged/bin/syndrid
      - name: Inert version text
        run: echo "--expect-version 0.1.0"
      - name: Create GitHub Release
        run: echo publish
      - name: Too-late version-bound smoke
        run: python3 .github/scripts/smoke_syndrid_release_binary.py staged/bin/syndrid --expect-version 0.1.0
"""
        self.assertTrue(
            contract.release_job_invokes_python_before_publication(
                workflow, "smoke_syndrid_release_binary.py"
            )
        )
        self.assertFalse(
            contract.release_job_invokes_python_before_publication(
                workflow,
                "smoke_syndrid_release_binary.py",
                required_arg="--expect-version",
            )
        )
'''
if tests.count(marker) != 1:
    raise SystemExit("gate-ordering test insertion marker drifted")
test_path.write_text(tests.replace(marker, additions + marker), encoding="utf-8")

subprocess.run(
    [
        "uv",
        "run",
        "--frozen",
        "--project",
        "scripts",
        "ruff",
        "format",
        str(checker_path),
        str(test_path),
    ],
    check=True,
)
subprocess.run(
    [
        "python3",
        "-m",
        "unittest",
        "discover",
        "-s",
        "scripts/install",
        "-p",
        "test_syndrid_release_*.py",
    ],
    check=True,
)

for helper in (
    Path(".github/workflows/apply-pr107-command-ordering.yml"),
    Path(".github/scripts/apply_pr107_command_ordering.py"),
):
    helper.unlink()

allowed = {
    ".github/scripts/check_syndrid_release_contract.py",
    "scripts/install/test_syndrid_release_gate_ordering.py",
    ".github/workflows/apply-pr107-command-ordering.yml",
    ".github/scripts/apply_pr107_command_ordering.py",
}
changed = subprocess.check_output(["git", "diff", "--name-only"], text=True).splitlines()
unexpected = sorted(set(changed) - allowed)
if unexpected:
    raise SystemExit(f"unexpected apply delta: {unexpected}")
if checker_path.as_posix() not in changed or test_path.as_posix() not in changed:
    raise SystemExit("expected checker/test delta was not produced")

subprocess.run(["git", "diff", "--check"], check=True)
subprocess.run(["git", "config", "user.name", "github-actions[bot]"], check=True)
subprocess.run(
    [
        "git",
        "config",
        "user.email",
        "41898282+github-actions[bot]@users.noreply.github.com",
    ],
    check=True,
)
subprocess.run(
    [
        "git",
        "add",
        str(checker_path),
        str(test_path),
        ".github/workflows/apply-pr107-command-ordering.yml",
        ".github/scripts/apply_pr107_command_ordering.py",
    ],
    check=True,
)
subprocess.run(
    ["git", "commit", "-m", "release: verify real gates before publication"], check=True
)
subprocess.run(
    ["git", "push", "origin", "HEAD:fix/pr107-command-ordering"], check=True
)
