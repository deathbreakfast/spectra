#!/usr/bin/env bash
# Run bench matrix as short sequential cargo invocations (keeps SSH responsive).
# Intended for co-located AWS hosts; uses modest concurrency by default.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPO="$(cd "$ROOT/../../.." && pwd)"
cd "$REPO"

export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-1}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/spectra-target-bench}"
export SPECTRA_BENCH_HARDWARE="${SPECTRA_BENCH_HARDWARE:-aws-t3-xlarge}"
export SPECTRA_BENCH_CONCURRENCY="${SPECTRA_BENCH_CONCURRENCY:-16}"
export SPECTRA_BENCH_DURATION_SECS="${SPECTRA_BENCH_DURATION_SECS:-30}"
export SPECTRA_BENCH_QUERY_ITERS="${SPECTRA_BENCH_QUERY_ITERS:-100}"
export SPECTRA_BENCH_PREFILL_SWEEP="${SPECTRA_BENCH_PREFILL_SWEEP:-1000,10000,100000}"

# shellcheck disable=SC1091
source "$ROOT/scripts/export-env-aws.sh"
"$ROOT/scripts/ensure-remote-services.sh"
"$ROOT/scripts/wait-clickhouse.sh"
"$ROOT/scripts/wait-tensorbase.sh"
"$ROOT/scripts/cleanup-remote-tables.sh"

REPORT_DIR="${SPECTRA_BENCH_REPORT_DIR:-${REPO}/profiling/spectra-bench/reports}"
mkdir -p "$REPORT_DIR"
FEATURES=(--features "mem,sqlite,clickhouse,tensorbase,telemetry-console")
# Build once
cargo build -p spectra-bench "${FEATURES[@]}"
BIN="${CARGO_TARGET_DIR}/debug/spectra-bench"

run_one() {
  local experiment="$1" storage="$2" topology="$3"
  local hw="${SPECTRA_BENCH_HARDWARE}"
  local report="${REPORT_DIR}/${experiment}-${storage}-${topology}-${hw}.json"
  echo "=== ${experiment} ${storage} ${topology} ==="
  # Leave one vCPU free for sshd on 4-vCPU hosts (taskset 0-2).
  local pin=(nice -n 10)
  if command -v taskset >/dev/null 2>&1; then
    pin=(taskset -c 0-2 nice -n 10)
  fi
  "${pin[@]}" "$BIN" run \
    --experiment "$experiment" \
    --storage "$storage" \
    --topology "$topology" \
    --prefill-sweep "$SPECTRA_BENCH_PREFILL_SWEEP" \
    --concurrency "$SPECTRA_BENCH_CONCURRENCY" \
    --duration-secs "$SPECTRA_BENCH_DURATION_SECS" \
    --query-iters "$SPECTRA_BENCH_QUERY_ITERS" \
    --report "$report"
  # brief cool-down so sshd can schedule
  sleep 2
}

WRITE=(bm-sw0 bm-sw1 bm-sw2 bm-sw3 bm-sw4)
QUERY=(bm-sq0 bm-sq1 bm-sq2 bm-sq3)

for storage in mem sqlite; do
  for exp in "${WRITE[@]}" "${QUERY[@]}"; do
    run_one "$exp" "$storage" embedded
  done
done

run_one bm-s0 mem embedded
run_one bm-s1 mem embedded
run_one bm-s2 sqlite embedded
run_one bm-s3 mem embedded

if [[ -n "${SPECTRA_CLICKHOUSE_URL:-}" ]]; then
  for exp in "${WRITE[@]}" "${QUERY[@]}"; do
    run_one "$exp" clickhouse remote-ingest
  done
fi
if [[ -n "${SPECTRA_TENSORBASE_URL:-}" ]]; then
  for exp in "${WRITE[@]}" "${QUERY[@]}"; do
    run_one "$exp" tensorbase remote-ingest
  done
fi

echo "Bench matrix complete. Reports in ${REPORT_DIR}"
