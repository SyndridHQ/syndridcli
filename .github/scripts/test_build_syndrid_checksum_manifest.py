from __future__ import annotations

import hashlib
import importlib.util
from pathlib import Path
import tempfile
import unittest

SCRIPT = Path(__file__).with_name("build-syndrid-checksum-manifest.py")
SPEC = importlib.util.spec_from_file_location("build_syndrid_checksum_manifest", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class BuildSyndridChecksumManifestTests(unittest.TestCase):
    def stage_complete_assets(self, root: Path) -> dict[str, bytes]:
        expected: dict[str, bytes] = {}
        for index, target in enumerate(MODULE.TARGETS):
            target_dir = root / target
            target_dir.mkdir(parents=True)
            for suffix in ("tar.gz", "tar.zst"):
                name = f"syndrid-package-{target}.{suffix}"
                payload = f"{index}:{suffix}".encode()
                (target_dir / name).write_bytes(payload)
                expected[name] = payload
        return expected

    def test_manifest_covers_every_canonical_archive(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            expected = self.stage_complete_assets(root)
            output = root / "syndrid-package_SHA256SUMS"

            MODULE.build_manifest(root, output)

            rows = output.read_text(encoding="utf-8").splitlines()
            self.assertEqual(len(rows), len(MODULE.TARGETS) * 2)
            self.assertEqual(rows, sorted(rows, key=lambda row: row.split("  ", 1)[1]))
            for name, payload in expected.items():
                digest = hashlib.sha256(payload).hexdigest()
                self.assertIn(f"{digest}  {name}", rows)

    def test_duplicate_archive_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.stage_complete_assets(root)
            duplicate = root / "duplicate"
            duplicate.mkdir()
            name = f"syndrid-package-{MODULE.TARGETS[0]}.tar.gz"
            (duplicate / name).write_bytes(b"duplicate")

            with self.assertRaisesRegex(RuntimeError, "expected exactly one staged"):
                MODULE.build_manifest(root, root / "manifest")

    def test_missing_archive_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.stage_complete_assets(root)
            missing = root / MODULE.TARGETS[-1] / f"syndrid-package-{MODULE.TARGETS[-1]}.tar.zst"
            missing.unlink()

            with self.assertRaisesRegex(RuntimeError, "expected exactly one staged"):
                MODULE.build_manifest(root, root / "manifest")


if __name__ == "__main__":
    unittest.main()
