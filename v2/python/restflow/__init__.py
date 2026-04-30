"""Prototype Python API for the RestFlow V2 core."""

__version__ = "0.1.0"

from .bridge import (
    BridgeChatTurn,
    BridgeMessage,
    BridgeModelRef,
    BridgeModelSpec,
    BridgeProfile,
    BridgeRun,
    BridgeSession,
    BridgeSkill,
    BridgeSnapshot,
    BridgeTask,
    BridgeToolCall,
    BridgeToolSpec,
)
from .core import Core, CoreCommand, CoreResponse, CoreSnapshot

__all__ = [
    "BridgeChatTurn",
    "BridgeMessage",
    "BridgeModelRef",
    "BridgeModelSpec",
    "BridgeProfile",
    "BridgeRun",
    "BridgeSession",
    "BridgeSkill",
    "BridgeSnapshot",
    "BridgeTask",
    "BridgeToolCall",
    "BridgeToolSpec",
    "Core",
    "CoreCommand",
    "CoreResponse",
    "CoreSnapshot",
]
