#!/usr/bin/env python3
"""Build the canonical checksum manifest for staged Syndrid release packages.

This helper is intentionally side-effect-limited: it reads an already-staged release
asset directory, validates that every canonical Syndrid target has both gzip and
zstd package archives, and writes one Syndrid-owned SHA-256 manifest.
"""

from __future__ import annotations

import argparse
import hashlib
from pathlib import Path
import sys

TARGETS = (
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "aarch64-unknown-linux-musl",
    "x86_64-unknown-linux-musl",
    "aarch64-pc-windows-msvc",
    "x86_64-pc-windows-msvc",
)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def find_unique(root: Path, filename: str) -> Path:
    matches = sorted(path for path in root.rglob(filename) if path.is_file())
    if len(matches) != 1:
        locations = ", ".join(str(path) for path in matches) or "none"
        raise RuntimeError(
            f"expected exactly one staged {filename}; found {len(matches)} ({locations})"
        )
    return matches[0]


def build_manifest(root: Path, output: Path) -> None:
    rows: list[tuple[str, str]] = []
    for target in TARGETS:
        for suffix in ("tar.gz", "tar.zst"):
            filename = f"syndrid-package-{target}.{suffix}"
            archive = find_unique(root, filename)
            rows.append((filename, sha256(archive)))

    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(
        "".join(f"{digest}  {filename}\n" for filename, digest in sorted(rows)),
        encoding="utf-8",
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--assets-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()

    assets_dir = args.assets_dir.resolve()
    if not assets_dir.is_dir():
        print(f"Syndrid checksum manifest: assets directory not found: {assets_dir}", file=sys.stderr)
        return 2

    try:
        build_manifest(assets_dir, args.output.resolve())
    except RuntimeError as error:
        print(f"Syndrid checksum manifest: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
