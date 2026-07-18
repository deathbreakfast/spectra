#!/usr/bin/env bash
# Download a URL to a path and verify SHA-256 against SHA256SUMS.
# Usage: download_verified <url> <dest-path> [sums-basename]
set -euo pipefail

_HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "${_HERE}/verify-sha256.sh"

download_verified() {
  local url="$1"
  local dest="$2"
  local name="${3:-$(basename "$dest")}"
  curl -fsSL -o "$dest" "$url"
  verify_sha256 "$dest" "$name"
}

if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
  download_verified "$@"
fi
