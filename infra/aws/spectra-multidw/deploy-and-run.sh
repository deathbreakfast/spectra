#!/usr/bin/env bash
# Laptop: rsync to writer, sample util on all hosts, run BM-SW5/SW6, wait.
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
HW="${SPECTRA_BENCH_HARDWARE:-aws-t3-xlarge}"
DW_N="${SPECTRA_BENCH_DW_N:-1}"
DW_KIND="${SPECTRA_MULTIDW_DW_KIND:?}"
DURATION="${SPECTRA_BENCH_DURATION_SECS:-30}"
SAMPLE_SECS=$((DURATION + 45))

echo "Syncing repo to writer ${SSH_USER}@${HOST}:${REMOTE_DIR}..."
ssh "${SSH_OPTS[@]}" "${SSH_USER}@${HOST}" "mkdir -p ${REMOTE_DIR} ~/.cargo/git/db ~/.cargo/git/checkouts ~/spectra-multidw-util"
rsync -az --delete -e "${RSYNC_SSH[*]}" \
  --exclude target --exclude target-spectra-* --exclude .git \
  "$REPO/" "${SSH_USER}@${HOST}:${REMOTE_DIR}/"
rsync -az -e "${RSYNC_SSH[*]}" "$ENV_FILE" "${SSH_USER}@${HOST}:${REMOTE_DIR}/infra/aws/spectra-multidw/instances.env"

# Start util sampling on DW hosts (laptop SSH)
for i in $(seq 0 $((DW_N - 1))); do
  var="DW_${i}_PUBLIC_IP"
  dhost="${!var:-}"
  [[ -n "$dhost" ]] || continue
  echo "Starting util sample on DW ${i} (${dhost})..."
  scp "${SSH_OPTS[@]}" "$ROOT/scripts/sample-host-util.sh" \
    "${SSH_USER}@${dhost}:/tmp/sample-host-util.sh"
  ssh "${SSH_OPTS[@]}" "${SSH_USER}@${dhost}" \
    "chmod +x /tmp/sample-host-util.sh; nohup /tmp/sample-host-util.sh dw-${DW_KIND}-${i} ${SAMPLE_SECS} \$HOME/dw-util-${i}.json >\$HOME/dw-util-${i}.log 2>&1 &"
done

ssh "${SSH_OPTS[@]}" "${SSH_USER}@${HOST}" bash -s <<REMOTE
set -euo pipefail
source "\$HOME/.cargo/env" 2>/dev/null || true
if [[ -f "\$HOME/spectra-tb-client.env" ]]; then
  # shellcheck disable=SC1091
  source "\$HOME/spectra-tb-client.env"
fi
export CARGO_TARGET_DIR=\$HOME/spectra-target-multidw
export CARGO_INCREMENTAL=0
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-4}
export SPECTRA_BENCH_HARDWARE=${HW}
export SPECTRA_BENCH_CONCURRENCY=${SPECTRA_BENCH_CONCURRENCY:-64}
export SPECTRA_BENCH_DURATION_SECS=${DURATION}
export SPECTRA_BENCH_DW_N=${DW_N}
export SPECTRA_MULTIDW_DW_KIND=${DW_KIND}
export SPECTRA_MULTIDW_RUN_SW7=${SPECTRA_MULTIDW_RUN_SW7:-1}
export SPECTRA_MULTIDW_RUN_BASE=${SPECTRA_MULTIDW_RUN_BASE:-1}
export SPECTRA_BENCH_WRITER_LADDER=${SPECTRA_BENCH_WRITER_LADDER:-1,2}
export SPECTRA_BENCH_BATCH_SWEEP=${SPECTRA_BENCH_BATCH_SWEEP:-512,2048}
export SPECTRA_BENCH_REPORT_DIR=${REMOTE_DIR}/profiling/spectra-bench/reports
export INSTANCES_ENV=${REMOTE_DIR}/infra/aws/spectra-multidw/instances.env
export SPECTRA_BENCH_UTIL_DIR=\$HOME/spectra-multidw-util
cd ${REMOTE_DIR}
chmod +x infra/aws/spectra-multidw/*.sh infra/aws/spectra-multidw/scripts/*.sh
nohup ./infra/aws/spectra-multidw/run-multidw-aws.sh > \$HOME/spectra-multidw-campaign.log 2>&1 &
echo "campaign PID=\$! log=\$HOME/spectra-multidw-campaign.log"
while kill -0 \$! 2>/dev/null; do sleep 20; done
wait \$!
tail -40 \$HOME/spectra-multidw-campaign.log
REMOTE

# Fetch DW util samples onto writer then merge
for i in $(seq 0 $((DW_N - 1))); do
  var="DW_${i}_PUBLIC_IP"
  dhost="${!var:-}"
  [[ -n "$dhost" ]] || continue
  scp "${SSH_OPTS[@]}" "${SSH_USER}@${dhost}:/home/${SSH_USER}/dw-util-${i}.json" \
    "/tmp/dw-util-${i}.json" 2>/dev/null || true
  if [[ -f "/tmp/dw-util-${i}.json" ]]; then
    scp "${SSH_OPTS[@]}" "/tmp/dw-util-${i}.json" \
      "${SSH_USER}@${HOST}:/home/${SSH_USER}/spectra-multidw-util/dw-${DW_KIND}-${i}.json" || true
  fi
done

# Patch reports on writer with merged util
ssh "${SSH_OPTS[@]}" "${SSH_USER}@${HOST}" bash -s <<'REMOTE'
set -euo pipefail
UTIL_DIR="$HOME/spectra-multidw-util"
SUMMARY="$UTIL_DIR/host-util-summary.json"
python3 - <<'PY'
import json, glob, os
util_dir = os.path.expanduser("~/spectra-multidw-util")
paths = [p for p in sorted(glob.glob(util_dir + "/*.json")) if not p.endswith("host-util-summary.json")]
out = []
for p in paths:
    try:
        with open(p) as f:
            out.append(json.load(f))
    except Exception:
        pass
summary = util_dir + "/host-util-summary.json"
with open(summary, "w") as f:
    json.dump(out, f, indent=2)
report_dir = "/tmp/spectra-aws/profiling/spectra-bench/reports"
for rp in glob.glob(report_dir + "/multidw-*.json"):
    with open(rp) as f:
        doc = json.load(f)
    if isinstance(doc, list):
        continue
    doc["host_util"] = out
    # Refresh binding_tier from util if unset
    writer_peak = max((u.get("cpu_peak_pct") or 0) for u in out if "writer" in str(u.get("role",""))) if out else 0
    dw_peak = max((u.get("cpu_peak_pct") or 0) for u in out if str(u.get("role","")).startswith("dw")) if out else 0
    if writer_peak >= 85 and dw_peak < 70:
        doc["binding_tier"] = "client-cpu"
    elif dw_peak >= 70:
        doc["binding_tier"] = "dw"
    else:
        doc["binding_tier"] = "unset"
    with open(rp, "w") as f:
        json.dump(doc, f, indent=2)
        f.write("\n")
print("patched reports with host_util entries", len(out))
PY
REMOTE

echo "Remote multi-DW campaign finished. Fetch with ./fetch-reports.sh"
