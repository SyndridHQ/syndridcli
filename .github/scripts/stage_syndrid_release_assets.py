#!/usr/bin/env python3
"""Stage Syndrid-owned release metadata before GitHub publication.

This helper deliberately performs only deterministic local staging. It copies the
canonical Syndrid installers into the release asset directory and builds the
canonical Syndrid checksum manifest from already-produced package archives.
"""

from __future__ import annotations

import argparse
from pathlib import Path
import shutil
import subprocess
import sys

DEFAULT_ROOT = Path(__file__).resolve().parents[2]


def stage_release_assets(repo_root: Path, dist: Path) -> None:
    repo_root = repo_root.resolve()
    dist = dist.resolve()
    dist.mkdir(parents=True, exist_ok=True)

    installer_pairs = (
        (repo_root / "scripts/install/install-syndrid.sh", dist / "install.sh"),
        (repo_root / "scripts/install/install-syndrid.ps1", dist / "install.ps1"),
    )
    for source, destination in installer_pairs:
        if not source.is_file():
            raise RuntimeError(f"canonical Syndrid installer is missing: {source}")
        shutil.copyfile(source, destination)

    manifest_builder = repo_root / ".github/scripts/build-syndrid-checksum-manifest.py"
    if not manifest_builder.is_file():
        raise RuntimeError(f"Syndrid checksum builder is missing: {manifest_builder}")

    subprocess.run(
        [
            sys.executable,
            str(manifest_builder),
            "--assets-dir",
            str(dist),
            "--output",
            str(dist / "syndrid-package_SHA256SUMS"),
        ],
        check=True,
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo-root", type=Path, default=DEFAULT_ROOT)
    parser.add_argument("--dist", type=Path, required=True)
    args = parser.parse_args()

    try:
        stage_release_assets(args.repo_root, args.dist)
    except (RuntimeError, subprocess.CalledProcessError) as exc:
        print(f"Syndrid release asset staging failed: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
