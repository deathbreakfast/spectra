#!/usr/bin/env bash
# Full Spectra bench matrix across mem/sqlite/tensorbase/clickhouse on this host.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$ROOT/../../.." && pwd)"

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/spectra-target-bench}"
export CARGO_INCREMENTAL=0
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-4}"
export SPECTRA_BENCH_HARDWARE="${SPECTRA_BENCH_HARDWARE:-aws-t3-xlarge}"
export SPECTRA_BENCH_REPORT_DIR="${SPECTRA_BENCH_REPORT_DIR:-${REPO}/profiling/spectra-bench/reports}"
# Full 1M cell remains opt-in (`SPECTRA_BENCH_PREFILL_SWEEP=...,1000000`); default omits it for wall-clock.
export SPECTRA_BENCH_PREFILL_SWEEP="${SPECTRA_BENCH_PREFILL_SWEEP:-1000,10000,100000}"
# Modest concurrency + nice in sequential runner keeps SSH usable on burstable hosts.
export SPECTRA_BENCH_CONCURRENCY="${SPECTRA_BENCH_CONCURRENCY:-16}"
# 1000 iters × 1M-depth scans is multi-hour on a single node; campaign default is 100.
export SPECTRA_BENCH_QUERY_ITERS="${SPECTRA_BENCH_QUERY_ITERS:-100}"

# shellcheck disable=SC1091
source "$ROOT/scripts/export-env-aws.sh"
"$ROOT/scripts/ensure-remote-services.sh"
"$ROOT/scripts/wait-clickhouse.sh"
"$ROOT/scripts/wait-tensorbase.sh"
"$ROOT/scripts/cleanup-remote-tables.sh"

cd "$REPO"
mkdir -p "$SPECTRA_BENCH_REPORT_DIR"

echo "=== full bench matrix (hardware=${SPECTRA_BENCH_HARDWARE} C=${SPECTRA_BENCH_CONCURRENCY}) ==="
# Sequential + nice keeps sshd responsive under firehose (see scripts/run-bench-campaign-sequential.sh).
chmod +x "$ROOT/scripts/run-bench-campaign-sequential.sh"
"$ROOT/scripts/run-bench-campaign-sequential.sh"

echo "Spectra AWS bench campaign complete. Reports in ${SPECTRA_BENCH_REPORT_DIR}"
