#!/usr/bin/env bash
# Bootstrap all hosts: DW services + writer toolchain.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENV_FILE="${INSTANCES_ENV:-$ROOT/instances.env}"
# shellcheck disable=SC1091
source "$ENV_FILE"

DW_N="${SPECTRA_BENCH_DW_N:-1}"
DW_KIND="${SPECTRA_MULTIDW_DW_KIND:?}"

echo "Bootstrapping DW hosts (${DW_KIND}, n=${DW_N})..."
for i in $(seq 0 $((DW_N - 1))); do
  "$ROOT/bootstrap-dw.sh" "$i"
done

echo "Bootstrapping writer..."
"$ROOT/bootstrap-writer.sh"

echo "Bootstrap complete."
