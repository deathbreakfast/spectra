#!/usr/bin/env bash
# Upstream gate — blocks release tags when forbidden vocabulary appears.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

fail() {
  echo "gate-check: FAIL — $1" >&2
  exit 1
}

check_forbidden() {
  local pattern="$1"
  local label="$2"
  shift 2
  if rg -n -i "$pattern" "$@" . \
    --glob '!scripts/gate-check.sh' \
    --glob '!scripts/verify-release.sh' ; then
    fail "$label"
  fi
}

# Family / host adapter crate names in docs (upstream purity)
check_forbidden 'prioritization\.md|spectra-wiring|valence-spectra|chronon-spectra|boson-spectra|\bvalence\b|\bchronon\b|\bboson\b' \
  'forbidden host adapter or product reference'

# Monolith vocabulary in docs
check_forbidden '\bmonolith(ic)?\b' \
  'forbidden monolith vocabulary in docs' \
  --glob '*.md'

# Mode enums
if rg -n 'SpectraMode|SPECTRA_MODE|enum\s+\w*Mode' --glob '*.rs' . \
  --glob '!scripts/gate-check.sh' \
  --glob '!scripts/verify-release.sh' ; then
  fail 'forbidden Mode enum in Rust sources'
fi

# Surreal implementation (allow gate docs that prohibit Surreal by name)
check_forbidden 'surrealdb|spectra-backend-surreal|surreal_store|SurrealLocal|SurrealStore' \
  'forbidden Surreal implementation reference'

echo "gate-check: OK"
