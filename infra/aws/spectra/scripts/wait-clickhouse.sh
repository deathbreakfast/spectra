#!/usr/bin/env bash
# Wait until ClickHouse HTTP /ping responds on localhost.
set -euo pipefail

URL="${SPECTRA_CLICKHOUSE_URL:-http://127.0.0.1:8123}"
PING_URL="${URL%/}/ping"

echo "Waiting for ClickHouse at ${PING_URL}..."
for _ in $(seq 1 60); do
  if curl -sf "$PING_URL" >/dev/null 2>&1; then
    echo "ClickHouse is up."
    exit 0
  fi
  sleep 2
done
echo "ClickHouse wait timeout" >&2
exit 1
