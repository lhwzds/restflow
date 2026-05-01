#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
V2_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
REPO_ROOT="$(cd "${V2_ROOT}/.." && pwd)"

PYTHON_BIN="${PYTHON_BIN:-python3}"
PYTHON_VENV="${PYTHON_VENV:-/tmp/restflow-v2-python-venv}"
CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/tmp/restflow-v2-python-target}"
WHEEL_DIR="${WHEEL_DIR:-/tmp/restflow-v2-python-dist}"

export CARGO_TARGET_DIR
export CARGO_TERM_COLOR="${CARGO_TERM_COLOR:-always}"

venv_python() {
  printf '%s/bin/python' "${PYTHON_VENV}"
}

ensure_python_env() {
  if [ ! -x "$(venv_python)" ]; then
    "${PYTHON_BIN}" -m venv "${PYTHON_VENV}"
  fi
  "$(venv_python)" -m pip --version >/dev/null
  if ! "$(venv_python)" -m maturin --version >/dev/null 2>&1; then
    "$(venv_python)" -m pip install maturin
  fi
}

python_tests() {
  PYTHONDONTWRITEBYTECODE=1 "$(venv_python)" -m unittest discover -s "${REPO_ROOT}/v2/python/tests"
}

rust_native_checks() {
  cargo test --manifest-path "${REPO_ROOT}/v2/Cargo.toml" -p restflow-native --features python-module
  cargo clippy --manifest-path "${REPO_ROOT}/v2/Cargo.toml" -p restflow-native --features python-module --all-targets -- -D warnings
}

develop_package() {
  "$(venv_python)" "${REPO_ROOT}/v2/python/scripts/package.py" develop \
    --target-dir "${CARGO_TARGET_DIR}"
}

smoke_package() {
  "$(venv_python)" "${REPO_ROOT}/v2/python/scripts/package.py" smoke
}

build_package() {
  "$(venv_python)" "${REPO_ROOT}/v2/python/scripts/package.py" build \
    --target-dir "${CARGO_TARGET_DIR}" \
    --dist-dir "${WHEEL_DIR}"
}

run_ci() {
  ensure_python_env
  python_tests
  rust_native_checks
  develop_package
  smoke_package
  build_package
}

usage() {
  cat <<'EOF'
Usage: v2/scripts/check.sh [ci|bootstrap|python|rust|develop|smoke|build]

Commands:
  ci         Run the full local/CI package loop.
  bootstrap Create/update the Python build venv and install maturin.
  python    Run Python contract tests.
  rust      Run restflow-native Rust test and clippy.
  develop   Install the Python/PyO3 package into the venv.
  smoke     Import the installed package and call the native core.
  build     Build a wheel into WHEEL_DIR.

Environment:
  PYTHON_BIN       Python used to create the venv. Default: python3
  PYTHON_VENV      Virtualenv path. Default: /tmp/restflow-v2-python-venv
  CARGO_TARGET_DIR Cargo target path. Default: /tmp/restflow-v2-python-target
  WHEEL_DIR        Wheel output path. Default: /tmp/restflow-v2-python-dist
EOF
}

command="${1:-ci}"
case "${command}" in
  ci)
    run_ci
    ;;
  bootstrap)
    ensure_python_env
    ;;
  python)
    python_tests
    ;;
  rust)
    rust_native_checks
    ;;
  develop)
    develop_package
    ;;
  smoke)
    smoke_package
    ;;
  build)
    build_package
    ;;
  -h | --help | help)
    usage
    ;;
  *)
    usage >&2
    exit 2
    ;;
esac
