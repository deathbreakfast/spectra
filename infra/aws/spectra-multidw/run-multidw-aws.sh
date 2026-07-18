#!/usr/bin/env bash
# On-writer: run BM-SW5/SW6 (single-row durable) and optionally BM-SW7 (L2 batched)
# with a writer ladder + batch_max sweep.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$ROOT/../../.." && pwd)"
if [[ -d /tmp/spectra-aws/spectra-bench ]]; then
  REPO_ROOT=/tmp/spectra-aws
fi

ENV_FILE="${INSTANCES_ENV:-$ROOT/instances.env}"
if [[ -f "$ENV_FILE" ]]; then
  # shellcheck disable=SC1091
  source "$ENV_FILE"
fi

# shellcheck disable=SC1091
source "$ROOT/scripts/export-env-aws.sh"

DW_N="${SPECTRA_BENCH_DW_N:-1}"
DW_KIND="${SPECTRA_MULTIDW_DW_KIND:?}"
HW="${SPECTRA_BENCH_HARDWARE:-aws-t3-xlarge}"
DURATION="${SPECTRA_BENCH_DURATION_SECS:-30}"
CONCURRENCY="${SPECTRA_BENCH_CONCURRENCY:-64}"
REPORT_DIR="${SPECTRA_BENCH_REPORT_DIR:-$REPO_ROOT/profiling/spectra-bench/reports}"
UTIL_DIR="${SPECTRA_BENCH_UTIL_DIR:-$HOME/spectra-multidw-util}"
# Comma-separated writer process counts for BM-SW7 ladder (default 1,2).
WRITER_LADDER="${SPECTRA_BENCH_WRITER_LADDER:-1,2}"
# Comma-separated L2 batch_max values for BM-SW7 (default 32,512,2048).
BATCH_SWEEP="${SPECTRA_BENCH_BATCH_SWEEP:-512,2048}"
# Set SPECTRA_MULTIDW_RUN_SW7=0 to skip BM-SW7.
RUN_SW7="${SPECTRA_MULTIDW_RUN_SW7:-1}"
# Set SPECTRA_MULTIDW_RUN_BASE=0 to skip BM-SW5/SW6 (batched-only campaign).
RUN_BASE="${SPECTRA_MULTIDW_RUN_BASE:-1}"
mkdir -p "$REPORT_DIR" "$UTIL_DIR"

export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-4}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/spectra-target-multidw}"
export CARGO_INCREMENTAL=0
export SPECTRA_BENCH_HARDWARE="$HW"
export SPECTRA_BENCH_DURATION_SECS="$DURATION"
export SPECTRA_BENCH_CONCURRENCY="$CONCURRENCY"
export SPECTRA_BENCH_DW_N="$DW_N"
export SPECTRA_BENCH_CLIENT_COUNT="$DW_N"

FEATURE="$DW_KIND"
SAMPLE="$ROOT/scripts/sample-host-util.sh"
chmod +x "$SAMPLE" 2>/dev/null || true
"$SAMPLE" "writer" "$((DURATION + 20))" "$UTIL_DIR/writer.json" &
WRITER_UTIL_PID=$!

cd "$REPO_ROOT"

# Warm build once
cargo build -p spectra-bench --features "$FEATURE"

run_one() {
  local exp="$1"
  local shard="$2"
  local report="${REPORT_DIR}/multidw-${exp}-${DW_KIND}-n${DW_N}-shard${shard}-${HW}.json"
  (
    export SPECTRA_BENCH_CLIENT_INDEX="$shard"
    echo "=== ${exp} storage=${DW_KIND} n=${DW_N} shard=${shard} ==="
    cargo run -p spectra-bench --features "$FEATURE" -- \
      run --experiment "$exp" \
      --storage "$DW_KIND" \
      --topology remote-ingest \
      --duration-secs "$DURATION" \
      --concurrency "$CONCURRENCY" \
      --report "$report"
  )
}

run_sw7_one() {
  local writers="$1"
  local batch_max="$2"
  local writer_idx="$3"
  local report="${REPORT_DIR}/multidw-bm-sw7-${DW_KIND}-n${DW_N}-w${writers}-batch${batch_max}-writer${writer_idx}-${HW}.json"
  (
    export SPECTRA_BENCH_CLIENT_INDEX="$writer_idx"
    export SPECTRA_BENCH_CLIENT_COUNT="$writers"
    export SPECTRA_BENCH_WRITER_N="$writers"
    export SPECTRA_BENCH_BATCH_MAX="$batch_max"
    echo "=== bm-sw7 storage=${DW_KIND} n=${DW_N} writers=${writers} batch_max=${batch_max} writer=${writer_idx} ==="
    cargo run -p spectra-bench --features "$FEATURE" -- \
      run --experiment bm-sw7 \
      --storage "$DW_KIND" \
      --topology remote-ingest \
      --duration-secs "$DURATION" \
      --concurrency "$CONCURRENCY" \
      --batch-max "$batch_max" \
      --report "$report"
  )
}

for exp in bm-sw5 bm-sw6; do
  if [[ "$RUN_BASE" == "0" ]]; then
    echo "Skipping ${exp} (SPECTRA_MULTIDW_RUN_BASE=0)"
    continue
  fi
  pids=()
  for shard in $(seq 0 $((DW_N - 1))); do
    run_one "$exp" "$shard" &
    pids+=($!)
  done
  ec=0
  for pid in "${pids[@]}"; do
    wait "$pid" || ec=1
  done
  if [[ "$ec" -ne 0 ]]; then
    echo "experiment ${exp} had failures" >&2
    exit 1
  fi
done

if [[ "$RUN_SW7" != "0" ]]; then
  IFS=',' read -r -a WRITERS <<< "$WRITER_LADDER"
  IFS=',' read -r -a BATCHES <<< "$BATCH_SWEEP"
  for writers in "${WRITERS[@]}"; do
    writers="$(echo "$writers" | tr -d '[:space:]')"
    [[ -n "$writers" ]] || continue
    for batch_max in "${BATCHES[@]}"; do
      batch_max="$(echo "$batch_max" | tr -d '[:space:]')"
      [[ -n "$batch_max" ]] || continue
      pids=()
      for w in $(seq 0 $((writers - 1))); do
        run_sw7_one "$writers" "$batch_max" "$w" &
        pids+=($!)
      done
      ec=0
      for pid in "${pids[@]}"; do
        wait "$pid" || ec=1
      done
      if [[ "$ec" -ne 0 ]]; then
        echo "bm-sw7 writers=${writers} batch_max=${batch_max} had failures" >&2
        exit 1
      fi
    done
  done
fi

wait "$WRITER_UTIL_PID" || true

# Merge local util files (DW samples may arrive later via deploy-and-run)
python3 - <<PY
import json, glob, os
util_dir = "${UTIL_DIR}"
paths = [p for p in sorted(glob.glob(util_dir + "/*.json")) if not p.endswith("host-util-summary.json")]
out = []
for p in paths:
    try:
        with open(p) as f:
            out.append(json.load(f))
    except Exception:
        pass
summary = util_dir + "/host-util-summary.json"
with open(summary, "w") as f:
    json.dump(out, f, indent=2)
print("wrote", summary, "entries", len(out))
PY

echo "Campaign complete. Reports under ${REPORT_DIR}"
ls -la "$REPORT_DIR"/multidw-* 2>/dev/null || true
