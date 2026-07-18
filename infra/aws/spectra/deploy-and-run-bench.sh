#!/usr/bin/env bash
# Laptop entry: rsync repo to EC2 and run full bench matrix.
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
HW="${SPECTRA_BENCH_HARDWARE:-aws-t3-xlarge}"

echo "Syncing repo to ${SSH_USER}@${HOST}:${REMOTE_DIR}..."
ssh "${SSH_OPTS[@]}" "${SSH_USER}@${HOST}" "mkdir -p ${REMOTE_DIR} ~/.cargo/git/db ~/.cargo/git/checkouts"
rsync -az --delete -e "${RSYNC_SSH[*]}" \
  --exclude target --exclude target-spectra-* --exclude .git \
  "$REPO/" "${SSH_USER}@${HOST}:${REMOTE_DIR}/"


ssh "${SSH_OPTS[@]}" "${SSH_USER}@${HOST}" bash -s <<REMOTE
set -euo pipefail
source "\$HOME/.cargo/env" 2>/dev/null || true
export CARGO_TARGET_DIR=\$HOME/spectra-target-bench
export CARGO_INCREMENTAL=0
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-4}
export SPECTRA_BENCH_HARDWARE=${HW}
export SPECTRA_BENCH_CONCURRENCY=\${SPECTRA_BENCH_CONCURRENCY:-64}
export SPECTRA_BENCH_REPORT_DIR=${REMOTE_DIR}/profiling/spectra-bench/reports
cd ${REMOTE_DIR}
# Run detached so laptop SSH drops do not kill the campaign.
nohup ./infra/aws/spectra/run-bench-aws.sh > \$HOME/spectra-bench-campaign.log 2>&1 &
echo "bench PID=\$! log=\$HOME/spectra-bench-campaign.log"
# Wait until complete (or fail)
while kill -0 \$! 2>/dev/null; do sleep 30; done
wait \$!
tail -30 \$HOME/spectra-bench-campaign.log
REMOTE

echo "Remote Spectra bench campaign finished. Fetch reports with ./fetch-reports.sh"
