"""Durable task and run data models."""

from __future__ import annotations

from dataclasses import dataclass


@dataclass
class Task:
    id: str
    title: str
    input: str | None = None
    agent_id: str | None = None
    session_id: str | None = None
    status: str | None = None
    schedule: str | None = None
    created_at: str | None = None
    updated_at: str | None = None
    error: str | None = None


@dataclass
class Run:
    id: str
    task_id: str
    status: str
    raw_status: str | None = None
    session_id: str | None = None
    execution_id: str | None = None
    checkpoint_id: str | None = None
    error: str | None = None
    started_at: str | None = None
    updated_at: str | None = None
    ended_at: str | None = None
