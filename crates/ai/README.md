# AI

`ai` is the Rust agent framework crate. It owns the ReAct executor, provider
clients, tool interfaces, context management, steering, and telemetry domain.

Python execution and Python-defined tools are intentionally outside this crate.
Expose those capabilities through skrun-compatible examples or external
commands instead of adding a Python package or native extension boundary here.
