#!/usr/bin/env bash
# Ensure ClickHouse Docker + TensorBase are up (safe after reboot /tmp wipe).
set -euo pipefail

sudo docker start spectra-clickhouse 2>/dev/null \
  || sudo docker run -d --name spectra-clickhouse -p 8123:8123 clickhouse/clickhouse-server:24

TB_DIR="${TENSORBASE_DIR:-$HOME/tensorbase-smoke}"
if [[ ! -x "$TB_DIR/server" ]]; then
  echo "TensorBase server missing at $TB_DIR/server — run bootstrap.sh" >&2
  exit 1
fi

TB_DATA="${HOME}/tensorbase-smoke/data"
TB_SCHEMA="${HOME}/tensorbase-smoke/schema"
mkdir -p "$TB_DATA" "$TB_SCHEMA" /tmp/tb_schema /tmp/tb_data
if [[ -f "$TB_DIR/base.conf" ]]; then
  sed -i "s|/tmp/tb_data|${TB_DATA}|g; s|/tmp/tb_schema|${TB_SCHEMA}|g" "$TB_DIR/base.conf" || true
fi
if ! ss -ltn | grep -q ':9528'; then
  pkill -f './server -c' 2>/dev/null || true
  # TensorBase leaks FDs under heavy insert; raise soft limit for the server process.
  (
    ulimit -n 65535 || true
    nohup "$TB_DIR/server" -c "$TB_DIR/base.conf" >"$TB_DIR/server.log" 2>&1 &
    echo $! >"$TB_DIR/server.pid"
  )
fi
