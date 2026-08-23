from __future__ import annotations

import importlib.util
from pathlib import Path
import tempfile
import unittest

REPO_ROOT = Path(__file__).resolve().parents[2]


def load_repo_script(module_name: str, relative_path: str):
    script_path = REPO_ROOT / relative_path
    spec = importlib.util.spec_from_file_location(module_name, script_path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"could not load {relative_path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


contract = load_repo_script(
    "syndrid_release_contract",
    ".github/scripts/check_syndrid_release_contract.py",
)
smoke = load_repo_script(
    "syndrid_release_smoke",
    ".github/scripts/smoke_syndrid_release_binary.py",
)


class SyndridReleaseContractTests(unittest.TestCase):
    def write(self, root: Path, relative_path: str, content: str) -> None:
        path = root / relative_path
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")

    def seed_safe_contract(self, root: Path) -> None:
        self.write(
            root,
            ".github/workflows/rust-release.yml",
            'binaries: "codex syndrid codex-code-mode-host"\n--bundle syndrid\nCreate GitHub Release\n',
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

    def test_safe_contract_preserves_required_syndrid_artifact_invariants(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.seed_safe_contract(root)

            result = contract.audit_release_contract(root)

            self.assertTrue(result["ok"])
            self.assertEqual(result["blockers"], [])
            self.assertEqual(result["missing_required_invariants"], [])

    def test_upstream_publication_assumptions_are_reported(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.seed_safe_contract(root)
            self.write(
                root,
                ".github/workflows/rust-release-prepare.yml",
                "if: github.repository == 'openai/codex'\n",
            )
            self.write(
                root,
                ".github/workflows/rust-release.yml",
                "\n".join(
                    [
                        'binaries: "codex syndrid codex-code-mode-host"',
                        "--bundle syndrid",
                        "Create GitHub Release",
                        'scope: "@openai"',
                        "npm publish",
                        "developers.openai.com",
                        "identifier: OpenAI.Codex",
                        "microsoft/winget-pkgs",
                        "https://github.com/openai/codex/releases/",
                        "fork-user: openai-oss-forks",
                        "git push origin HEAD:main",
                        "name: codesigning",
                    ]
                ),
            )
            self.write(
                root,
                "codex-cli/package.json",
                '{"name":"@openai/codex","repository":"https://github.com/openai/codex.git"}\n',
            )
            self.write(root, "scripts/install/install.sh", "openai/codex\n")
            self.write(root, "scripts/install/install.ps1", "openai/codex\n")

            result = contract.audit_release_contract(root)

            self.assertFalse(result["ok"])
            self.assertEqual(len(result["blockers"]), len(contract.FORBIDDEN))
            self.assertEqual(result["missing_required_invariants"], [])

    def test_side_effecting_publication_commands_are_blocked_without_upstream_branding(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.seed_safe_contract(root)
            self.write(
                root,
                ".github/workflows/rust-release.yml",
                "\n".join(
                    [
                        'binaries: "codex syndrid codex-code-mode-host"',
                        "--bundle syndrid",
                        "Create GitHub Release",
                        "npm publish",
                        "microsoft/winget-pkgs",
                        "git push origin HEAD:main",
                    ]
                ),
            )

            result = contract.audit_release_contract(root)

            self.assertFalse(result["ok"])
            self.assertEqual(
                {finding["needle"] for finding in result["blockers"]},
                {"npm publish", "microsoft/winget-pkgs", "git push origin HEAD:main"},
            )
            self.assertEqual(result["missing_required_invariants"], [])

    def test_missing_github_release_syndrid_binary_or_package_is_reported(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.seed_safe_contract(root)
            self.write(root, ".github/workflows/rust-release.yml", "name: rust-release\n")
            self.write(root, ".github/workflows/rust-release-windows.yml", "name: rust-release-windows\n")

            result = contract.audit_release_contract(root)

            self.assertFalse(result["ok"])
            self.assertEqual(len(result["missing_required_invariants"]), 4)

    def test_missing_canonical_syndrid_package_archive_is_reported(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.seed_safe_contract(root)
            self.write(
                root,
                ".github/workflows/rust-release.yml",
                'binaries: "codex syndrid codex-code-mode-host"\nCreate GitHub Release\n',
            )

            result = contract.audit_release_contract(root)

            self.assertFalse(result["ok"])
            self.assertEqual(
                [finding["needle"] for finding in result["missing_required_invariants"]],
                ["--bundle syndrid"],
            )
            self.assertEqual(
                [finding["path"] for finding in result["missing_required_invariants"]],
                [".github/workflows/rust-release.yml"],
            )

    def test_missing_windows_canonical_syndrid_package_archive_is_reported(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.seed_safe_contract(root)
            self.write(
                root,
                ".github/workflows/rust-release-windows.yml",
                "name: rust-release-windows\n",
            )

            result = contract.audit_release_contract(root)

            self.assertFalse(result["ok"])
            self.assertEqual(
                [finding["needle"] for finding in result["missing_required_invariants"]],
                ["--bundle syndrid"],
            )
            self.assertEqual(
                [finding["path"] for finding in result["missing_required_invariants"]],
                [".github/workflows/rust-release-windows.yml"],
            )


class SyndridReleaseSmokeTests(unittest.TestCase):
    def make_fake_binary(self, root: Path, version: str = "0.1.0") -> Path:
        binary = root / "syndrid"
        binary.write_text(
            "#!/bin/sh\n"
            "case \"$1\" in\n"
            "  --help) printf 'Usage: codex [OPTIONS]\\n' ;;\n"
            f"  --version) printf 'codex-cli {version}\\n' ;;\n"
            "  *) exit 64 ;;\n"
            "esac\n",
            encoding="utf-8",
        )
        binary.chmod(0o755)
        return binary

    def test_help_and_exact_version_smoke_pass(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            binary = self.make_fake_binary(Path(directory))

            result = smoke.smoke_release_binary(binary, expected_version="0.1.0")

            self.assertEqual(result["version"], "0.1.0")
            self.assertGreater(result["help_output_bytes"], 0)

    def test_mismatched_release_version_fails(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            binary = self.make_fake_binary(Path(directory), version="0.1.0")

            with self.assertRaisesRegex(
                RuntimeError,
                "does not match expected release version",
            ):
                smoke.smoke_release_binary(binary, expected_version="0.2.0")

    def test_invalid_expected_version_is_rejected_before_execution(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            binary = self.make_fake_binary(Path(directory))

            with self.assertRaisesRegex(ValueError, "expected_version"):
                smoke.smoke_release_binary(binary, expected_version="release-latest")


if __name__ == "__main__":
    unittest.main()