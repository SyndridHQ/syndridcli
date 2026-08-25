from __future__ import annotations

import hashlib
import importlib.util
from pathlib import Path
import tempfile
import unittest

MODULE_PATH = Path(__file__).with_name("stage_syndrid_release_assets.py")
SPEC = importlib.util.spec_from_file_location("stage_syndrid_release_assets", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)

TARGETS = (
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "aarch64-unknown-linux-musl",
    "x86_64-unknown-linux-musl",
    "aarch64-pc-windows-msvc",
    "x86_64-pc-windows-msvc",
)


class StageSyndridReleaseAssetsTest(unittest.TestCase):
    def test_stages_syndrid_installers_and_manifest(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo = root / "repo"
            dist = root / "dist"
            scripts = repo / "scripts/install"
            github_scripts = repo / ".github/scripts"
            scripts.mkdir(parents=True)
            github_scripts.mkdir(parents=True)

            (scripts / "install-syndrid.sh").write_text("unix-installer\n", encoding="utf-8")
            (scripts / "install-syndrid.ps1").write_text("windows-installer\n", encoding="utf-8")
            builder = github_scripts / "build-syndrid-checksum-manifest.py"
            builder.write_text(
                (Path(__file__).with_name("build-syndrid-checksum-manifest.py")).read_text(
                    encoding="utf-8"
                ),
                encoding="utf-8",
            )

            expected: dict[str, str] = {}
            for target in TARGETS:
                target_dir = dist / target
                target_dir.mkdir(parents=True, exist_ok=True)
                for suffix in ("tar.gz", "tar.zst"):
                    name = f"syndrid-package-{target}.{suffix}"
                    payload = name.encode("utf-8")
                    (target_dir / name).write_bytes(payload)
                    expected[name] = hashlib.sha256(payload).hexdigest()

            MODULE.stage_release_assets(repo, dist)

            self.assertEqual((dist / "install.sh").read_text(), "unix-installer\n")
            self.assertEqual((dist / "install.ps1").read_text(), "windows-installer\n")

            rows = {}
            for line in (dist / "syndrid-package_SHA256SUMS").read_text().splitlines():
                digest, name = line.split("  ", 1)
                rows[name] = digest
            self.assertEqual(rows, expected)

    def test_missing_installer_fails_before_manifest_generation(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo = root / "repo"
            dist = root / "dist"
            (repo / "scripts/install").mkdir(parents=True)
            (repo / "scripts/install/install-syndrid.sh").write_text("unix\n", encoding="utf-8")

            with self.assertRaisesRegex(RuntimeError, "canonical Syndrid installer is missing"):
                MODULE.stage_release_assets(repo, dist)


if __name__ == "__main__":
    unittest.main()
