import importlib.util
import sys
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

    def test_build_env_sets_python_and_target_dir_without_overriding_env(self) -> None:
        env = package_script.build_env("/usr/bin/python3", Path("/tmp/restflow-target"))

        self.assertEqual(env["PYO3_PYTHON"], "/usr/bin/python3")
        self.assertEqual(env["CARGO_TARGET_DIR"], "/tmp/restflow-target")


if __name__ == "__main__":
    unittest.main()
