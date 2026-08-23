from __future__ import annotations

import importlib.util
from pathlib import Path
import tempfile
import unittest

REPO_ROOT = Path(__file__).resolve().parents[2]


def load_contract():
    path = REPO_ROOT / ".github/scripts/check_syndrid_release_contract.py"
    spec = importlib.util.spec_from_file_location("syndrid_release_contract_concurrency", path)
    if spec is None or spec.loader is None:
        raise RuntimeError("could not load release contract checker")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


contract = load_contract()


class SyndridReleaseConcurrencyTests(unittest.TestCase):
    def write(self, root: Path, relative_path: str, content: str) -> None:
        path = root / relative_path
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")

    def seed_contract(self, root: Path, concurrency: str) -> None:
        self.write(
            root,
            ".github/workflows/rust-release.yml",
            concurrency
            + 'binaries: "codex syndrid codex-code-mode-host"\n'
            + "--bundle syndrid\n"
            + "--bundle syndrid\n"
            + 'verify_signed_binary "${package_dir}/bin/syndrid" "syndrid"\n'
            + "syndrid-package-*.tar.gz\n"
            + "Create GitHub Release\n"
            + "files: dist/**\n",
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

    def test_global_cancel_in_progress_release_concurrency_is_blocked(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.seed_contract(
                root,
                "concurrency:\n"
                "  group: ${{ github.workflow }}\n"
                "  cancel-in-progress: true\n",
            )

            result = contract.audit_release_contract(root)

            self.assertFalse(result["ok"])
            self.assertEqual(result["missing_required_invariants"], [])
            self.assertEqual(
                [finding["needle"] for finding in result["blockers"]],
                ["group: ${{ github.workflow }}\n  cancel-in-progress: true"],
            )
            self.assertIn("one release cannot interrupt another", result["blockers"][0]["reason"])

    def test_tag_scoped_non_cancelling_release_concurrency_is_not_blocked(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.seed_contract(
                root,
                "concurrency:\n"
                "  group: ${{ github.workflow }}-${{ github.ref }}\n"
                "  cancel-in-progress: false\n",
            )

            result = contract.audit_release_contract(root)

            self.assertTrue(result["ok"])
            self.assertEqual(result["blockers"], [])
            self.assertEqual(result["missing_required_invariants"], [])


if __name__ == "__main__":
    unittest.main()
