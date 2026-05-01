# RestFlow V2 Skill Runtime

This directory is the new product direction for RestFlow: a small skill runtime
that lets agent frameworks discover, build, install, and call executable skills.
The legacy daemon, TUI, web app, team runtime, memory runtime, and workflow
surface are migration references, not V2 expansion targets.

## Principles

- Use short module names.
- Give each module a narrow responsibility.
- Keep UI interaction out of runtime semantics.
- Keep durable storage out of the executable skill runtime.
- Keep Python bindings at the same module boundary as Rust APIs.
- Support external agent frameworks instead of replacing their main agent loop.
- Make Rust binary skills and uv-backed Python skills first-class.

## Modules

```text
agent   agent loop and execution planning
bridge  legacy migration DTOs and import checks
skill   skill catalog, mentions, and per-turn capability planning
tool    tool trait and registry
runtime executable skill artifacts, scaffold, build, and run
cli     minimal V2 executable skill commands
run     Task/Run durable execution model
chat    sessions, turns, and messages
engine  core composition and command execution
proto   CoreCommand, CoreResponse, and CoreSnapshot protocol types
server  command and JSON ingress for product shells
store   repository traits and backend contracts
model   providers, models, selectors, and runtime model specs
auth    secrets, auth profiles, and provider access policy
event   stream, trace, and telemetry event types
```

## Executable Skills

V2 supports two local artifact kinds:

```text
rust_binary  Cargo-built executable skill, called with stdin/stdout JSON
python_uv    uv-managed Python project, called with stdin/stdout JSON
```

Each skill directory contains an `artifact.json` manifest:

```json
{
  "schema_version": 1,
  "kind": "rust_binary",
  "id": "regex-finder",
  "name": "Regex Finder",
  "version": "0.1.0",
  "entry": "bin/release/regex-finder",
  "protocol": {
    "transport": "stdio-json",
    "input": "single-json-value",
    "output": "single-json-value"
  },
  "schema": {
    "input": "schema/input.json",
    "output": "schema/output.json"
  }
}
```

The initial protocol is intentionally strict:

- input is one JSON value written to stdin
- output is one JSON object written to stdout
- stderr is diagnostics
- non-zero exit is failure
- streaming is not part of the first executable skill contract

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

## Python Skill API

The high-level Python API calls local executable skill artifacts through the
same Rust `runtime` crate used by the CLI:

```python
import restflow

result = restflow.skill("/path/to/skill").call({"pattern": "TODO", "path": "."})
```

By default, skill IDs resolve under `~/.restflow/v2/skills`, or under the
directory set by `RESTFLOW_SKILLS_DIR`. The Python package requires the
`restflow_native` PyO3 extension for executable skill operations; it does not
maintain a separate subprocess runtime fallback.

## CLI Skill Loop

The minimal V2 CLI exists only for executable skills:

```bash
cargo run --manifest-path v2/Cargo.toml -p cli -- \
  skill new --kind rust_binary --id rust-echo /tmp/rust-echo

cargo run --manifest-path v2/Cargo.toml -p cli -- \
  skill build /tmp/rust-echo

cargo run --manifest-path v2/Cargo.toml -p cli -- \
  skill run --input '{"ok":true}' /tmp/rust-echo

cargo run --manifest-path v2/Cargo.toml -p cli -- \
  skill install-local --root /tmp/restflow-skills --overwrite /tmp/rust-echo

cargo run --manifest-path v2/Cargo.toml -p cli -- \
  skill list --root /tmp/restflow-skills
```

Example skills live under `v2/examples/skills`.

## Non-Goals For This Slice

- Do not open or migrate production databases.
- Do not duplicate the current TUI/Web implementation.
- Do not implement a main agent framework.
- Do not implement a remote marketplace UI.
- Do not implement streaming executable skills.
