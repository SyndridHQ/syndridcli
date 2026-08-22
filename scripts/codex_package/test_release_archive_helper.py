#!/usr/bin/env python3

import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


REPO_ROOT = Path(__file__).resolve().parents[2]
ARCHIVE_HELPER = REPO_ROOT / ".github/scripts/build-codex-package-archive.sh"


class ReleaseArchiveHelperTest(unittest.TestCase):
    def run_syndrid_helper(self, target: str, entrypoint_name: str) -> tuple[list[str], Path]:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            workspace = root / "workspace"
            scripts_dir = workspace / "scripts"
            scripts_dir.mkdir(parents=True)
            capture_path = root / "args.json"
            fake_builder = scripts_dir / "build_codex_package.py"
            fake_builder.write_text(
                "import json, os, sys\n"
                "from pathlib import Path\n"
                "Path(os.environ['CAPTURE_ARGS']).write_text(json.dumps(sys.argv[1:]))\n",
                encoding="utf-8",
            )

            entrypoint_dir = root / "entrypoints"
            entrypoint_dir.mkdir()
            (entrypoint_dir / entrypoint_name).touch()
            archive_dir = root / "archives"
            runner_temp = root / "runner-temp"
            runner_temp.mkdir()

            env = os.environ.copy()
            env.update(
                {
                    "CAPTURE_ARGS": str(capture_path),
                    "GITHUB_WORKSPACE": str(workspace),
                    "RUNNER_TEMP": str(runner_temp),
                }
            )
            subprocess.run(
                [
                    "bash",
                    str(ARCHIVE_HELPER),
                    "--target",
                    target,
                    "--bundle",
                    "syndrid",
                    "--entrypoint-dir",
                    str(entrypoint_dir),
                    "--archive-dir",
                    str(archive_dir),
                ],
                check=True,
                env=env,
            )

            return json.loads(capture_path.read_text(encoding="utf-8")), archive_dir

    def assert_archive_outputs(self, args: list[str], archive_dir: Path, target: str) -> None:
        archive_outputs = [
            args[index + 1]
            for index, value in enumerate(args)
            if value == "--archive-output"
        ]
        self.assertEqual(
            archive_outputs,
            [
                str(archive_dir / f"syndrid-package-{target}.tar.gz"),
                str(archive_dir / f"syndrid-package-{target}.tar.zst"),
            ],
        )

    def test_syndrid_bundle_selects_syndrid_variant_entrypoint_and_archive_names(
        self,
    ) -> None:
        target = "x86_64-apple-darwin"
        args, archive_dir = self.run_syndrid_helper(target, "syndrid")

        self.assertEqual(args[args.index("--variant") + 1], "syndrid")
        self.assertEqual(
            Path(args[args.index("--entrypoint-bin") + 1]).name,
            "syndrid",
        )
        self.assert_archive_outputs(args, archive_dir, target)

    def test_windows_syndrid_bundle_selects_exe_entrypoint_and_archive_names(self) -> None:
        target = "x86_64-pc-windows-msvc"
        args, archive_dir = self.run_syndrid_helper(target, "syndrid.exe")

        self.assertEqual(args[args.index("--variant") + 1], "syndrid")
        self.assertEqual(
            Path(args[args.index("--entrypoint-bin") + 1]).name,
            "syndrid.exe",
        )
        self.assert_archive_outputs(args, archive_dir, target)


if __name__ == "__main__":
    unittest.main()
