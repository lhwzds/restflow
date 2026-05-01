import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT_PATH = Path(__file__).resolve().parents[1] / "scripts" / "package.py"
SPEC = importlib.util.spec_from_file_location("restflow_package_script", SCRIPT_PATH)
assert SPEC is not None
assert SPEC.loader is not None
package_script = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = package_script
SPEC.loader.exec_module(package_script)


class PackageScriptTests(unittest.TestCase):
    def test_maturin_build_command_uses_release_and_dist_dir(self) -> None:
        command = package_script.maturin_command(
            "build", release=True, dist_dir=Path("/tmp/wheels")
        )

        self.assertEqual(
            command,
            [
                sys.executable,
                "-m",
                "maturin",
                "build",
                "--release",
                "--out",
                "/tmp/wheels",
            ],
        )

    def test_maturin_develop_command_can_use_debug_build(self) -> None:
        command = package_script.maturin_command("develop", release=False, dist_dir=None)

        self.assertEqual(command, [sys.executable, "-m", "maturin", "develop"])

    def test_smoke_runs_outside_python_source_tree(self) -> None:
        calls: list[tuple[list[str], Path]] = []

        def fake_run(command: list[str], env: dict[str, str], cwd: Path) -> None:
            calls.append((command, cwd))

        original_run = package_script.run
        package_script.run = fake_run
        try:
            package_script.smoke({})
        finally:
            package_script.run = original_run

        self.assertEqual(calls[0][1], Path("/tmp"))
        smoke_code = calls[0][0][-1]
        self.assertIn("restflow.skill(root).call", smoke_code)
        self.assertIn("artifact.json", smoke_code)

    def test_build_env_sets_python_and_target_dir_without_overriding_env(self) -> None:
        env = package_script.build_env("/usr/bin/python3", Path("/tmp/restflow-target"))

        self.assertEqual(env["PYO3_PYTHON"], "/usr/bin/python3")
        self.assertEqual(env["CARGO_TARGET_DIR"], "/tmp/restflow-target")

    def test_build_env_sets_virtual_env_for_venv_python(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            (root / "pyvenv.cfg").write_text("", encoding="utf-8")
            bin_dir = root / "bin"
            bin_dir.mkdir()
            python = bin_dir / "python"
            python.touch()

            env = package_script.build_env(str(python), Path("/tmp/restflow-target"))

        self.assertEqual(env["VIRTUAL_ENV"], str(root.resolve()))


if __name__ == "__main__":
    unittest.main()
