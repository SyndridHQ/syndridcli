from __future__ import annotations

import importlib.util
from pathlib import Path
import tempfile
import unittest

REPO_ROOT = Path(__file__).resolve().parents[2]
SCRIPT_PATH = REPO_ROOT / ".github/scripts/check_syndrid_release_contract.py"

spec = importlib.util.spec_from_file_location(
    "syndrid_release_contract_publication_dependencies", SCRIPT_PATH
)
if spec is None or spec.loader is None:
    raise RuntimeError("could not load Syndrid release contract checker")
contract = importlib.util.module_from_spec(spec)
spec.loader.exec_module(contract)


class SyndridReleasePublicationDependencyTests(unittest.TestCase):
    def write(self, root: Path, relative_path: str, content: str) -> None:
        path = root / relative_path
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")

    def release_workflow(self) -> str:
        return (
            "name: rust-release\n"
            "concurrency:\n"
            "  group: rust-release-${{ github.ref_name }}\n"
            "  cancel-in-progress: false\n"
            'binaries: "codex syndrid codex-code-mode-host"\n'
            'verify_signed_binary "${package_dir}/bin/syndrid" "syndrid"\n'
            "syndrid-package-*.tar.gz\n"
            "Create GitHub Release\n"
            "files: dist/**\n"
            "jobs:\n"
            "  build:\n"
            "    steps:\n"
            "      - run: build-codex-package --bundle syndrid\n"
            "  finalize-macos:\n"
            "    steps:\n"
            "      - run: build-codex-package --bundle syndrid\n"
            "  release:\n"
            "    needs:\n"
            "      - tag-check\n"
            "      - build\n"
            "      - finalize-macos\n"
            "      - build-windows\n"
            "    if: >-\n"
            "      ${{\n"
            "        needs.tag-check.result == 'success' &&\n"
            "        needs.build.result == 'success' &&\n"
            "        needs.finalize-macos.result == 'success' &&\n"
            "        needs.build-windows.result == 'success'\n"
            "      }}\n"
        )

    def seed_contract(self, root: Path, workflow: str | None = None) -> None:
        self.write(
            root,
            ".github/workflows/rust-release.yml",
            workflow if workflow is not None else self.release_workflow(),
        )
        self.write(
            root,
            ".github/workflows/rust-release-windows.yml",
            "jobs:\n"
            "  build-windows:\n"
            "    steps:\n"
            "      - run: build-codex-package --bundle syndrid\n",
        )
        self.write(
            root, ".github/workflows/rust-release-prepare.yml", "name: prepare\n"
        )
        self.write(root, "codex-cli/package.json", '{"name":"syndrid"}\n')
        self.write(root, "scripts/install/install.sh", "#!/bin/sh\n")
        self.write(root, "scripts/install/install.ps1", "# syndrid installer\n")

    def invariant_needles(self, result: dict[str, object]) -> list[str]:
        return [finding["needle"] for finding in result["missing_required_invariants"]]

    def test_complete_publication_dependency_graph_is_accepted(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.seed_contract(root)

            result = contract.audit_release_contract(root)

            self.assertTrue(result["ok"])
            self.assertEqual(result["blockers"], [])
            self.assertEqual(result["missing_required_invariants"], [])

    def test_each_required_producer_must_remain_in_release_needs(self) -> None:
        for dependency in contract.RELEASE_PUBLICATION_DEPENDENCIES:
            with self.subTest(dependency=dependency):
                with tempfile.TemporaryDirectory() as directory:
                    root = Path(directory)
                    workflow = self.release_workflow().replace(
                        f"      - {dependency}\n", ""
                    )
                    self.seed_contract(root, workflow)

                    result = contract.audit_release_contract(root)

                    self.assertFalse(result["ok"])
                    self.assertIn(
                        f"release.needs:{dependency}",
                        self.invariant_needles(result),
                    )

    def test_each_required_producer_must_succeed_before_publication(self) -> None:
        for dependency in contract.RELEASE_PUBLICATION_DEPENDENCIES:
            with self.subTest(dependency=dependency):
                with tempfile.TemporaryDirectory() as directory:
                    root = Path(directory)
                    predicate = f"needs.{dependency}.result == 'success'"
                    workflow = self.release_workflow().replace(predicate, "true")
                    self.seed_contract(root, workflow)

                    result = contract.audit_release_contract(root)

                    self.assertFalse(result["ok"])
                    self.assertIn(predicate, self.invariant_needles(result))


if __name__ == "__main__":
    unittest.main()
