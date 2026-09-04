#!/usr/bin/env bash
# Line-coverage baseline for the Spectra workspace (requires cargo-llvm-cov).
#
# Default scope matches CI: exclude e2e/bench (timing-sensitive under instrumentation).
# Pass --full to include every workspace crate, or forward any cargo-llvm-cov flags.
set -euo pipefail

cd "$(dirname "$0")/.."

if ! command -v cargo-llvm-cov >/dev/null 2>&1; then
  echo "install cargo-llvm-cov: cargo install cargo-llvm-cov --locked" >&2
  exit 1
fi

export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-1}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-target-spectra-extract}"

FULL=0
ARGS=()
for arg in "$@"; do
  if [[ "$arg" == "--full" ]]; then
    FULL=1
  else
    ARGS+=("$arg")
  fi
done

if [[ $FULL -eq 1 ]]; then
  cargo llvm-cov --workspace --all-features "${ARGS[@]}"
else
  cargo llvm-cov --workspace \
    --exclude spectra-e2e \
    --exclude spectra-bench \
    --all-features \
    "${ARGS[@]}"
fi
