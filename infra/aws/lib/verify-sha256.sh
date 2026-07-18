#!/usr/bin/env bash
# Verify a file's SHA-256 against infra/aws/checksums/SHA256SUMS.
# Usage: verify_sha256 <path-to-file> [basename-in-sums]
set -euo pipefail

verify_sha256() {
  local file="$1"
  local name="${2:-$(basename "$file")}"
  local sums="${SPECTRA_SHA256SUMS:-}"
  if [[ -z "$sums" ]]; then
    local here
    here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    sums="${here}/../checksums/SHA256SUMS"
  fi
  if [[ ! -f "$sums" ]]; then
    echo "verify_sha256: missing checksums file: $sums" >&2
    return 1
  fi
  if [[ ! -f "$file" ]]; then
    echo "verify_sha256: missing file: $file" >&2
    return 1
  fi
  local expected actual
  expected="$(awk -v n="$name" '$2 == n { print $1; exit }' "$sums")"
  if [[ -z "$expected" ]]; then
    echo "verify_sha256: no checksum entry for $name in $sums" >&2
    return 1
  fi
  actual="$(sha256sum "$file" | awk '{ print $1 }')"
  if [[ "$actual" != "$expected" ]]; then
    echo "verify_sha256: SHA-256 mismatch for $name" >&2
    echo "  expected: $expected" >&2
    echo "  actual:   $actual" >&2
    return 1
  fi
  echo "verify_sha256: ok $name"
}

# When sourced, only define the function. When executed, run with args.
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
  verify_sha256 "$@"
fi
