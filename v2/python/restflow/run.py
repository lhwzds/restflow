"""Durable task and run API placeholders."""

from dataclasses import dataclass


@dataclass
class Task:
    id: str
    title: str


@dataclass
class Run:
    id: str
    task_id: str
    status: str

