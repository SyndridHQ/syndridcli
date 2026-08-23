from __future__ import annotations

import importlib.util
from pathlib import Path
import tempfile
import unittest

REPO_ROOT = Path(__file__).resolve().parents[2]
SCRIPT_PATH = REPO_ROOT / ".github/scripts/check_syndrid_release_contract.py"

spec = importlib.util.spec_from_file_location("syndrid_release_contract_checksum", SCRIPT_PATH)
if spec is None or spec.loader is None:
    raise RuntimeError("could not load Syndrid release contract checker")
contract = importlib.util.module_from_spec(spec)
spec.loader.exec_module(contract)


class SyndridReleaseChecksumContractTests(unittest.TestCase):
    def write(self, root: Path, relative_path: str, content: str) -> None:
        path = root / relative_path
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")

    def seed_safe_contract(self, root: Path) -> None:
        self.write(
            root,
            ".github/workflows/rust-release.yml",
            "concurrency:\n"
            "  group: release-${{ github.ref_name }}\n"
            "  cancel-in-progress: false\n"
            'binaries: "codex syndrid codex-code-mode-host"\n'
            "--bundle syndrid\n"
            "--bundle syndrid\n"
            'verify_signed_binary "${package_dir}/bin/syndrid" "syndrid"\n'
            "syndrid-package-*.tar.gz\n"
            "syndrid-package-*.tar.zst\n"
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
        self.write(root, "scripts/install/install.sh", "#!/bin/sh\n")
        self.write(root, "scripts/install/install.ps1", "# syndrid installer\n")

    def test_both_canonical_archive_forms_are_required(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.seed_safe_contract(root)

            result = contract.audit_release_contract(root)

            self.assertTrue(result["ok"])
            self.assertEqual(result["missing_required_invariants"], [])

    def test_missing_zstd_checksum_coverage_is_reported_independently(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.seed_safe_contract(root)
            workflow = root / ".github/workflows/rust-release.yml"
            workflow.write_text(
                workflow.read_text(encoding="utf-8").replace(
                    "syndrid-package-*.tar.zst\n",
                    "",
                ),
                encoding="utf-8",
            )

            result = contract.audit_release_contract(root)

            self.assertFalse(result["ok"])
            self.assertEqual(
                [finding["needle"] for finding in result["missing_required_invariants"]],
                ["syndrid-package-*.tar.zst"],
            )
            self.assertIn(
                "zstd package archives",
                result["missing_required_invariants"][0]["reason"],
            )


if __name__ == "__main__":
    unittest.main()
