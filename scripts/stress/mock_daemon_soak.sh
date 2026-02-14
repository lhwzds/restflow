#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

export RESTFLOW_TEST_MODE="${RESTFLOW_TEST_MODE:-1}"
SOAK_MINUTES="${SOAK_MINUTES:-60}"
LOG_DIR="${LOG_DIR:-$ROOT_DIR/target/stress-artifacts}"
METRIC_INTERVAL_SECONDS="${METRIC_INTERVAL_SECONDS:-30}"
FD_WARN_THRESHOLD="${FD_WARN_THRESHOLD:-2048}"
FD_HARD_THRESHOLD="${FD_HARD_THRESHOLD:-4096}"
THREAD_WARN_THRESHOLD="${THREAD_WARN_THRESHOLD:-256}"
THREAD_HARD_THRESHOLD="${THREAD_HARD_THRESHOLD:-512}"
RSS_WARN_KB="${RSS_WARN_KB:-1048576}"
RSS_HARD_KB="${RSS_HARD_KB:-1572864}"
mkdir -p "$LOG_DIR"

RUNS=$((SOAK_MINUTES * 2))
LOG_FILE="$LOG_DIR/mock-daemon-soak.log"
SUMMARY_FILE="$LOG_DIR/stress-summary.json"
METRICS_FILE="$LOG_DIR/soak-metrics.jsonl"
SUMMARY_MD_FILE="$LOG_DIR/soak-summary.md"

success=0
failure=0
warn_count=0
panic_count=0
hard_limit_breaches=0
last_metric_at=0

collect_metrics() {
  local now
  now="$(date +%s)"
  if (( now - last_metric_at < METRIC_INTERVAL_SECONDS )); then
    return
  fi
  last_metric_at="$now"

  local fd_count
  if command -v lsof >/dev/null 2>&1; then
    fd_count="$(lsof -p $$ 2>/dev/null | wc -l | tr -d ' ')"
  else
    fd_count=0
  fi

  local thread_count
  thread_count="$(ps -o nlwp= -p $$ 2>/dev/null | tr -d ' ' || echo 0)"
  if [[ -z "$thread_count" ]]; then
    thread_count=0
  fi

  local rss_kb
  rss_kb="$(ps -o rss= -p $$ 2>/dev/null | tr -d ' ' || echo 0)"
  if [[ -z "$rss_kb" ]]; then
    rss_kb=0
  fi

  local level="ok"
  if (( fd_count >= FD_HARD_THRESHOLD || thread_count >= THREAD_HARD_THRESHOLD || rss_kb >= RSS_HARD_KB )); then
    level="hard"
    hard_limit_breaches=$((hard_limit_breaches + 1))
  elif (( fd_count >= FD_WARN_THRESHOLD || thread_count >= THREAD_WARN_THRESHOLD || rss_kb >= RSS_WARN_KB )); then
    level="warn"
    warn_count=$((warn_count + 1))
  fi

  python3 - <<PY
import json
entry = {
  "timestamp": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")",
  "pid": $$,
  "fd_count": int("${fd_count}"),
  "thread_count": int("${thread_count}"),
  "rss_kb": int("${rss_kb}"),
  "running_task_count": 0,
  "pending_task_count": 0,
  "queue_depth": 0,
  "level": "${level}"
}
with open("${METRICS_FILE}", "a", encoding="utf-8") as f:
    f.write(json.dumps(entry, ensure_ascii=False) + "\\n")
PY

  if [[ "$level" == "hard" ]]; then
    echo "Hard threshold exceeded: fd=${fd_count}, threads=${thread_count}, rss_kb=${rss_kb}" | tee -a "$LOG_FILE"
    return 1
  fi
  return 0
}

for ((i = 1; i <= RUNS; i++)); do
  echo "[$(date -u +"%Y-%m-%dT%H:%M:%SZ")] soak iteration $i/$RUNS" | tee -a "$LOG_FILE"
  if ! collect_metrics; then
    panic_count=$((panic_count + 1))
    break
  fi
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
  "timeout": max(0, ${RUNS} - (${success} + ${failure})),
  "success_rate": (${success} / ${RUNS}) if ${RUNS} else 0,
  "panic_count": ${panic_count},
  "warn_count": ${warn_count},
  "hard_limit_breaches": ${hard_limit_breaches},
  "thresholds": {
    "fd_warn": int("${FD_WARN_THRESHOLD}"),
    "fd_hard": int("${FD_HARD_THRESHOLD}"),
    "thread_warn": int("${THREAD_WARN_THRESHOLD}"),
    "thread_hard": int("${THREAD_HARD_THRESHOLD}"),
    "rss_warn_kb": int("${RSS_WARN_KB}"),
    "rss_hard_kb": int("${RSS_HARD_KB}")
  },
  "metrics_jsonl": "${METRICS_FILE}",
}
with open("${SUMMARY_FILE}", "w", encoding="utf-8") as f:
    json.dump(summary, f, indent=2)
PY

cat >"$SUMMARY_MD_FILE" <<EOF
# Mock Daemon Soak Summary

- Total runs: ${RUNS}
- Success: ${success}
- Failure: ${failure}
- Warn count: ${warn_count}
- Hard limit breaches: ${hard_limit_breaches}
- Panic count: ${panic_count}
- Metrics JSONL: ${METRICS_FILE}
- Summary JSON: ${SUMMARY_FILE}
EOF

echo "Soak complete: success=${success}, failure=${failure}, summary=${SUMMARY_FILE}" | tee -a "$LOG_FILE"

if [[ "$failure" -gt 0 || "$hard_limit_breaches" -gt 0 || "$panic_count" -gt 0 ]]; then
  exit 1
fi
