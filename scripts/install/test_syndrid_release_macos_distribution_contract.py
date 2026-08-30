from __future__ import annotations

import importlib.util
from pathlib import Path
import tempfile
import unittest

REPO_ROOT = Path(__file__).resolve().parents[2]


def load_contract():
    path = REPO_ROOT / ".github/scripts/check_syndrid_release_contract.py"
    spec = importlib.util.spec_from_file_location(
        "syndrid_release_contract_macos", path
    )
    if spec is None or spec.loader is None:
        raise RuntimeError(f"could not load {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


contract = load_contract()


class SyndridReleaseMacOSDistributionContractTests(unittest.TestCase):
    def write(self, root: Path, path: str, content: str) -> None:
        full = root / path
        full.parent.mkdir(parents=True, exist_ok=True)
        full.write_text(content, encoding="utf-8")

    def seed(self, root: Path, extra: str = "") -> None:
        self.write(
            root,
            ".github/workflows/rust-release.yml",
            'binaries: "codex syndrid codex-code-mode-host"\n'
            "--bundle syndrid\n"
            "Cosign Linux release binaries\n"
            "syndrid-package-*.tar.gz\n"
            "Create GitHub Release\n"
            "files: dist/**\n" + extra,
        )
        self.write(
            root,
            ".github/workflows/rust-release-windows.yml",
            "--bundle syndrid\n",
        )
        self.write(
            root,
            ".github/workflows/rust-release-prepare.yml",
            "name: prepare\n",
        )
        self.write(root, "codex-cli/package.json", '{"name":"syndrid"}\n')
        self.write(root, "scripts/install/install.sh", "#!/bin/sh\n")
        self.write(root, "scripts/install/install.ps1", "# syndrid installer\n")

    def test_macos_is_not_required_for_v01(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.seed(root)
            result = contract.audit_release_contract(root)
            self.assertTrue(result["ok"], result)

    def test_macos_production_markers_are_blocked(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.seed(root, "build-macos:\n")
            result = contract.audit_release_contract(root)
            self.assertFalse(result["ok"])
            self.assertEqual(
                [finding["needle"] for finding in result["blockers"]],
                ["build-macos:"],
            )


if __name__ == "__main__":
    unittest.main()
