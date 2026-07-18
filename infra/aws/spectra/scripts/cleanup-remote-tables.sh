#!/usr/bin/env bash
# Truncate Spectra remote tables before smoke tests (idempotent).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck disable=SC1091
source "$ROOT/scripts/export-env-aws.sh"

CH_URL="${SPECTRA_CLICKHOUSE_URL%/}"
TB_URL="${SPECTRA_TENSORBASE_URL:-tcp://127.0.0.1:9528}"
TB_HOST_PORT="${TB_URL#*://}"

echo "Truncating ClickHouse tables at ${CH_URL}..."
for table in spectra_metrics spectra_events; do
  curl -sf "${CH_URL}/" --data-binary "TRUNCATE TABLE IF EXISTS ${table}" >/dev/null || true
done

if command -v clickhouse-client >/dev/null 2>&1; then
  CH_CLIENT=(clickhouse-client --host "${TB_HOST_PORT%%:*}" --port "${TB_HOST_PORT##*:}")
elif [[ -x "${HOME}/tensorbase-smoke/clickhouse-client" ]]; then
  CH_CLIENT=("${HOME}/tensorbase-smoke/clickhouse-client" --host "${TB_HOST_PORT%%:*}" --port "${TB_HOST_PORT##*:}")
else
  echo "No clickhouse-client for TensorBase truncate; skipping."
  exit 0
fi

echo "Truncating TensorBase tables at ${TB_HOST_PORT}..."
for table in spectra_metrics spectra_events; do
  "${CH_CLIENT[@]}" -q "TRUNCATE TABLE IF EXISTS ${table}" >/dev/null 2>&1 || true
done

echo "Remote table cleanup complete."
