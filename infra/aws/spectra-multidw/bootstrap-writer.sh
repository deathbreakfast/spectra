#!/usr/bin/env bash
# Bootstrap writer host: Rust toolchain + sysstat (no DW services).
# For tensorbase campaigns, also installs clickhouse-client (required by Spectra TB adapter).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENV_FILE="${INSTANCES_ENV:-$ROOT/instances.env}"
# shellcheck disable=SC1091
source "$ENV_FILE"

HOST="${SPECTRA_WRITER_HOST:-${WRITER_PUBLIC_IP:-$WRITER_PRIVATE_IP}}"
SSH_KEY="${SSH_KEY_PATH:-$HOME/.ssh/${AWS_KEY_NAME:-}.pem}"
SSH_OPTS=(-o StrictHostKeyChecking=accept-new -i "$SSH_KEY")
DW_KIND="${SPECTRA_MULTIDW_DW_KIND:?}"
TB_VERSION="${TENSORBASE_VERSION:-v2021.07.05}"

echo "Bootstrapping writer ${SSH_USER}@${HOST}..."
for _ in $(seq 1 30); do
  if ssh "${SSH_OPTS[@]}" "${SSH_USER}@${HOST}" "echo ok" >/dev/null 2>&1; then
    break
  fi
  sleep 5
done

ssh "${SSH_OPTS[@]}" "${SSH_USER}@${HOST}" bash -s <<REMOTE
set -euo pipefail
sudo apt-get update -qq
sudo apt-get install -y -qq curl pkg-config libssl-dev build-essential sysstat unzip
RUSTUP_INIT_SHA256="6c30b75a75b28a96fd913a037c8581b580080b6ee9b8169a3c0feb1af7fe8caf"
TB_CLIENT_SHA256="e15e9568a8827364db40139cf8c2cc184223d89beba85545fe5387f85bb648aa"
verify_sha256() {
  local file="\$1" expected="\$2" actual
  actual="\$(sha256sum "\$file" | awk '{ print \$1 }')"
  if [[ "\$actual" != "\$expected" ]]; then
    echo "SHA-256 mismatch for \$file (expected \$expected, got \$actual)" >&2
    exit 1
  fi
}
if ! command -v cargo >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf -o /tmp/rustup-init.sh https://sh.rustup.rs
  verify_sha256 /tmp/rustup-init.sh "\$RUSTUP_INIT_SHA256"
  sh /tmp/rustup-init.sh -y
  rm -f /tmp/rustup-init.sh
fi
source "\$HOME/.cargo/env" || true
rustc --version

# TensorBase adapter needs clickhouse-client on the writer for tcp:// URLs.
if [[ "${DW_KIND}" == "tensorbase" ]]; then
  CLIENT_DIR="\$HOME/clickhouse-client-bin"
  mkdir -p "\$CLIENT_DIR"
  if [[ ! -x "\$CLIENT_DIR/clickhouse-client" ]]; then
    curl -fsSL -o /tmp/ch_client.zip \
      "https://github.com/tensorbase/tensorbase/releases/download/${TB_VERSION}/clickhouse_client_repack_linux.zip"
    verify_sha256 /tmp/ch_client.zip "\$TB_CLIENT_SHA256"
    unzip -o /tmp/ch_client.zip clickhouse-client -d "\$CLIENT_DIR"
    chmod +x "\$CLIENT_DIR/clickhouse-client"
  fi
  echo "export SPECTRA_CLICKHOUSE_CLIENT_PATH=\$CLIENT_DIR/clickhouse-client" > "\$HOME/spectra-tb-client.env"
fi
REMOTE

echo "Writer bootstrap done."
