from __future__ import annotations

import importlib.util
from pathlib import Path
import tempfile
import unittest

REPO_ROOT = Path(__file__).resolve().parents[2]


def load_contract():
    path = REPO_ROOT / ".github/scripts/check_syndrid_release_contract.py"
    spec = importlib.util.spec_from_file_location(
        "syndrid_release_contract_side_effects", path
    )
    if spec is None or spec.loader is None:
        raise RuntimeError("could not load Syndrid release contract checker")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


contract = load_contract()


class SyndridReleaseTagSideEffectTests(unittest.TestCase):
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
        self.write(
            root, ".github/workflows/rust-release-prepare.yml", "name: prepare\n"
        )
        self.write(root, "codex-cli/package.json", '{"name":"syndrid"}\n')
        self.write(root, "scripts/install/install.sh", "#!/bin/sh\n")
        self.write(root, "scripts/install/install.ps1", "# syndrid installer\n")

    def test_dotslash_publication_is_a_pre_tag_blocker(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.seed_safe_contract(root)
            path = root / ".github/workflows/rust-release.yml"
            path.write_text(
                path.read_text(encoding="utf-8") + "publish-dotslash:\n",
                encoding="utf-8",
            )

            result = contract.audit_release_contract(root)

            self.assertFalse(result["ok"])
            self.assertEqual(
                [finding["needle"] for finding in result["blockers"]],
                ["publish-dotslash:"],
            )
            self.assertEqual(result["missing_required_invariants"], [])

    def test_inherited_argument_comment_lint_assets_are_a_pre_tag_blocker(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.seed_safe_contract(root)
            path = root / ".github/workflows/rust-release.yml"
            path.write_text(
                path.read_text(encoding="utf-8")
                + "argument-comment-lint-release-assets:\n",
                encoding="utf-8",
            )

            result = contract.audit_release_contract(root)

            self.assertFalse(result["ok"])
            self.assertEqual(
                [finding["needle"] for finding in result["blockers"]],
                ["argument-comment-lint-release-assets:"],
            )
            self.assertEqual(result["missing_required_invariants"], [])

    def test_inherited_windows_release_artifacts_are_pre_tag_blockers(self) -> None:
        inherited_artifacts = (
            "Build Python runtime wheel",
            "--bundle primary",
            "--bundle app-server",
            "Build Windows symbols",
        )
        for marker in inherited_artifacts:
            with self.subTest(marker=marker):
                with tempfile.TemporaryDirectory() as directory:
                    root = Path(directory)
                    self.seed_safe_contract(root)
                    path = root / ".github/workflows/rust-release-windows.yml"
                    path.write_text(
                        path.read_text(encoding="utf-8") + marker + "\n",
                        encoding="utf-8",
                    )

                    result = contract.audit_release_contract(root)

                    self.assertFalse(result["ok"])
                    self.assertEqual(
                        [finding["needle"] for finding in result["blockers"]],
                        [marker],
                    )
                    self.assertEqual(result["missing_required_invariants"], [])

    def test_unavailable_self_hosted_release_runners_are_pre_tag_blockers(self) -> None:
        inherited_runners = (
            ("rust-release.yml", "linux-x64-xl"),
            ("rust-release.yml", "linux-arm64"),
            ("rust-release-windows.yml", "event.repository.name }}-runners"),
        )
        for relative_path, marker in inherited_runners:
            with self.subTest(marker=marker):
                with tempfile.TemporaryDirectory() as directory:
                    root = Path(directory)
                    self.seed_safe_contract(root)
                    path = root / ".github/workflows" / relative_path
                    path.write_text(
                        path.read_text(encoding="utf-8") + marker + "\n",
                        encoding="utf-8",
                    )

                    result = contract.audit_release_contract(root)

                    self.assertFalse(result["ok"])
                    self.assertEqual(
                        [finding["needle"] for finding in result["blockers"]],
                        [marker],
                    )
                    self.assertEqual(result["missing_required_invariants"], [])

    def test_latest_alpha_force_update_is_a_pre_tag_blocker(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.seed_safe_contract(root)
            path = root / ".github/workflows/rust-release.yml"
            path.write_text(
                path.read_text(encoding="utf-8")
                + "repos/${GITHUB_REPOSITORY}/git/refs/heads/latest-alpha-cli\n"
                + "-F force=true\n",
                encoding="utf-8",
            )

            result = contract.audit_release_contract(root)

            self.assertFalse(result["ok"])
            self.assertEqual(
                [finding["needle"] for finding in result["blockers"]],
                ["repos/${GITHUB_REPOSITORY}/git/refs/heads/latest-alpha-cli"],
            )
            self.assertEqual(result["missing_required_invariants"], [])

    def test_release_asset_overwrite_is_a_pre_tag_blocker(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.seed_safe_contract(root)
            path = root / ".github/workflows/rust-release.yml"
            path.write_text(
                path.read_text(encoding="utf-8") + "overwrite_files: true\n",
                encoding="utf-8",
            )

            result = contract.audit_release_contract(root)

            self.assertFalse(result["ok"])
            self.assertEqual(
                [finding["needle"] for finding in result["blockers"]],
                ["overwrite_files: true"],
            )
            self.assertEqual(result["missing_required_invariants"], [])


if __name__ == "__main__":
    unittest.main()
