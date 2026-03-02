#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

COMMANDS_DIR="${REPO_ROOT}/crates/restflow-tauri/src/commands"
IPC_BINDINGS_FILE="${REPO_ROOT}/crates/restflow-tauri/src/ipc_bindings.rs"
BASELINE_FILE="${REPO_ROOT}/crates/restflow-tauri/src/ipc_command_coverage.baseline.txt"

if [[ ! -d "${COMMANDS_DIR}" ]]; then
  echo "Commands directory not found: ${COMMANDS_DIR}" >&2
  exit 1
fi
if [[ ! -f "${IPC_BINDINGS_FILE}" ]]; then
  echo "IPC bindings file not found: ${IPC_BINDINGS_FILE}" >&2
  exit 1
fi
if [[ ! -f "${BASELINE_FILE}" ]]; then
  echo "Baseline file not found: ${BASELINE_FILE}" >&2
  exit 1
fi

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT

TAURI_COMMANDS_FILE="${TMP_DIR}/tauri_commands.txt"
BOUND_COMMANDS_FILE="${TMP_DIR}/bound_commands.txt"
UNBOUND_COMMANDS_FILE="${TMP_DIR}/unbound_commands.txt"
DANGLING_BOUND_COMMANDS_FILE="${TMP_DIR}/dangling_bound_commands.txt"
SNAPSHOT_FILE="${TMP_DIR}/coverage_snapshot.txt"

awk '
  BEGIN { expect_fn_signature = 0 }
  /#\[tauri::command/ {
    expect_fn_signature = 1
    next
  }
  expect_fn_signature == 1 && /fn[[:space:]]+[A-Za-z0-9_]+[[:space:]]*\(/ {
    line = $0
    sub(/^.*fn[[:space:]]+/, "", line)
    sub(/\(.*/, "", line)
    gsub(/[[:space:]]/, "", line)
    if (length(line) > 0) {
      print line
      expect_fn_signature = 0
    }
  }
' "${COMMANDS_DIR}"/*.rs | sort -u > "${TAURI_COMMANDS_FILE}"

awk '
  /tauri_specta::collect_commands!\[/ { inside_collect_macro = 1; next }
  inside_collect_macro == 1 && /\]/ { inside_collect_macro = 0; next }
  inside_collect_macro == 1 {
    line = $0
    gsub(/[[:space:]]/, "", line)
    if (line ~ /^commands::[A-Za-z0-9_]+,?$/) {
      sub(/^commands::/, "", line)
      sub(/,$/, "", line)
      print line
    }
  }
' "${IPC_BINDINGS_FILE}" | sort -u > "${BOUND_COMMANDS_FILE}"

comm -23 "${TAURI_COMMANDS_FILE}" "${BOUND_COMMANDS_FILE}" > "${UNBOUND_COMMANDS_FILE}"
comm -13 "${TAURI_COMMANDS_FILE}" "${BOUND_COMMANDS_FILE}" > "${DANGLING_BOUND_COMMANDS_FILE}"

{
  echo "# tauri-ipc-command-coverage baseline"
  echo "# format-version: 1"
  echo "tauri_total=$(wc -l < "${TAURI_COMMANDS_FILE}" | tr -d ' ')"
  echo "bound_total=$(wc -l < "${BOUND_COMMANDS_FILE}" | tr -d ' ')"
  echo "dangling_total=$(wc -l < "${DANGLING_BOUND_COMMANDS_FILE}" | tr -d ' ')"
  echo "unbound_total=$(wc -l < "${UNBOUND_COMMANDS_FILE}" | tr -d ' ')"
  echo
  echo "[unbound_commands]"
  if [[ -s "${UNBOUND_COMMANDS_FILE}" ]]; then
    cat "${UNBOUND_COMMANDS_FILE}"
  else
    echo "(none)"
  fi
  echo
  echo "[dangling_bound_commands]"
  if [[ -s "${DANGLING_BOUND_COMMANDS_FILE}" ]]; then
    cat "${DANGLING_BOUND_COMMANDS_FILE}"
  else
    echo "(none)"
  fi
} > "${SNAPSHOT_FILE}"

if diff -u "${BASELINE_FILE}" "${SNAPSHOT_FILE}"; then
  echo "Tauri IPC command coverage check passed."
  exit 0
fi

echo
echo "Tauri IPC command coverage check failed."
echo "Review the diff above, then update ${BASELINE_FILE} intentionally if expected." >&2
exit 1
