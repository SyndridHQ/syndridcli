from __future__ import annotations

import importlib.util
from pathlib import Path
import tempfile
import unittest

REPO_ROOT = Path(__file__).resolve().parents[2]


def load_contract_module():
    path = REPO_ROOT / ".github/scripts/check_syndrid_release_contract.py"
    spec = importlib.util.spec_from_file_location("syndrid_release_contract_smoke_gate", path)
    if spec is None or spec.loader is None:
        raise RuntimeError("could not load release contract checker")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


contract = load_contract_module()


class SyndridReleaseSmokeContractTests(unittest.TestCase):
    def write(self, root: Path, relative_path: str, content: str) -> None:
        path = root / relative_path
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")

    def seed_contract(self, root: Path, *, include_smoke_step: bool) -> None:
        release_lines = [
            'binaries: "codex syndrid codex-code-mode-host"',
            "--bundle syndrid",
            "--bundle syndrid",
            'verify_signed_binary "${package_dir}/bin/syndrid" "syndrid"',
            "syndrid-package-*.tar.gz",
            "Create GitHub Release",
        ]
        if include_smoke_step:
            release_lines.append(
                "python3 .github/scripts/smoke_syndrid_release_binary.py --binary staged/syndrid --expect-version 0.1.0"
            )

        self.write(root, ".github/workflows/rust-release.yml", "\n".join(release_lines) + "\n")
        self.write(root, ".github/workflows/rust-release-windows.yml", "--bundle syndrid\n")
        self.write(root, ".github/workflows/rust-release-prepare.yml", "name: prepare\n")
        self.write(root, "codex-cli/package.json", '{"name":"syndrid"}\n')
        self.write(root, "scripts/install/install.sh", "#!/bin/sh\n")
        self.write(root, "scripts/install/install.ps1", "# syndrid installer\n")
        self.write(
            root,
            ".github/scripts/smoke_syndrid_release_binary.py",
            "# release smoke helper\n",
        )

    def test_missing_tag_workflow_smoke_is_release_blocker(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.seed_contract(root, include_smoke_step=False)

            result = contract.audit_release_contract(root)

            self.assertFalse(result["ok"])
            self.assertEqual(result["blockers"], [])
            self.assertEqual(
                [finding["needle"] for finding in result["missing_required_invariants"]],
                ["smoke_syndrid_release_binary.py"],
            )
            self.assertIn(
                "--help/--version",
                result["missing_required_invariants"][0]["reason"],
            )

    def test_tag_workflow_smoke_satisfies_release_invariant(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.seed_contract(root, include_smoke_step=True)

            result = contract.audit_release_contract(root)

            self.assertTrue(result["ok"])
            self.assertEqual(result["blockers"], [])
            self.assertEqual(result["missing_required_invariants"], [])


if __name__ == "__main__":
    unittest.main()
