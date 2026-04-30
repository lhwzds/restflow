import json
import sys
import unittest
from pathlib import Path


sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from restflow import (
    BridgeChatTurn,
    CallableTransport,
    CoreClient,
    CoreCommand,
    CoreResponse,
    InMemoryCoreHarness,
    NativeTransport,
)
from restflow.model import Model, Provider
from restflow.skill import Skill


class CoreContractTests(unittest.TestCase):
    def test_command_json_uses_rust_tagged_enum_shape(self) -> None:
        command = CoreCommand(
            type="chat_turn",
            session_id="session-1",
            message="hello",
            assigned_skills=["team"],
        )

        self.assertEqual(
            json.loads(command.to_json()),
            {
                "type": "chat_turn",
                "session_id": "session-1",
                "message": "hello",
                "assigned_skills": ["team"],
            },
        )

    def test_payload_alias_still_serializes_without_payload_wrapper(self) -> None:
        command = BridgeChatTurn(
            session_id="session-1",
            message="hello",
            assigned_skills=["team"],
        ).to_core_command()

        self.assertEqual(command.payload["message"], "hello")
        self.assertNotIn("payload", command.to_dict())
        self.assertEqual(command.to_dict()["assigned_skills"], ["team"])

    def test_type_field_is_reserved(self) -> None:
        with self.assertRaises(ValueError):
            CoreCommand(type="chat_turn", fields={"type": "switch_model"})
        with self.assertRaises(ValueError):
            CoreResponse(type="saved", payload={"type": "error"})

        command = CoreCommand(type="chat_turn", message="hello")
        with self.assertRaises(TypeError):
            command.payload["type"] = "switch_model"

        command.fields["type"] = "switch_model"
        with self.assertRaises(ValueError):
            command.to_json()

    def test_command_serializes_nested_dataclasses(self) -> None:
        command = CoreCommand(
            type="save_skill",
            skill=Skill(
                id="team",
                name="Team",
                source="system",
                read_only=True,
                suggested_tools=["spawn_agent"],
            ),
        )

        self.assertEqual(
            command.to_dict()["skill"],
            {
                "id": "team",
                "name": "Team",
                "source": "system",
                "source_ref": None,
                "read_only": True,
                "description": None,
                "content": "",
                "suggested_tools": ["spawn_agent"],
            },
        )

    def test_core_client_uses_json_transport_boundary(self) -> None:
        requests: list[dict[str, object]] = []

        def handler(command_json: str) -> str:
            requests.append(json.loads(command_json))
            return CoreResponse(
                type="model_switched",
                model=Model(provider=Provider(id="openai"), id="gpt-5.5"),
            ).to_json()

        client = CoreClient(CallableTransport(handler))
        response = client.handle(
            CoreCommand(
                type="switch_model",
                model=Model(provider=Provider(id="openai"), id="gpt-5.5"),
            )
        )

        self.assertEqual(
            requests,
            [
                {
                    "type": "switch_model",
                    "model": {"provider": {"id": "openai"}, "id": "gpt-5.5"},
                }
            ],
        )
        self.assertEqual(response.type, "model_switched")
        self.assertEqual(response.payload["model"]["id"], "gpt-5.5")

    def test_core_client_can_use_native_transport(self) -> None:
        class FakeNativeModule:
            def __init__(self) -> None:
                self.requests: list[dict[str, object]] = []

            def handle_json(self, command_json: str) -> str:
                self.requests.append(json.loads(command_json))
                return CoreResponse(type="saved").to_json()

        native = FakeNativeModule()
        client = CoreClient(NativeTransport(native))
        response = client.handle(CoreCommand(type="save_skill", skill={"id": "team"}))

        self.assertEqual(response.type, "saved")
        self.assertEqual(native.requests, [{"type": "save_skill", "skill": {"id": "team"}}])

    def test_core_client_native_loads_pyo3_module_by_name(self) -> None:
        class FakeNativeCore:
            def __init__(self) -> None:
                self.requests: list[dict[str, object]] = []

            def handle_json(self, command_json: str) -> str:
                self.requests.append(json.loads(command_json))
                return CoreResponse(type="saved").to_json()

        class FakeNativeModule:
            Core = FakeNativeCore

        module_name = "fake_restflow_native"
        sys.modules[module_name] = FakeNativeModule()
        try:
            first = CoreClient.native(module_name)
            second = CoreClient.native(module_name)
            first_response = first.handle(CoreCommand(type="save_skill", skill={"id": "team"}))
            second_response = second.handle(CoreCommand(type="save_skill", skill={"id": "browser"}))
        finally:
            del sys.modules[module_name]

        self.assertEqual(first_response.type, "saved")
        self.assertEqual(second_response.type, "saved")
        self.assertIsNot(first.transport.module, second.transport.module)

    def test_core_client_native_falls_back_to_module_handler(self) -> None:
        class FakeNativeModule:
            def handle_json(self, command_json: str) -> str:
                json.loads(command_json)
                return CoreResponse(type="saved").to_json()

        module_name = "fake_restflow_native_fallback"
        sys.modules[module_name] = FakeNativeModule()
        try:
            response = CoreClient.native(module_name).handle(CoreCommand(type="save_skill"))
        finally:
            del sys.modules[module_name]

        self.assertEqual(response.type, "saved")

    def test_top_level_exports_migration_harness(self) -> None:
        core = InMemoryCoreHarness(model=Model(provider=Provider(id="openai"), id="gpt-5.5"))

        self.assertEqual(core.snapshot().current_model.id, "gpt-5.5")


if __name__ == "__main__":
    unittest.main()
