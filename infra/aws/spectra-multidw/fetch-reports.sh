#!/usr/bin/env bash
# Fetch multidw reports (+ util summary) from writer.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$ROOT/../../.." && pwd)"
ENV_FILE="${INSTANCES_ENV:-$ROOT/instances.env}"
# shellcheck disable=SC1091
source "$ENV_FILE"

HOST="${SPECTRA_WRITER_HOST:-${WRITER_PUBLIC_IP:-$WRITER_PRIVATE_IP}}"
SSH_KEY="${SSH_KEY_PATH:-$HOME/.ssh/${AWS_KEY_NAME:-}.pem}"
SSH_OPTS=(-o StrictHostKeyChecking=accept-new -i "$SSH_KEY")
RSYNC_SSH=(ssh "${SSH_OPTS[@]}")
REMOTE_DIR="/tmp/spectra-aws"
LOCAL_DIR="${REPO}/profiling/spectra-bench/reports"
LOCAL_UTIL="${ROOT}/host-util"

mkdir -p "$LOCAL_DIR" "$LOCAL_UTIL"
echo "Fetching reports from ${SSH_USER}@${HOST}..."
rsync -az -e "${RSYNC_SSH[*]}" \
  "${SSH_USER}@${HOST}:${REMOTE_DIR}/profiling/spectra-bench/reports/" \
  "${LOCAL_DIR}/" || true
rsync -az -e "${RSYNC_SSH[*]}" \
  "${SSH_USER}@${HOST}:\$HOME/spectra-multidw-util/" \
  "${LOCAL_UTIL}/" 2>/dev/null || true

echo "Reports under ${LOCAL_DIR}"
ls -la "$LOCAL_DIR"/multidw-* 2>/dev/null | head -40 || ls -la "$LOCAL_DIR" | head -40
