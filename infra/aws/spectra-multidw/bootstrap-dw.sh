#!/usr/bin/env bash
# Bootstrap one DW host: ClickHouse (Docker) or TensorBase (binary).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENV_FILE="${INSTANCES_ENV:-$ROOT/instances.env}"
# shellcheck disable=SC1091
source "$ENV_FILE"

IDX="${1:?usage: bootstrap-dw.sh <index>}"
DW_KIND="${SPECTRA_MULTIDW_DW_KIND:?}"
var_pub="DW_${IDX}_PUBLIC_IP"
var_priv="DW_${IDX}_PRIVATE_IP"
HOST="${!var_pub:-${!var_priv}}"
SSH_KEY="${SSH_KEY_PATH:-$HOME/.ssh/${AWS_KEY_NAME:-}.pem}"
SSH_OPTS=(-o StrictHostKeyChecking=accept-new -i "$SSH_KEY")

echo "Bootstrapping DW ${IDX} ${SSH_USER}@${HOST} (${DW_KIND})..."
for _ in $(seq 1 30); do
  if ssh "${SSH_OPTS[@]}" "${SSH_USER}@${HOST}" "echo ok" >/dev/null 2>&1; then
    break
  fi
  sleep 5
done

if [[ "$DW_KIND" == "clickhouse" ]]; then
  ssh "${SSH_OPTS[@]}" "${SSH_USER}@${HOST}" bash -s <<'REMOTE'
set -euo pipefail
sudo apt-get update -qq
sudo apt-get install -y -qq docker.io curl sysstat
sudo usermod -aG docker "$USER" || true
sudo docker rm -f spectra-clickhouse 2>/dev/null || true
# Bind on all interfaces so writer can reach private IP:8123
sudo docker run -d --name spectra-clickhouse \
  -p 8123:8123 \
  clickhouse/clickhouse-server:24
for _ in $(seq 1 60); do
  if curl -sf 'http://127.0.0.1:8123/ping' >/dev/null; then
    echo "ClickHouse ready"
    exit 0
  fi
  sleep 2
done
echo "ClickHouse failed to become ready" >&2
exit 1
REMOTE
else
  ssh "${SSH_OPTS[@]}" "${SSH_USER}@${HOST}" bash -s <<'REMOTE'
set -euo pipefail
sudo apt-get update -qq
sudo apt-get install -y -qq curl pkg-config libssl-dev unzip sysstat
TB_VERSION="${TENSORBASE_VERSION:-v2021.07.05}"
TB_BASE_SHA256="44de56edd69f25e0219bebf22ffdd1e91a815611b79948436af943b4784c3945"
TB_CLIENT_SHA256="e15e9568a8827364db40139cf8c2cc184223d89beba85545fe5387f85bb648aa"
verify_sha256() {
  local file="$1" expected="$2" actual
  actual="$(sha256sum "$file" | awk '{ print $1 }')"
  if [[ "$actual" != "$expected" ]]; then
    echo "SHA-256 mismatch for $file (expected $expected, got $actual)" >&2
    exit 1
  fi
}
TB_DIR="$HOME/tensorbase-smoke"
rm -rf "$TB_DIR"
mkdir -p "$TB_DIR"
cd "$TB_DIR"
curl -fsSL -o base_linux.zip \
  "https://github.com/tensorbase/tensorbase/releases/download/${TB_VERSION}/base_linux.zip"
verify_sha256 base_linux.zip "$TB_BASE_SHA256"
unzip -o base_linux.zip
SERVER_BIN=""
if [[ -x ./server ]]; then SERVER_BIN=./server
elif [[ -x ./bin/server ]]; then SERVER_BIN=./bin/server
else SERVER_BIN="$(find . -maxdepth 3 -type f -name server -perm -111 | head -1)"
fi
TB_DATA="${HOME}/tensorbase-smoke/data"
TB_SCHEMA="${HOME}/tensorbase-smoke/schema"
mkdir -p "$TB_DATA" "$TB_SCHEMA"
if [[ -f "$TB_DIR/base.conf" ]]; then
  sed -i "s|/tmp/tb_data|${TB_DATA}|g; s|/tmp/tb_schema|${TB_SCHEMA}|g" "$TB_DIR/base.conf" || true
fi
# Listen on all interfaces for private-IP writer access
if [[ -f "$TB_DIR/base.conf" ]]; then
  sed -i 's/127.0.0.1/0.0.0.0/g; s/"localhost"/"0.0.0.0"/g; s/localhost/0.0.0.0/g' "$TB_DIR/base.conf" || true
fi
# clickhouse-client used by Spectra TensorBase adapter from writer hosts — also keep on DW for ops
if [[ ! -x "${HOME}/tensorbase-smoke/clickhouse-client" ]]; then
  curl -fsSL -o "${TB_DIR}/clickhouse_client_repack_linux.zip" \
    "https://github.com/tensorbase/tensorbase/releases/download/${TB_VERSION}/clickhouse_client_repack_linux.zip"
  verify_sha256 "${TB_DIR}/clickhouse_client_repack_linux.zip" "$TB_CLIENT_SHA256"
  unzip -o "${TB_DIR}/clickhouse_client_repack_linux.zip" clickhouse-client -d "${TB_DIR}"
  chmod +x "${TB_DIR}/clickhouse-client"
fi
pkill -9 -f 'tensorbase-smoke' 2>/dev/null || true
pkill -9 -x server 2>/dev/null || true
sleep 1
ulimit -n 65535
nohup "$SERVER_BIN" -c "$TB_DIR/base.conf" >"$TB_DIR/server.log" 2>&1 &
for _ in $(seq 1 60); do
  if ss -lntp 2>/dev/null | grep -q ':9528'; then
    echo "TensorBase ready"
    ss -lntp | grep 9528 || true
    exit 0
  fi
  sleep 2
done
echo "TensorBase failed to become ready" >&2
tail -50 "$TB_DIR/server.log" >&2 || true
exit 1
REMOTE
fi

echo "DW ${IDX} bootstrap done."
