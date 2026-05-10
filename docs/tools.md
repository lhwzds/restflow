---
title: tools
covers:
  - crates/tools/Cargo.toml
  - crates/tools/src/lib.rs
---

# tools

`tools` owns built-in local tool implementations.

## Responsibilities

- Shell, file, edit, multi-edit, patch, glob, and grep tools.
- Skill tools: `load_skill` for guidance and `run_skill` for executable skrun
  skills.
- Sub-agent management tools that call the sub-agent manager interface.
- Tool registry builders and security wrappers.

## Boundaries

- Tool traits and shared schemas belong in `types`.
- Runtime service adapters belong in `runner` or `core`.
- New broad capabilities should usually be skrun skills, not built-in Rust
  tools.
