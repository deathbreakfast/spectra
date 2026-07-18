#!/usr/bin/env bash
# Terminate writer + all DW instances and delete security group.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENV_FILE="${INSTANCES_ENV:-$ROOT/instances.env}"

if [[ ! -f "$ENV_FILE" ]]; then
  echo "No ${ENV_FILE}; nothing to tear down." >&2
  exit 0
fi

# shellcheck disable=SC1091
source "$ENV_FILE"

AWS_REGION="${AWS_REGION:-us-west-2}"
DW_N="${SPECTRA_BENCH_DW_N:-1}"

IDS=()
[[ -n "${INSTANCE_WRITER:-}" ]] && IDS+=("$INSTANCE_WRITER")
for i in $(seq 0 $((DW_N - 1))); do
  var="INSTANCE_DW_${i}"
  [[ -n "${!var:-}" ]] && IDS+=("${!var}")
done

if [[ ${#IDS[@]} -gt 0 ]]; then
  echo "Terminating ${IDS[*]}..."
  aws ec2 terminate-instances --region "$AWS_REGION" --instance-ids "${IDS[@]}" >/dev/null
  aws ec2 wait instance-terminated --region "$AWS_REGION" --instance-ids "${IDS[@]}" || true
fi

if [[ -n "${SECURITY_GROUP_ID:-}" ]]; then
  echo "Deleting security group ${SECURITY_GROUP_ID}..."
  # Retry a few times — ENIs can lag after terminate
  for _ in $(seq 1 10); do
    if aws ec2 delete-security-group --region "$AWS_REGION" --group-id "$SECURITY_GROUP_ID" 2>/dev/null; then
      break
    fi
    sleep 10
  done
fi

rm -f "$ENV_FILE"
echo "Teardown complete."
