#!/usr/bin/env bash
# Bootstrap Spectra AWS EC2: Docker ClickHouse, TensorBase server, Rust toolchain.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENV_FILE="${INSTANCES_ENV:-$ROOT/instances.env}"
# shellcheck disable=SC1091
source "$ENV_FILE"

HOST="${SMOKE_PUBLIC_IP:-$SMOKE_IP}"
SSH_KEY="${SSH_KEY_PATH:-$HOME/.ssh/${AWS_KEY_NAME:-}.pem}"
SSH_OPTS=(-o StrictHostKeyChecking=accept-new -i "$SSH_KEY")

echo "Bootstrapping ${SSH_USER}@${HOST}..."
for _ in $(seq 1 30); do
  if ssh "${SSH_OPTS[@]}" "${SSH_USER}@${HOST}" "echo ok" >/dev/null 2>&1; then
    break
  fi
  sleep 5
done

ssh "${SSH_OPTS[@]}" "${SSH_USER}@${HOST}" bash -s <<'REMOTE'
set -euo pipefail

sudo apt-get update -qq
sudo apt-get install -y -qq docker.io curl pkg-config libssl-dev build-essential unzip

sudo usermod -aG docker "$USER" || true

# Digests pinned in infra/aws/checksums/SHA256SUMS (TensorBase v2021.07.05).
TB_BASE_SHA256="44de56edd69f25e0219bebf22ffdd1e91a815611b79948436af943b4784c3945"
TB_CLIENT_SHA256="e15e9568a8827364db40139cf8c2cc184223d89beba85545fe5387f85bb648aa"
RUSTUP_INIT_SHA256="6c30b75a75b28a96fd913a037c8581b580080b6ee9b8169a3c0feb1af7fe8caf"

verify_sha256() {
  local file="$1" expected="$2"
  local actual
  actual="$(sha256sum "$file" | awk '{ print $1 }')"
  if [[ "$actual" != "$expected" ]]; then
    echo "SHA-256 mismatch for $file" >&2
    echo "  expected: $expected" >&2
    echo "  actual:   $actual" >&2
    exit 1
  fi
}

if ! command -v cargo >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf -o /tmp/rustup-init.sh https://sh.rustup.rs
  verify_sha256 /tmp/rustup-init.sh "$RUSTUP_INIT_SHA256"
  sh /tmp/rustup-init.sh -y
  rm -f /tmp/rustup-init.sh
fi
# shellcheck disable=SC1091
source "$HOME/.cargo/env" || true

sudo docker rm -f spectra-clickhouse 2>/dev/null || true
sudo docker run -d --name spectra-clickhouse \
  -p 8123:8123 \
  clickhouse/clickhouse-server:24

TB_VERSION="${TENSORBASE_VERSION:-v2021.07.05}"
TB_DIR="$HOME/tensorbase-smoke"
rm -rf "$TB_DIR"
mkdir -p "$TB_DIR"
cd "$TB_DIR"

curl -fsSL -o base_linux.zip \
  "https://github.com/tensorbase/tensorbase/releases/download/${TB_VERSION}/base_linux.zip"
verify_sha256 base_linux.zip "$TB_BASE_SHA256"
unzip -o base_linux.zip
chmod +x server 2>/dev/null || chmod +x ./bin/server 2>/dev/null || true

SERVER_BIN=""
if [[ -x ./server ]]; then
  SERVER_BIN=./server
elif [[ -x ./bin/server ]]; then
  SERVER_BIN=./bin/server
else
  SERVER_BIN="$(find . -maxdepth 3 -type f -name server -perm -111 | head -1)"
fi
if [[ -z "$SERVER_BIN" || ! -x "$SERVER_BIN" ]]; then
  echo "TensorBase server binary not found in base_linux.zip" >&2
  find . -maxdepth 3 -type f | head -20 >&2
  exit 1
fi

# Prefer persistent dirs under $HOME (survives reboot); rewrite base.conf if needed.
TB_DATA="${HOME}/tensorbase-smoke/data"
TB_SCHEMA="${HOME}/tensorbase-smoke/schema"
mkdir -p "$TB_DATA" "$TB_SCHEMA" /tmp/tb_schema /tmp/tb_data
if [[ -f "$TB_DIR/base.conf" ]]; then
  sed -i "s|/tmp/tb_data|${TB_DATA}|g; s|/tmp/tb_schema|${TB_SCHEMA}|g" "$TB_DIR/base.conf" || true
fi

# clickhouse-client for TensorBase truncate in smoke cleanup
if [[ ! -x "${HOME}/tensorbase-smoke/clickhouse-client" ]]; then
  curl -fsSL -o "${TB_DIR}/clickhouse_client_repack_linux.zip" \
    "https://github.com/tensorbase/tensorbase/releases/download/${TB_VERSION}/clickhouse_client_repack_linux.zip"
  verify_sha256 "${TB_DIR}/clickhouse_client_repack_linux.zip" "$TB_CLIENT_SHA256"
  unzip -o "${TB_DIR}/clickhouse_client_repack_linux.zip" clickhouse-client -d "${TB_DIR}"
  chmod +x "${TB_DIR}/clickhouse-client"
fi

pkill -f 'tensorbase.*server' 2>/dev/null || true
pkill -f './server -c' 2>/dev/null || true
nohup "$SERVER_BIN" -c "$TB_DIR/base.conf" >"$TB_DIR/server.log" 2>&1 &
echo $! >"$TB_DIR/server.pid"

echo "Bootstrap remote services started."
REMOTE

echo "Bootstrap complete."
