#!/usr/bin/env bash
# Terminate Spectra AWS EC2 and delete security group.
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

if [[ -n "${INSTANCE_SMOKE:-}" ]]; then
  echo "Terminating ${INSTANCE_SMOKE}..."
  aws ec2 terminate-instances --region "$AWS_REGION" --instance-ids "$INSTANCE_SMOKE" >/dev/null
  aws ec2 wait instance-terminated --region "$AWS_REGION" --instance-ids "$INSTANCE_SMOKE" || true
fi

if [[ -n "${SECURITY_GROUP_ID:-}" ]]; then
  echo "Deleting security group ${SECURITY_GROUP_ID}..."
  aws ec2 delete-security-group --region "$AWS_REGION" --group-id "$SECURITY_GROUP_ID" || true
fi

rm -f "$ENV_FILE"
echo "Teardown complete."
