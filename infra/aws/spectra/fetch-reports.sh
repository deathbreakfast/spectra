#!/usr/bin/env bash
# Fetch bench JSON reports from EC2 into the local checkout.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$ROOT/../../.." && pwd)"
ENV_FILE="${INSTANCES_ENV:-$ROOT/instances.env}"
# shellcheck disable=SC1091
source "$ENV_FILE"

HOST="${SPECTRA_E2E_HOST:-${SMOKE_PUBLIC_IP:-$SMOKE_IP}}"
SSH_KEY="${SSH_KEY_PATH:-$HOME/.ssh/${AWS_KEY_NAME:-}.pem}"
SSH_OPTS=(-o StrictHostKeyChecking=accept-new -i "$SSH_KEY")
RSYNC_SSH=(ssh "${SSH_OPTS[@]}")
REMOTE_DIR="/tmp/spectra-aws"
LOCAL_DIR="${REPO}/profiling/spectra-bench/reports"

mkdir -p "$LOCAL_DIR"
echo "Fetching reports from ${SSH_USER}@${HOST}:${REMOTE_DIR}/profiling/spectra-bench/reports/ ..."
rsync -az -e "${RSYNC_SSH[*]}" \
  "${SSH_USER}@${HOST}:${REMOTE_DIR}/profiling/spectra-bench/reports/" \
  "${LOCAL_DIR}/"

echo "Reports saved under ${LOCAL_DIR}"
ls -la "$LOCAL_DIR" | head -50
