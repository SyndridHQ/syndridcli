#!/usr/bin/env python3
"""Perform side-effect-minimal smoke checks on a staged Syndrid CLI binary.

The release hardening contract deliberately limits this script to clap-style
metadata paths. It does not log in, read project state, invoke providers, or run
an interactive session.
"""

from __future__ import annotations

import argparse
from pathlib import Path
import re
import subprocess
import sys

VERSION_RE = re.compile(r"\b\d+\.\d+\.\d+(?:-(?:alpha|beta)(?:\.\d+)?)?\b")


def run_metadata_command(binary: Path, flag: str, timeout_seconds: float) -> str:
    try:
        completed = subprocess.run(
            [str(binary), flag],
            check=False,
            capture_output=True,
            text=True,
            timeout=timeout_seconds,
        )
    except subprocess.TimeoutExpired as exc:
        raise RuntimeError(f"{flag} timed out after {timeout_seconds:g}s") from exc
    except OSError as exc:
        raise RuntimeError(f"could not execute staged binary: {exc}") from exc

    combined = "\n".join(part for part in (completed.stdout, completed.stderr) if part).strip()
    if completed.returncode != 0:
        raise RuntimeError(
            f"{flag} exited with status {completed.returncode}: {combined or '<no output>'}"
        )
    if not combined:
        raise RuntimeError(f"{flag} succeeded but produced no output")
    return combined


def smoke_release_binary(
    binary: Path,
    *,
    expected_version: str | None = None,
    timeout_seconds: float = 10.0,
) -> dict[str, object]:
    binary = binary.resolve()
    if not binary.is_file():
        raise RuntimeError(f"staged binary does not exist or is not a file: {binary}")
    if timeout_seconds <= 0:
        raise ValueError("timeout_seconds must be positive")
    if expected_version is not None and VERSION_RE.fullmatch(expected_version) is None:
        raise ValueError("expected_version must be x.y.z[-alpha[.N]|-beta[.N]]")

    help_output = run_metadata_command(binary, "--help", timeout_seconds)
    version_output = run_metadata_command(binary, "--version", timeout_seconds)

    version_match = VERSION_RE.search(version_output)
    if version_match is None:
        raise RuntimeError("--version output does not contain a supported semantic version")

    observed_version = version_match.group(0)
    if expected_version is not None and observed_version != expected_version:
        raise RuntimeError(
            "staged binary version does not match expected release version: "
            f"expected={expected_version} observed={observed_version}"
        )

    return {
        "binary": str(binary),
        "version": observed_version,
        "help_output_bytes": len(help_output.encode("utf-8")),
    }


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Run safe --help/--version smoke checks on a staged Syndrid binary."
    )
    parser.add_argument("binary", type=Path, help="Path to the staged Syndrid executable")
    parser.add_argument(
        "--expect-version",
        help="Require --version to report this exact semantic version",
    )
    parser.add_argument(
        "--timeout-seconds",
        type=float,
        default=10.0,
        help="Per-command timeout (default: 10 seconds)",
    )
    args = parser.parse_args()

    try:
        result = smoke_release_binary(
            args.binary,
            expected_version=args.expect_version,
            timeout_seconds=args.timeout_seconds,
        )
    except (RuntimeError, ValueError) as exc:
        print(f"Syndrid release smoke check failed: {exc}", file=sys.stderr)
        return 1

    print(f"Syndrid release smoke check passed for {result['binary']}")
    print(f"version={result['version']}")
    print(f"help_output_bytes={result['help_output_bytes']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
