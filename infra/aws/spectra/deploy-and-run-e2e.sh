#!/usr/bin/env bash
# Laptop entry: rsync repo to EC2 and run full remote E2E.
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

echo "Syncing repo to ${SSH_USER}@${HOST}:${REMOTE_DIR}..."
ssh "${SSH_OPTS[@]}" "${SSH_USER}@${HOST}" "mkdir -p ${REMOTE_DIR} ~/.cargo/git/db ~/.cargo/git/checkouts"
rsync -az --delete -e "${RSYNC_SSH[*]}" \
  --exclude target --exclude target-spectra-* --exclude .git \
  --exclude profiling/spectra-bench/reports \
  "$REPO/" "${SSH_USER}@${HOST}:${REMOTE_DIR}/"


ssh "${SSH_OPTS[@]}" "${SSH_USER}@${HOST}" bash -s <<REMOTE
set -euo pipefail
source "\$HOME/.cargo/env" 2>/dev/null || true
export CARGO_TARGET_DIR=\$HOME/spectra-target-e2e
export CARGO_INCREMENTAL=0
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-4}
cd ${REMOTE_DIR}
./infra/aws/spectra/run-e2e-aws.sh
REMOTE

echo "Remote Spectra full E2E passed."
