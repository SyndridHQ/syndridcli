#!/usr/bin/env python3

from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from codex_package.layout import build_package_dir
from codex_package.layout import validate_package_dir
from codex_package.targets import PACKAGE_VARIANTS
from codex_package.targets import PackageInputs
from codex_package.targets import TARGET_SPECS


class PackageLayoutTest(unittest.TestCase):
    def test_app_server_package_places_code_mode_host_beside_entrypoint(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            package_dir = root / "package"
            package_dir.mkdir()
            inputs = PackageInputs(
                entrypoint_bin=touch_executable(root / "codex-app-server"),
                code_mode_host_bin=touch_executable(root / "codex-code-mode-host"),
                rg_bin=touch_executable(root / "rg"),
                zsh_bin=None,
                bwrap_bin=touch_executable(root / "bwrap"),
                codex_command_runner_bin=None,
                codex_windows_sandbox_setup_bin=None,
            )

            build_package_dir(
                package_dir,
                "1.2.3",
                PACKAGE_VARIANTS["codex-app-server"],
                TARGET_SPECS["x86_64-unknown-linux-musl"],
                inputs,
            )
            validate_package_dir(
                package_dir,
                PACKAGE_VARIANTS["codex-app-server"],
                TARGET_SPECS["x86_64-unknown-linux-musl"],
                include_zsh=False,
            )

            self.assertTrue((package_dir / "bin" / "codex-code-mode-host").is_file())

    def test_syndrid_package_uses_syndrid_as_canonical_entrypoint(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            package_dir = root / "package"
            package_dir.mkdir()
            inputs = PackageInputs(
                entrypoint_bin=touch_executable(root / "syndrid"),
                code_mode_host_bin=touch_executable(root / "codex-code-mode-host"),
                rg_bin=touch_executable(root / "rg"),
                zsh_bin=None,
                bwrap_bin=touch_executable(root / "bwrap"),
                codex_command_runner_bin=None,
                codex_windows_sandbox_setup_bin=None,
            )

            build_package_dir(
                package_dir,
                "0.1.0",
                PACKAGE_VARIANTS["syndrid"],
                TARGET_SPECS["x86_64-unknown-linux-musl"],
                inputs,
            )
            validate_package_dir(
                package_dir,
                PACKAGE_VARIANTS["syndrid"],
                TARGET_SPECS["x86_64-unknown-linux-musl"],
                include_zsh=False,
            )

            self.assertTrue((package_dir / "bin" / "syndrid").is_file())
            self.assertFalse((package_dir / "bin" / "codex").exists())
            metadata = (package_dir / "codex-package.json").read_text(encoding="utf-8")
            self.assertIn('"variant": "syndrid"', metadata)
            self.assertIn('"entrypoint": "bin/syndrid"', metadata)

    def test_windows_syndrid_package_preserves_exe_entrypoint_and_resources(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            package_dir = root / "package"
            package_dir.mkdir()
            inputs = PackageInputs(
                entrypoint_bin=touch_executable(root / "syndrid.exe"),
                code_mode_host_bin=touch_executable(root / "codex-code-mode-host.exe"),
                rg_bin=touch_executable(root / "rg.exe"),
                zsh_bin=None,
                bwrap_bin=None,
                codex_command_runner_bin=touch_executable(
                    root / "codex-command-runner.exe"
                ),
                codex_windows_sandbox_setup_bin=touch_executable(
                    root / "codex-windows-sandbox-setup.exe"
                ),
            )

            build_package_dir(
                package_dir,
                "0.1.0",
                PACKAGE_VARIANTS["syndrid"],
                TARGET_SPECS["x86_64-pc-windows-msvc"],
                inputs,
            )
            validate_package_dir(
                package_dir,
                PACKAGE_VARIANTS["syndrid"],
                TARGET_SPECS["x86_64-pc-windows-msvc"],
                include_zsh=False,
            )

            self.assertTrue((package_dir / "bin" / "syndrid.exe").is_file())
            self.assertFalse((package_dir / "bin" / "codex.exe").exists())
            self.assertTrue(
                (package_dir / "codex-resources" / "codex-command-runner.exe").is_file()
            )
            self.assertTrue(
                (
                    package_dir
                    / "codex-resources"
                    / "codex-windows-sandbox-setup.exe"
                ).is_file()
            )
            metadata = (package_dir / "codex-package.json").read_text(encoding="utf-8")
            self.assertIn('"variant": "syndrid"', metadata)
            self.assertIn('"entrypoint": "bin/syndrid.exe"', metadata)


def touch_executable(path: Path) -> Path:
    path.touch(mode=0o755)
    return path


if __name__ == "__main__":
    unittest.main()
