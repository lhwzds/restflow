import json
import sys
import unittest
from pathlib import Path


sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import restflow
from restflow.skill import SkillRuntime, SkillRuntimeError, load_artifact


class FakeNativeRuntime:
    def __init__(self) -> None:
        self.calls: list[tuple[str, tuple[object, ...]]] = []

    def runtime_load_artifact_json(self, root: str) -> str:
        self.calls.append(("load", (root,)))
        skill_id = Path(root).name
        return json.dumps(
            {
                "schema_version": 1,
                "kind": "rust_binary",
                "id": skill_id,
                "name": "Echo",
                "version": "0.1.0",
                "entry": f"bin/{skill_id}",
            }
        )

    def runtime_list_skills_json(self, root: str | None) -> str:
        self.calls.append(("list", (root,)))
        return json.dumps(
            [
                {
                    "schema_version": 1,
                    "kind": "python_uv",
                    "id": "py-echo",
                    "name": "Python Echo",
                    "version": "0.1.0",
                    "entry": "skill.py",
                }
            ]
        )

    def runtime_build_skill_json(self, root: str, target_dir: str | None) -> str:
        self.calls.append(("build", (root, target_dir)))
        return self.runtime_load_artifact_json(root)

    def runtime_run_skill_json(self, root: str, input_json: str, timeout_seconds: int) -> str:
        self.calls.append(("run", (root, input_json, timeout_seconds)))
        return input_json

    def runtime_install_local_skill_json(
        self,
        source: str,
        root: str | None,
        skill_id: str | None,
        overwrite: bool,
    ) -> str:
        self.calls.append(("install", (source, root, skill_id, overwrite)))
        return json.dumps(
            {
                "schema_version": 1,
                "kind": "rust_binary",
                "id": skill_id or "echo",
                "name": "Echo",
                "version": "0.1.0",
                "entry": "bin/echo",
            }
        )


class SkillRuntimeTests(unittest.TestCase):
    def setUp(self) -> None:
        self.native = FakeNativeRuntime()
        sys.modules["restflow.restflow_native"] = self.native

    def tearDown(self) -> None:
        sys.modules.pop("restflow.restflow_native", None)

    def test_load_artifact_uses_native_runtime(self) -> None:
        artifact = load_artifact("/tmp/echo")

        self.assertEqual(artifact.id, "echo")
        self.assertEqual(artifact.protocol.transport, "stdio-json")
        self.assertEqual(self.native.calls, [("load", ("/tmp/echo",))])

    def test_load_artifact_wraps_native_errors(self) -> None:
        def fail(_root: str) -> str:
            raise RuntimeError("native failure")

        self.native.runtime_load_artifact_json = fail  # type: ignore[method-assign]

        with self.assertRaisesRegex(SkillRuntimeError, "native failure"):
            load_artifact("/tmp/echo")

    def test_skill_handle_runs_through_native_runtime(self) -> None:
        output = restflow.skill("/tmp/echo").call({"message": "hello"}, timeout=10)

        self.assertEqual(output, {"message": "hello"})
        self.assertEqual(
            self.native.calls,
            [
                ("load", ("/tmp/echo",)),
                ("run", ("/tmp/echo", '{"message":"hello"}', 10)),
            ],
        )

    def test_runtime_lists_installed_artifacts_through_native_runtime(self) -> None:
        artifacts = SkillRuntime(root="/tmp/skills").list()

        self.assertEqual([artifact.id for artifact in artifacts], ["py-echo"])
        self.assertEqual(self.native.calls, [("list", (str(Path("/tmp/skills").resolve()),))])

    def test_install_local_skill_uses_native_runtime(self) -> None:
        handle = restflow.install_local_skill(
            "/tmp/source",
            root="/tmp/skills",
            skill_id="custom-echo",
            overwrite=True,
        )

        self.assertEqual(handle.artifact.id, "custom-echo")
        self.assertEqual(
            self.native.calls,
            [
                (
                    "install",
                    ("/tmp/source", str(Path("/tmp/skills").resolve()), "custom-echo", True),
                ),
                ("load", (str(Path("/tmp/skills/custom-echo").resolve()),)),
            ],
        )

    def test_build_skill_uses_native_runtime(self) -> None:
        artifact = restflow.build_skill("/tmp/echo", target_dir="/tmp/target")

        self.assertEqual(artifact.id, "echo")
        self.assertEqual(
            self.native.calls,
            [
                ("build", ("/tmp/echo", "/tmp/target")),
                ("load", ("/tmp/echo",)),
            ],
        )

    def test_incomplete_native_runtime_reports_clear_error(self) -> None:
        sys.modules["restflow.restflow_native"] = object()

        with self.assertRaisesRegex(SkillRuntimeError, "native runtime is missing"):
            load_artifact("/tmp/echo")


if __name__ == "__main__":
    unittest.main()
