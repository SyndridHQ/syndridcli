from __future__ import annotations

import importlib.util
from pathlib import Path
import tempfile
import unittest

REPO_ROOT = Path(__file__).resolve().parents[2]


def load_contract():
    script_path = REPO_ROOT / ".github/scripts/check_syndrid_release_contract.py"
    spec = importlib.util.spec_from_file_location("syndrid_release_contract_installers", script_path)
    if spec is None or spec.loader is None:
        raise RuntimeError("could not load release contract checker")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


contract = load_contract()


class SyndridReleaseInstallerContractTests(unittest.TestCase):
    def write(self, root: Path, relative_path: str, content: str) -> None:
        path = root / relative_path
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")

    def seed_safe_contract(self, root: Path) -> None:
        self.write(
            root,
            ".github/workflows/rust-release.yml",
            'binaries: "codex syndrid codex-code-mode-host"\n'
            "--bundle syndrid\n"
            "--bundle syndrid\n"
            'verify_signed_binary "${package_dir}/bin/syndrid" "syndrid"\n'
            "syndrid-package-*.tar.gz\n"
            "Create GitHub Release\n"
            "files: dist/**\n",
        )
        self.write(
            root,
            ".github/workflows/rust-release-windows.yml",
            "--bundle syndrid\n",
        )
        self.write(root, ".github/workflows/rust-release-prepare.yml", "name: prepare\n")
        self.write(root, "codex-cli/package.json", '{"name":"syndrid"}\n')
        self.write(
            root,
            "scripts/install/install.sh",
            '#!/bin/sh\nBIN_PATH="$BIN_DIR/syndrid"\n',
        )
        self.write(
            root,
            "scripts/install/install.ps1",
            '$SyndridPath = Join-Path $StandaloneCurrentDir "bin\\syndrid.exe"\n',
        )

    def test_syndrid_owned_installer_entrypoints_are_not_blocked(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.seed_safe_contract(root)

            result = contract.audit_release_contract(root)

            self.assertTrue(result["ok"])
            self.assertEqual(result["blockers"], [])

    def test_unix_codex_entrypoint_is_a_release_blocker(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.seed_safe_contract(root)
            self.write(
                root,
                "scripts/install/install.sh",
                '#!/bin/sh\nBIN_PATH="$BIN_DIR/codex"\n',
            )

            result = contract.audit_release_contract(root)

            self.assertFalse(result["ok"])
            self.assertEqual(
                [finding["needle"] for finding in result["blockers"]],
                ['BIN_PATH="$BIN_DIR/codex"'],
            )
            self.assertIn("canonical Syndrid entrypoint", result["blockers"][0]["reason"])

    def test_windows_codex_entrypoint_is_a_release_blocker(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.seed_safe_contract(root)
            self.write(
                root,
                "scripts/install/install.ps1",
                '$CodexPath = Join-Path $StandaloneCurrentDir "bin\\codex.exe"\n',
            )

            result = contract.audit_release_contract(root)

            self.assertFalse(result["ok"])
            self.assertEqual(
                [finding["needle"] for finding in result["blockers"]],
                ['Join-Path $StandaloneCurrentDir "bin\\codex.exe"'],
            )
            self.assertIn("canonical installed entrypoint", result["blockers"][0]["reason"])


if __name__ == "__main__":
    unittest.main()
