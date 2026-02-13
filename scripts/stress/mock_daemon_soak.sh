#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

export RESTFLOW_TEST_MODE="${RESTFLOW_TEST_MODE:-1}"
SOAK_MINUTES="${SOAK_MINUTES:-60}"
LOG_DIR="${LOG_DIR:-$ROOT_DIR/target/stress-artifacts}"
mkdir -p "$LOG_DIR"

RUNS=$((SOAK_MINUTES * 2))
LOG_FILE="$LOG_DIR/mock-daemon-soak.log"
SUMMARY_FILE="$LOG_DIR/stress-summary.json"

success=0
failure=0

for ((i = 1; i <= RUNS; i++)); do
  echo "[$(date -u +"%Y-%m-%dT%H:%M:%SZ")] soak iteration $i/$RUNS" | tee -a "$LOG_FILE"
  if cargo test -p restflow-core --test stress_mock_runtime -- --nocapture >>"$LOG_FILE" 2>&1; then
    success=$((success + 1))
  else
    failure=$((failure + 1))
  fi

done

python3 - <<PY
import json
summary = {
  "total_runs": ${RUNS},
  "success": ${success},
  "failure": ${failure},
  "timeout": 0,
  "success_rate": (${success} / ${RUNS}) if ${RUNS} else 0,
  "panic_count": 0,
}
with open("${SUMMARY_FILE}", "w", encoding="utf-8") as f:
    json.dump(summary, f, indent=2)
PY

echo "Soak complete: success=${success}, failure=${failure}, summary=${SUMMARY_FILE}" | tee -a "$LOG_FILE"

if [[ "$failure" -gt 0 ]]; then
  exit 1
fi
