#!/usr/bin/env bash
# Wait until TensorBase native port accepts TCP connections.
set -euo pipefail

URL="${SPECTRA_TENSORBASE_URL:-tcp://127.0.0.1:9528}"
HOST_PORT="${URL#*://}"
HOST="${HOST_PORT%%:*}"
PORT="${HOST_PORT##*:}"

echo "Waiting for TensorBase at ${HOST}:${PORT}..."
for _ in $(seq 1 90); do
  if (command -v nc >/dev/null 2>&1 && nc -z "$HOST" "$PORT" 2>/dev/null) \
    || timeout 1 bash -c "echo >/dev/tcp/${HOST}/${PORT}" 2>/dev/null; then
    echo "TensorBase is up."
    exit 0
  fi
  sleep 2
done
echo "TensorBase wait timeout (see ~/tensorbase-smoke/server.log on host)" >&2
exit 1
