"""Prototype Python API for the RestFlow V2 core."""

from __future__ import annotations

__version__ = "0.1.0"

from .bridge import (
    BridgeChatTurn,
    BridgeMessage,
    BridgeModelRef,
    BridgeModelSpec,
    BridgeProfile,
    BridgeRole,
    BridgeRun,
    BridgeRunTask,
    BridgeSession,
    BridgeSkill,
    BridgeSkillSource,
    BridgeSnapshot,
    BridgeStatus,
    BridgeTask,
    BridgeToolCall,
    BridgeToolSpec,
)
from .core import (
    CallableTransport,
    CoreClient,
    CoreCommand,
    CoreResponse,
    CoreSnapshot,
    CoreTransport,
    InMemoryCoreHarness,
    NativeTransport,
    load_native_transport,
)
from .migrate import (
    MigrationIssue,
    MigrationReport,
    core_from_bridge_snapshot,
    import_bridge_snapshot,
    inspect_bridge_snapshot,
    replace_bridge_snapshot,
)
from .store import MemoryStore, Store

__all__ = [
    "BridgeChatTurn",
    "BridgeMessage",
    "BridgeModelRef",
    "BridgeModelSpec",
    "BridgeProfile",
    "BridgeRole",
    "BridgeRun",
    "BridgeRunTask",
    "BridgeSession",
    "BridgeSkill",
    "BridgeSkillSource",
    "BridgeSnapshot",
    "BridgeStatus",
    "BridgeTask",
    "BridgeToolCall",
    "BridgeToolSpec",
    "CallableTransport",
    "CoreClient",
    "CoreCommand",
    "CoreResponse",
    "CoreSnapshot",
    "CoreTransport",
    "InMemoryCoreHarness",
    "NativeTransport",
    "MigrationIssue",
    "MigrationReport",
    "MemoryStore",
    "Store",
    "core_from_bridge_snapshot",
    "import_bridge_snapshot",
    "inspect_bridge_snapshot",
    "load_native_transport",
    "replace_bridge_snapshot",
]
