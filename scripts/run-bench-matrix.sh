#!/usr/bin/env bash
# Run the full Spectra capacity + smoke experiment matrix across storages.
# Generic: works on any host with cargo + optional remote URL env vars.
#
# Env:
#   SPECTRA_CLICKHOUSE_URL / SPECTRA_TENSORBASE_URL — required for remote storages
#   SPECTRA_BENCH_HARDWARE — label stamped into reports (e.g. aws-t3-xlarge)
#   SPECTRA_BENCH_REPORT_DIR — default profiling/spectra-bench/reports
#   SPECTRA_BENCH_PREFILL_SWEEP — default full depth sweep
#   CARGO_BUILD_JOBS / CARGO_TARGET_DIR — build guardrails
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-1}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-target-spectra-bench}"
export SPECTRA_BENCH_HARDWARE="${SPECTRA_BENCH_HARDWARE:-local}"

REPORT_DIR="${SPECTRA_BENCH_REPORT_DIR:-profiling/spectra-bench/reports}"
mkdir -p "$REPORT_DIR"

PREFILL_SWEEP="${SPECTRA_BENCH_PREFILL_SWEEP:-1000,10000,100000,1000000}"
FEATURES=(--features "mem,sqlite,clickhouse,tensorbase,telemetry-console")

WRITE_EXPERIMENTS=(bm-sw0 bm-sw1 bm-sw2 bm-sw3 bm-sw4)
QUERY_EXPERIMENTS=(bm-sq0 bm-sq1 bm-sq2 bm-sq3)
SMOKE_EXPERIMENTS=(bm-s0 bm-s1 bm-s2 bm-s3)

run_one() {
  local experiment="$1"
  local storage="$2"
  local topology="$3"
  local hw="${SPECTRA_BENCH_HARDWARE}"
  local report="${REPORT_DIR}/${experiment}-${storage}-${topology}-${hw}.json"

  echo "=== ${experiment} storage=${storage} topology=${topology} ==="
  local extra=()
  if [[ -n "${SPECTRA_BENCH_CONCURRENCY:-}" ]]; then
    extra+=(--concurrency "$SPECTRA_BENCH_CONCURRENCY")
  fi
  if [[ -n "${SPECTRA_BENCH_DURATION_SECS:-}" ]]; then
    extra+=(--duration-secs "$SPECTRA_BENCH_DURATION_SECS")
  fi
  cargo run -p spectra-bench "${FEATURES[@]}" -- run \
    --experiment "$experiment" \
    --storage "$storage" \
    --topology "$topology" \
    --prefill-sweep "$PREFILL_SWEEP" \
    "${extra[@]}" \
    --report "$report"
}

# Embedded storages
for storage in mem sqlite; do
  for exp in "${WRITE_EXPERIMENTS[@]}" "${QUERY_EXPERIMENTS[@]}"; do
    run_one "$exp" "$storage" embedded
  done
done

# Smoke (topology/storage constraints enforced by CLI)
run_one bm-s0 mem embedded
run_one bm-s1 mem embedded
run_one bm-s2 sqlite embedded
run_one bm-s3 mem embedded

# Remote storages when URLs are present
if [[ -n "${SPECTRA_CLICKHOUSE_URL:-}" ]]; then
  for exp in "${WRITE_EXPERIMENTS[@]}" "${QUERY_EXPERIMENTS[@]}"; do
    run_one "$exp" clickhouse remote-ingest
  done
else
  echo "SKIP clickhouse: SPECTRA_CLICKHOUSE_URL unset"
fi

if [[ -n "${SPECTRA_TENSORBASE_URL:-}" ]]; then
  for exp in "${WRITE_EXPERIMENTS[@]}" "${QUERY_EXPERIMENTS[@]}"; do
    run_one "$exp" tensorbase remote-ingest
  done
else
  echo "SKIP tensorbase: SPECTRA_TENSORBASE_URL unset"
fi

echo "Bench matrix complete. Reports in ${REPORT_DIR}"
# Silence unused smoke list when all invoked explicitly above
: "${SMOKE_EXPERIMENTS[@]}"
