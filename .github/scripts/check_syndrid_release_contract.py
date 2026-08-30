#!/usr/bin/env python3
"""Audit SyndridCLI release inputs before a Syndrid-owned tag is published.

The checker is intentionally scoped to behavior reachable from the production
v0.1 tag workflow. Dormant upstream compatibility files are not publication
blockers unless the tag workflow consumes them.
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


# These are direct production tag-workflow hazards. Historical npm/package and
# installer compatibility files are deliberately not blockers when the release
# workflow neither publishes npm nor stages those legacy installers.
FORBIDDEN = [
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
]


TAG_SIDE_EFFECTS = [
    Finding(
        ".github/workflows/rust-release.yml",
        "publish-dotslash:",
        "the tag workflow still publishes DotSlash metadata whose current output contract is Codex-only; explicitly migrate or disable this side effect before a Syndrid v0.1 tag",
    ),
    Finding(
        ".github/workflows/rust-release.yml",
        "argument-comment-lint-release-assets:",
        "the Syndrid tag workflow still schedules inherited argument-comment-lint release artifacts that are outside the v0.1 Syndrid package contract",
    ),
    Finding(
        ".github/workflows/rust-release.yml",
        "repos/${GITHUB_REPOSITORY}/git/refs/heads/latest-alpha-cli",
        "the tag workflow still force-updates the inherited latest-alpha-cli branch; explicitly accept, rename, or disable this moving-ref side effect before a Syndrid v0.1 tag",
    ),
    Finding(
        ".github/workflows/rust-release-windows.yml",
        "Build Python runtime wheel",
        "the tag workflow still builds an inherited OpenAI Python runtime wheel outside the Syndrid v0.1 package contract",
    ),
    Finding(
        ".github/workflows/rust-release-windows.yml",
        "--bundle primary",
        "the tag workflow still builds an inherited canonical Codex package alongside the Syndrid package",
    ),
    Finding(
        ".github/workflows/rust-release-windows.yml",
        "--bundle app-server",
        "the tag workflow still builds an inherited Codex app-server package alongside the Syndrid package",
    ),
    Finding(
        ".github/workflows/rust-release-windows.yml",
        "Build Windows symbols",
        "the tag workflow still publishes an inherited Windows symbol artifact outside the Syndrid v0.1 package contract",
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
        "the primary release path must continue to build the Syndrid binary",
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
        raise RuntimeError(
            f"release-contract audit: missing required file: {path}"
        ) from None


def append_invariant(invariants: list[dict[str, str]], finding: Finding) -> None:
    invariants.append(
        {"path": finding.path, "needle": finding.needle, "reason": finding.reason}
    )


def is_structured_release_workflow(release_workflow: str) -> bool:
    """Return whether this is the real tag workflow rather than a unit-test fragment."""
    return (
        "name: rust-release" in release_workflow
        and re.search(r"(?m)^jobs:\s*$", release_workflow) is not None
    )


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
    return (
        re.search(
            r"(?m)^\s*cancel-in-progress:\s*false\s*(?:#.*)?$",
            concurrency_match.group("body"),
        )
        is not None
    )


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


def workflow_step_blocks(job_block: str) -> list[str]:
    """Split a workflow job into top-level step blocks in execution order."""
    lines = job_block.splitlines()
    starts = [index for index, line in enumerate(lines) if line.startswith("      - ")]
    blocks: list[str] = []
    for position, start in enumerate(starts):
        end = starts[position + 1] if position + 1 < len(starts) else len(lines)
        blocks.append("\n".join(lines[start:end]))
    return blocks


def step_run_script(step_block: str) -> str | None:
    """Return only the shell script attached to one workflow step."""
    lines = step_block.splitlines()
    block_markers = {"|", ">", "|-", ">-", "|+", ">+"}
    for index, line in enumerate(lines):
        match = re.match(r"^(?:      - run:|        run:)\s*(?P<value>.*)$", line)
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
                body.append(
                    continuation[10:] if indent >= 10 else continuation.lstrip()
                )
            else:
                body.append("")
        return "\n".join(body)
    return None


def step_invokes_python_script(
    step_block: str, script_name: str, required_arg: str | None = None
) -> bool:
    """Return whether a run step actually invokes a Python helper command."""
    run_script = step_run_script(step_block)
    if run_script is None:
        return False

    normalized = re.sub(r"\\\s*\n\s*", " ", run_script)
    runner = (
        r"(?:python(?:3(?:\.\d+)?)?|"
        r"uv\s+run(?:\s+\S+)*\s+python(?:3(?:\.\d+)?)?)"
    )
    invocation = re.compile(rf"^{runner}\s+[^#\n]*{re.escape(script_name)}(?:\s|$)")
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
            if re.search(r"(?m)^      - name:\s*Create GitHub Release\s*$", step)
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
    """Return whether one producer job actually requests the canonical Syndrid bundle."""
    job = workflow_job_block(workflow, job_name)
    if job is None:
        return False

    if re.search(r"--bundle(?:=|\s+)[\"']?syndrid[\"']?(?:\s|\\|$)", job):
        return True

    dynamic_bundle = re.search(
        r"printf\s+['\"]%s\\0['\"]\s+(?P<bundles>[^\n|]+)\|(?P<pipeline>.*?--bundle\s+[\"']?\{\}[\"']?)",
        job,
        flags=re.DOTALL,
    )
    if dynamic_bundle is None:
        return False

    bundles = re.findall(r"[A-Za-z0-9_-]+", dynamic_bundle.group("bundles"))
    return "syndrid" in bundles


def release_builds_syndrid_binary(release_workflow: str) -> bool:
    """Accept either the legacy release matrix or an explicit Syndrid build."""
    if REQUIRED[0].needle in release_workflow:
        return True
    return re.search(r"(?m)^\s*--bin\s+syndrid(?:\s|\\|$)", release_workflow) is not None


def append_package_producer_invariants(
    invariants: list[dict[str, str]], release_workflow: str, windows_workflow: str
) -> None:
    for workflow, job_name, finding in (
        (release_workflow, "build", REQUIRED[1]),
        (release_workflow, "finalize-macos", REQUIRED[1]),
        (windows_workflow, "build-windows", REQUIRED[2]),
    ):
        if not job_builds_syndrid_bundle(workflow, job_name):
            append_invariant(invariants, finding)


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

    needs = set(re.findall(r"(?m)^      - ([A-Za-z0-9_-]+)\s*$", release_block))
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
        and 'zstd_archive_path="${archive_dir}/${archive_stem}-${target}.tar.zst"'
        in content
        and '--archive-output "$zstd_archive_path"' in content
    )


def audit_release_contract(root: Path) -> dict[str, object]:
    blockers: list[dict[str, str]] = []
    invariants: list[dict[str, str]] = []

    release_workflow = read(root, ".github/workflows/rust-release.yml")
    windows_workflow = read(root, ".github/workflows/rust-release-windows.yml")

    for finding in [
        *FORBIDDEN,
        *TAG_SIDE_EFFECTS,
        *PACKAGE_INPUT_BLOCKERS,
        *MACOS_DISTRIBUTION_BLOCKERS,
    ]:
        if finding.needle in read(root, finding.path):
            blockers.append(
                {
                    "path": finding.path,
                    "needle": finding.needle,
                    "reason": finding.reason,
                }
            )

    structured_release = is_structured_release_workflow(release_workflow)
    required = [*REQUIRED[3:]] if structured_release else list(REQUIRED)
    if canonical_syndrid_producer_emits_zstd(root):
        required.extend([ZSTD_CHECKSUM_REQUIRED, SYNDRID_CHECKSUM_MANIFEST_REQUIRED])
    if "tag-check:" in release_workflow:
        required.extend(TAG_PROVENANCE_REQUIRED)

    if structured_release:
        if not release_builds_syndrid_binary(release_workflow):
            append_invariant(invariants, REQUIRED[0])
        if not has_tag_scoped_release_concurrency(release_workflow):
            append_invariant(invariants, RELEASE_CONCURRENCY_GROUP_REQUIRED)
        if not has_non_cancelling_release_concurrency(release_workflow):
            append_invariant(invariants, RELEASE_CONCURRENCY_CANCEL_REQUIRED)
        append_package_producer_invariants(
            invariants, release_workflow, windows_workflow
        )
        append_release_publication_dependency_invariants(invariants, release_workflow)

    audit_present = (
        root / ".github/scripts/check_syndrid_release_contract.py"
    ).is_file()
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

    return {
        "ok": not blockers and not invariants,
        "blockers": blockers,
        "missing_required_invariants": invariants,
    }


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Audit SyndridCLI pre-tag release contract."
    )
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
