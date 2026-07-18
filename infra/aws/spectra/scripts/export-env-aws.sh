#!/usr/bin/env bash
# Export SPECTRA_* URLs for co-located smoke host (localhost services).
set -euo pipefail

export SPECTRA_CLICKHOUSE_URL="${SPECTRA_CLICKHOUSE_URL:-http://127.0.0.1:8123}"
export SPECTRA_TENSORBASE_URL="${SPECTRA_TENSORBASE_URL:-tcp://127.0.0.1:9528}"

echo "SPECTRA_CLICKHOUSE_URL=${SPECTRA_CLICKHOUSE_URL}"
echo "SPECTRA_TENSORBASE_URL=${SPECTRA_TENSORBASE_URL}"
