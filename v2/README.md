# RestFlow V2 Core Prototype

This directory is an isolated prototype for the next internal architecture.

It does not replace the current daemon, CLI, TUI, web app, or existing Rust
workspace. The goal is to validate shorter module names, cleaner boundaries, and
a Python API shape before migrating production code.

## Principles

- Keep the current product stable while prototyping.
- Use short module names.
- Give each module a narrow responsibility.
- Keep UI interaction out of runtime semantics.
- Keep durable storage out of the agent core.
- Keep Python bindings at the same module boundary as Rust APIs.

## Modules

```text
agent   agent loop and execution planning
bridge  legacy migration DTOs and import checks
skill   skill catalog, mentions, and per-turn capability planning
tool    tool trait and registry
run     Task/Run durable execution model
chat    sessions, turns, and messages
proto   CoreCommand, CoreResponse, and CoreSnapshot protocol types
server  command and JSON ingress for product shells
store   repository traits and backend contracts
model   providers, models, selectors, and runtime model specs
auth    secrets, auth profiles, and provider access policy
event   stream, trace, and telemetry event types
```

## Python Package Loop

The Python package is backed by the Rust `restflow-native` PyO3 module. Use the
packaging helper so local installs and release wheels use the same settings:

```bash
python3 -m pip install maturin
python3 v2/python/scripts/package.py develop
python3 v2/python/scripts/package.py smoke
python3 v2/python/scripts/package.py build
```

The helper sets `PYO3_PYTHON` and keeps Cargo artifacts under
`/tmp/restflow-v2-python-target` by default, which avoids executing Rust build
artifacts from an external macOS volume.

## Non-Goals

- Do not run the existing daemon from here.
- Do not open or migrate production databases.
- Do not duplicate the current TUI/Web implementation.
- Do not expose production storage writes through Python.
- Do not depend on this prototype from current production crates yet.
