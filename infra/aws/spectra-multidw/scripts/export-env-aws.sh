#!/usr/bin/env bash
# Export multi-DW SPECTRA_* URLs pointing at private DW IPs (run on writer or laptop).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENV_FILE="${INSTANCES_ENV:-$ROOT/instances.env}"
if [[ -f "$ENV_FILE" ]]; then
  # shellcheck disable=SC1091
  source "$ENV_FILE"
fi

DW_N="${SPECTRA_BENCH_DW_N:-1}"
DW_KIND="${SPECTRA_MULTIDW_DW_KIND:-clickhouse}"

export SPECTRA_BENCH_DW_N="$DW_N"

for i in $(seq 0 $((DW_N - 1))); do
  var="DW_${i}_PRIVATE_IP"
  ip="${!var:-}"
  if [[ -z "$ip" ]]; then
    echo "Missing ${var} in instances.env" >&2
    exit 1
  fi
  if [[ "$DW_KIND" == "clickhouse" ]]; then
    export SPECTRA_CLICKHOUSE_URL_${i}="http://${ip}:8123"
    echo "SPECTRA_CLICKHOUSE_URL_${i}=http://${ip}:8123"
  else
    export SPECTRA_TENSORBASE_URL_${i}="tcp://${ip}:9528"
    echo "SPECTRA_TENSORBASE_URL_${i}=tcp://${ip}:9528"
  fi
done

# Convenience aliases for n=1 tooling that still reads bare URL
if [[ "$DW_N" == "1" ]]; then
  if [[ "$DW_KIND" == "clickhouse" ]]; then
    export SPECTRA_CLICKHOUSE_URL="${SPECTRA_CLICKHOUSE_URL_0}"
    echo "SPECTRA_CLICKHOUSE_URL=${SPECTRA_CLICKHOUSE_URL}"
  else
    export SPECTRA_TENSORBASE_URL="${SPECTRA_TENSORBASE_URL_0}"
    echo "SPECTRA_TENSORBASE_URL=${SPECTRA_TENSORBASE_URL}"
  fi
fi
