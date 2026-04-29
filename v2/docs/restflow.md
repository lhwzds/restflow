# restflow

Facade module that exposes the V2 kernel as one public Rust API.

Path: `crates/restflow`

Owns:
- public module re-exports
- kernel API entrypoint
- stable import shape for examples

Must Not:
- own runtime behavior
- own persistence
- duplicate module logic

Inputs:
- kernel modules

Outputs:
- unified Rust API surface

Depends On:
- agent
- auth
- chat
- event
- model
- run
- skill
- store
- tool

Verify:
- cargo check -p restflow-v2

