#!/usr/bin/env bash
# Full Spectra remote E2E gate on the current host (local or EC2).
# Runs embedded sanity, ignored remote catalog, live contracts, emit examples.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$ROOT/../../.." && pwd)"

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/spectra-target-e2e}"
export CARGO_INCREMENTAL=0
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-4}"

# shellcheck disable=SC1091
source "$ROOT/scripts/export-env-aws.sh"
"$ROOT/scripts/ensure-remote-services.sh"
"$ROOT/scripts/wait-clickhouse.sh"
"$ROOT/scripts/wait-tensorbase.sh"
"$ROOT/scripts/cleanup-remote-tables.sh"

cd "$REPO"

echo "=== spectra-e2e embedded CI slice (sanity) ==="
cargo test -p spectra-e2e --test scenarios -- --test-threads=1

echo "=== spectra-e2e full remote ignored catalog ==="
cargo test -p spectra-e2e --features clickhouse,tensorbase --test scenarios -- --ignored --test-threads=1

echo "=== clickhouse live storage contract ==="
cargo test -p spectra-backend-clickhouse --test storage_contract -- --include-ignored

echo "=== tensorbase live storage contract ==="
cargo test -p spectra-backend-tensorbase --test storage_contract -- --include-ignored

echo "=== quickstart_clickhouse_emit example ==="
cargo run -p uf-spectra --example quickstart_clickhouse_emit --features clickhouse

echo "=== quickstart_tensorbase_emit example ==="
cargo run -p uf-spectra --example quickstart_tensorbase_emit --features tensorbase

echo "Spectra full remote E2E validation complete."
