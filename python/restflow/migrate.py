"""Migration helpers for importing bridge DTOs into the Python harness."""

from __future__ import annotations

from dataclasses import dataclass, field

from .bridge import BridgeSnapshot
from .core import InMemoryCoreHarness


@dataclass
class MigrationIssue:
    kind: str
    message: str


@dataclass
class MigrationReport:
    applied: bool = False
    models: int = 0
    skills: int = 0
    sessions: int = 0
    messages: int = 0
    tasks: int = 0
    runs: int = 0
    profiles: int = 0
    tool_specs: int = 0
    issues: list[MigrationIssue] = field(default_factory=list)

    def is_clean(self) -> bool:
        return not self.issues

    def has_blocking_issues(self) -> bool:
        return any(
            issue.kind
            in {
                "current_model_missing_from_catalog",
                "run_references_missing_task",
                "run_references_missing_session",
                "task_references_missing_session",
                "run_session_differs_from_task",
                "duplicate_id",
                "existing_record",
            }
            for issue in self.issues
        )


def core_from_bridge_snapshot(
    snapshot: BridgeSnapshot,
) -> tuple[InMemoryCoreHarness, MigrationReport]:
    core = InMemoryCoreHarness(model=snapshot.current_model.to_model())
    report = import_bridge_snapshot(core, snapshot)
    return core, report


def import_bridge_snapshot(
    core: InMemoryCoreHarness, snapshot: BridgeSnapshot
) -> MigrationReport:
    report = inspect_bridge_snapshot(snapshot)
    report.issues.extend(_existing_record_issues(core, snapshot))
    if report.has_blocking_issues():
        return report

    _apply_bridge_snapshot(core, snapshot)
    report.applied = True
    return report


def replace_bridge_snapshot(
    core: InMemoryCoreHarness, snapshot: BridgeSnapshot
) -> MigrationReport:
    report = inspect_bridge_snapshot(snapshot)
    if report.has_blocking_issues():
        return report

    _reset_core_records(core)
    _apply_bridge_snapshot(core, snapshot)
    report.applied = True
    return report


def _apply_bridge_snapshot(core: InMemoryCoreHarness, snapshot: BridgeSnapshot) -> None:
    core.switch_model(snapshot.current_model.to_model())
    existing_models = {
        (spec.model.provider.id, spec.model.id): spec for spec in core.models
    }
    existing_models.update(
        {
            (spec.provider, spec.model): spec.to_model_spec()
            for spec in snapshot.models
        }
    )
    core.models = list(existing_models.values())
    for skill in snapshot.skills:
        core.skills.put(skill.id, skill.to_skill())
    for session in snapshot.sessions:
        core.sessions.put(session.id, session.to_session())
    for task in snapshot.tasks:
        core.tasks.put(task.id, task.to_task())
    for run in snapshot.runs:
        core.runs.put(run.id, run.to_run())
    for profile in snapshot.profiles:
        core.profiles.put(profile.provider, profile.to_profile())


def inspect_bridge_snapshot(snapshot: BridgeSnapshot) -> MigrationReport:
    model_exists = any(
        model.provider == snapshot.current_model.provider
        and model.model == snapshot.current_model.model
        for model in snapshot.models
    )
    task_ids = {task.id for task in snapshot.tasks}
    task_sessions = {
        task.id: task.session_id for task in snapshot.tasks if task.session_id is not None
    }
    session_ids = {session.id for session in snapshot.sessions}
    issues: list[MigrationIssue] = []
    _record_duplicate_ids(
        [f"{model.provider}:{model.model}" for model in snapshot.models],
        "model",
        issues,
    )
    _record_duplicate_ids([skill.id for skill in snapshot.skills], "skill", issues)
    _record_duplicate_ids([session.id for session in snapshot.sessions], "session", issues)
    _record_duplicate_ids([task.id for task in snapshot.tasks], "task", issues)
    _record_duplicate_ids([run.id for run in snapshot.runs], "run", issues)
    _record_duplicate_ids([profile.provider for profile in snapshot.profiles], "profile", issues)

    if not model_exists:
        issues.append(
            MigrationIssue(
                kind="current_model_missing_from_catalog",
                message=(
                    "current model is not listed in the model catalog: "
                    f"{snapshot.current_model.provider}:{snapshot.current_model.model}"
                ),
            )
        )

    for task in snapshot.tasks:
        if task.session_id is not None and task.session_id not in session_ids:
            issues.append(
                MigrationIssue(
                    kind="task_references_missing_session",
                    message=(
                        "task references a session that is not present in the snapshot: "
                        f"{task.id} -> {task.session_id}"
                    ),
                )
            )

    for run in snapshot.runs:
        if run.task_id not in task_ids:
            issues.append(
                MigrationIssue(
                    kind="run_references_missing_task",
                    message=(
                        "run references a task that is not present in the snapshot: "
                        f"{run.id} -> {run.task_id}"
                    ),
                )
            )
        task_session_id = task_sessions.get(run.task_id)
        if (
            run.session_id is not None
            and task_session_id is not None
            and run.session_id != task_session_id
        ):
            issues.append(
                MigrationIssue(
                    kind="run_session_differs_from_task",
                    message=(
                        "run session does not match the bound task session: "
                        f"{run.id} -> run {run.session_id}, task {task_session_id}"
                    ),
                )
            )
        if run.session_id is not None and run.session_id not in session_ids:
            issues.append(
                MigrationIssue(
                    kind="run_references_missing_session",
                    message=(
                        "run references a session that is not present in the snapshot: "
                        f"{run.id} -> {run.session_id}"
                    ),
                )
            )

    if snapshot.observed_tool_specs:
        issues.append(
            MigrationIssue(
                kind="tool_specs_are_informational",
                message=(
                    "tool specs were recorded for reporting but concrete tool "
                    "implementations are not imported"
                ),
            )
        )

    return MigrationReport(
        models=len(snapshot.models),
        skills=len(snapshot.skills),
        sessions=len(snapshot.sessions),
        messages=sum(len(session.messages) for session in snapshot.sessions),
        tasks=len(snapshot.tasks),
        runs=len(snapshot.runs),
        profiles=len(snapshot.profiles),
        tool_specs=len(snapshot.observed_tool_specs),
        issues=issues,
    )


def _reset_core_records(core: InMemoryCoreHarness) -> None:
    core.models.clear()
    core.skills.replace_all([])
    core.sessions.replace_all([])
    core.tasks.replace_all([])
    core.runs.replace_all([])
    core.profiles.replace_all([])


def _existing_record_issues(
    core: InMemoryCoreHarness, snapshot: BridgeSnapshot
) -> list[MigrationIssue]:
    issues: list[MigrationIssue] = []
    model_keys = {(model.model.provider.id, model.model.id) for model in core.models}
    for model in snapshot.models:
        if (model.provider, model.model) in model_keys:
            issues.append(_existing_record("model", f"{model.provider}:{model.model}"))
    for skill in snapshot.skills:
        if core.skills.exists(skill.id):
            issues.append(_existing_record("skill", skill.id))
    for session in snapshot.sessions:
        if core.sessions.exists(session.id):
            issues.append(_existing_record("session", session.id))
    for task in snapshot.tasks:
        if core.tasks.exists(task.id):
            issues.append(_existing_record("task", task.id))
    for run in snapshot.runs:
        if core.runs.exists(run.id):
            issues.append(_existing_record("run", run.id))
    for profile in snapshot.profiles:
        if core.profiles.exists(profile.provider):
            issues.append(_existing_record("profile", profile.provider))
    return issues


def _existing_record(domain: str, id: str) -> MigrationIssue:
    return MigrationIssue(
        kind="existing_record",
        message=f"{domain} already exists and create-only import will not overwrite it: {id}",
    )


def _record_duplicate_ids(ids: list[str], domain: str, issues: list[MigrationIssue]) -> None:
    seen: set[str] = set()
    reported: set[str] = set()
    for id in ids:
        if id in seen and id not in reported:
            reported.add(id)
            issues.append(
                MigrationIssue(
                    kind="duplicate_id",
                    message=f"snapshot contains duplicate {domain} id: {id}",
                )
            )
        seen.add(id)
