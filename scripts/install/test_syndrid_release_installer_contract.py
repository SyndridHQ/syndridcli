from __future__ import annotations

import hashlib
import importlib.util
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import textwrap
import unittest

REPO_ROOT = Path(__file__).resolve().parents[2]


def load_contract():
    script_path = REPO_ROOT / ".github/scripts/check_syndrid_release_contract.py"
    spec = importlib.util.spec_from_file_location(
        "syndrid_release_contract_installers", script_path
    )
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
        self.write(
            root, ".github/workflows/rust-release-prepare.yml", "name: prepare\n"
        )
        self.write(root, "codex-cli/package.json", '{"name":"syndrid"}\n')
        self.write(
            root,
            "scripts/install/install.sh",
            "#!/bin/sh\n"
            'BIN_PATH="$BIN_DIR/syndrid"\n'
            'package_asset="syndrid-package-$vendor_target.tar.gz"\n'
            'checksum_asset="syndrid-package_SHA256SUMS"\n',
        )
        self.write(
            root,
            "scripts/install/install.ps1",
            '$SyndridPath = Join-Path $StandaloneCurrentDir "bin\\syndrid.exe"\n'
            '$packageAsset = "syndrid-package-$target.tar.gz"\n'
            '$checksumAsset = "syndrid-package_SHA256SUMS"\n',
        )

    def assert_safe(self, root: Path) -> None:
        result = contract.audit_release_contract(root)
        self.assertTrue(result["ok"])
        self.assertEqual(result["blockers"], [])
        self.assertEqual(result["missing_required_invariants"], [])

    def test_syndrid_owned_installer_entrypoints_and_package_consumers_are_not_blocked(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.seed_safe_contract(root)
            self.assert_safe(root)

    def test_inactive_legacy_unix_entrypoint_is_not_a_tag_blocker(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.seed_safe_contract(root)
            self.write(
                root,
                "scripts/install/install.sh",
                "#!/bin/sh\n"
                'BIN_PATH="$BIN_DIR/codex"\n'
                'package_asset="syndrid-package-$vendor_target.tar.gz"\n'
                'checksum_asset="syndrid-package_SHA256SUMS"\n',
            )
            self.assert_safe(root)

    def test_inactive_legacy_windows_entrypoint_is_not_a_tag_blocker(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.seed_safe_contract(root)
            self.write(
                root,
                "scripts/install/install.ps1",
                '$CodexPath = Join-Path $StandaloneCurrentDir "bin\\codex.exe"\n'
                '$packageAsset = "syndrid-package-$target.tar.gz"\n'
                '$checksumAsset = "syndrid-package_SHA256SUMS"\n',
            )
            self.assert_safe(root)

    def test_inactive_legacy_unix_package_consumers_are_not_tag_blockers(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.seed_safe_contract(root)
            self.write(
                root,
                "scripts/install/install.sh",
                "#!/bin/sh\n"
                'BIN_PATH="$BIN_DIR/syndrid"\n'
                'package_asset="codex-package-$vendor_target.tar.gz"\n'
                'checksum_asset="codex-package_SHA256SUMS"\n',
            )
            self.assert_safe(root)

    def test_inactive_legacy_windows_package_consumers_are_not_tag_blockers(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.seed_safe_contract(root)
            self.write(
                root,
                "scripts/install/install.ps1",
                '$SyndridPath = Join-Path $StandaloneCurrentDir "bin\\syndrid.exe"\n'
                '$packageAsset = "codex-package-$target.tar.gz"\n'
                '$checksumAsset = "codex-package_SHA256SUMS"\n',
            )
            self.assert_safe(root)


class SyndridUnixInstallerRuntimeTests(unittest.TestCase):
    def write_executable(self, path: Path, contents: str) -> None:
        path.write_text(contents, encoding="utf-8")
        path.chmod(0o755)

    def test_upgrade_replaces_current_release_symlink(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fake_bin = root / "fake-bin"
            fake_bin.mkdir()

            fake_curl = fake_bin / "curl"
            self.write_executable(
                fake_curl,
                textwrap.dedent(
                    """\
                    #!/bin/sh
                    url=""
                    output=""
                    previous=""
                    for arg in "$@"; do
                      case "$arg" in
                        https://*) url="$arg" ;;
                      esac
                      if [ "$previous" = "-o" ]; then
                        output="$arg"
                      fi
                      previous="$arg"
                    done

                    case "$url" in
                      */syndrid-package_SHA256SUMS)
                        cp "$SYNDRID_TEST_MANIFEST" "$output"
                        ;;
                      */syndrid-package-*.tar.gz)
                        cp "$SYNDRID_TEST_ARCHIVE" "$output"
                        ;;
                      *)
                        exit 22
                        ;;
                    esac
                    """
                ),
            )
            self.write_executable(
                fake_bin / "uname",
                "#!/bin/sh\n"
                'case "$1" in\n'
                "  -s) printf 'Linux\\n' ;;\n"
                "  -m) printf 'x86_64\\n' ;;\n"
                "  *) exit 64 ;;\n"
                "esac\n",
            )

            package = root / "package"
            (package / "bin").mkdir(parents=True)
            self.write_executable(package / "bin/syndrid", "#!/bin/sh\nexit 0\n")
            (package / "codex-package.json").write_text("{}\n", encoding="utf-8")

            asset = "syndrid-package-x86_64-unknown-linux-musl.tar.gz"
            archive_path = root / asset
            with tarfile.open(archive_path, "w:gz") as archive:
                for child in package.iterdir():
                    archive.add(child, arcname=child.name)

            digest = hashlib.sha256(archive_path.read_bytes()).hexdigest()
            manifest_path = root / "syndrid-package_SHA256SUMS"
            manifest_path.write_text(
                f"{digest}  {asset}\n",
                encoding="utf-8",
            )

            syndrid_home = root / "syndrid-home"
            install_dir = root / "install-bin"
            env = os.environ.copy()
            env.update(
                {
                    "HOME": str(root / "home"),
                    "PATH": f"{fake_bin}:/usr/bin:/bin",
                    "SYNDRID_HOME": str(syndrid_home),
                    "SYNDRID_INSTALL_DIR": str(install_dir),
                    "SYNDRID_TEST_ARCHIVE": str(archive_path),
                    "SYNDRID_TEST_MANIFEST": str(manifest_path),
                }
            )

            def install(version: str) -> subprocess.CompletedProcess[str]:
                run_env = env.copy()
                run_env["SYNDRID_RELEASE"] = version
                return subprocess.run(
                    ["/bin/sh", str(REPO_ROOT / "scripts/install/install-syndrid.sh")],
                    capture_output=True,
                    check=False,
                    env=run_env,
                    text=True,
                )

            first = install("0.1.0")
            self.assertEqual(first.returncode, 0, first.stderr)

            current = syndrid_home / "packages/standalone/current"
            first_release = (
                syndrid_home
                / "packages/standalone/releases"
                / "rust-v0.1.0-x86_64-unknown-linux-musl"
            )
            self.assertEqual(os.readlink(current), str(first_release))

            second = install("0.1.1")
            self.assertEqual(second.returncode, 0, second.stderr)

            second_release = (
                syndrid_home
                / "packages/standalone/releases"
                / "rust-v0.1.1-x86_64-unknown-linux-musl"
            )
            self.assertEqual(os.readlink(current), str(second_release))
            self.assertTrue((current / "bin/syndrid").is_file())


if __name__ == "__main__":
    unittest.main()
