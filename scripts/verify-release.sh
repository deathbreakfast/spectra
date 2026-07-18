#!/usr/bin/env bash
# Release verification — gates + core regression.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "verify-release: gate-check"
./scripts/gate-check.sh

export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-1}"

echo "verify-release: core crate tests + smoke inventory"
export CARGO_TARGET_DIR="${SPECTRA_EXTRACT_TARGET:-target-spectra-extract}"
cargo test -p uf-spectra-core -p spectra-macros \
           -p spectra-backend-mem -p spectra-backend-sqlite \
           -p spectra-backend-tensorbase -p spectra-backend-clickhouse \
           -p spectra-runtime -p uf-spectra
cargo test -p uf-spectra --test smoke_inventory
cargo check -p uf-spectra --no-default-features
cargo check -p uf-spectra --features mem,sqlite
cargo run -p uf-spectra --example quickstart_transport --features mem

echo "verify-release: testkit + e2e"
export CARGO_TARGET_DIR="${SPECTRA_E2E_TARGET:-target-spectra-e2e}"
cargo test -p spectra-testkit
cargo test -p spectra-e2e

echo "verify-release: bench smoke"
export CARGO_TARGET_DIR="${SPECTRA_BENCH_TARGET:-target-spectra-bench}"
cargo run -p spectra-bench -- experiments
cargo run -p spectra-bench -- run --experiment bm-s1 --storage mem --topology embedded

echo "verify-release: verification crate hygiene"
if rg -i 'web-app-template|valence-spectra|spectra-wiring|prioritization|deathbreakfast|\bvalence\b|\bchronon\b|\bboson\b' \
  spectra-testkit spectra-e2e spectra-bench; then
  echo "verify-release: FAIL — forbidden host product reference in verification crates" >&2
  exit 1
fi

echo "verify-release: OK"
