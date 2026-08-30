from __future__ import annotations

import importlib.util
from pathlib import Path
import tempfile
import unittest

REPO_ROOT = Path(__file__).resolve().parents[2]


def load_contract():
    path = REPO_ROOT / ".github/scripts/check_syndrid_release_contract.py"
    spec = importlib.util.spec_from_file_location(
        "syndrid_release_contract_provenance", path
    )
    if spec is None or spec.loader is None:
        raise RuntimeError(f"could not load {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


contract = load_contract()


class SyndridReleaseTagProvenanceTests(unittest.TestCase):
    def write(self, root: Path, relative_path: str, content: str) -> None:
        path = root / relative_path
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")

    def seed_contract(self, root: Path, provenance: str = "") -> None:
        self.write(
            root,
            ".github/workflows/rust-release.yml",
            "tag-check:\n"
            'binaries: "codex syndrid codex-code-mode-host"\n'
            "--bundle syndrid\n"
            "Cosign Linux release binaries\n"
            "syndrid-package-*.tar.gz\n"
            "Create GitHub Release\n"
            "files: dist/**\n"
            f"{provenance}",
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

    def test_tag_check_without_main_provenance_is_blocked(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.seed_contract(root)

            result = contract.audit_release_contract(root)

            self.assertFalse(result["ok"])
            self.assertEqual(result["blockers"], [])
            self.assertEqual(
                [
                    finding["needle"]
                    for finding in result["missing_required_invariants"]
                ],
                [
                    "git fetch --no-tags origin main",
                    'git merge-base --is-ancestor "${GITHUB_SHA}" "origin/main"',
                ],
            )

    def test_tag_check_with_main_provenance_passes(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.seed_contract(
                root,
                "git fetch --no-tags origin main\n"
                'git merge-base --is-ancestor "${GITHUB_SHA}" "origin/main"\n',
            )

            result = contract.audit_release_contract(root)

            self.assertTrue(result["ok"])
            self.assertEqual(result["blockers"], [])
            self.assertEqual(result["missing_required_invariants"], [])


if __name__ == "__main__":
    unittest.main()
