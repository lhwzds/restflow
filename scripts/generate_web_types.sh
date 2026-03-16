#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"

cd "${REPO_ROOT}"

# TS_RS_EXPORT_DIR is configured in .cargo/config.toml to point at web/src/types/generated.
# Run every crate that owns exported bindings so generated files stay in sync.
TYPEGEN_TARGET_DIR="${REPO_ROOT}/target/typegen"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-${TYPEGEN_TARGET_DIR}}"

cargo test -p restflow-traits --features ts --lib export_bindings -- --test-threads=1
cargo test -p restflow-core --lib export_bindings -- --test-threads=1
cargo test -p restflow-tools --features ts --lib export_bindings -- --test-threads=1
