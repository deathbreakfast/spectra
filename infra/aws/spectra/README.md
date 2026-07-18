# Spectra AWS E2E + bench

Co-located **ClickHouse** (Docker) + **TensorBase** (release binary) on a single EC2 instance.

Operator workflow: provision → bootstrap → full E2E → full bench → fetch reports → teardown.

## Prerequisites

- AWS CLI configured (`aws sts get-caller-identity`)
- EC2 key pair: `export AWS_KEY_NAME=your-key`
- SSH key: `export SSH_KEY_PATH=$HOME/.ssh/your-key.pem`
- Optional: `export AWS_REGION=us-west-2` (default), `export SPECTRA_AWS_INSTANCE_TYPE=t3.xlarge`
- Provision uses the **default VPC** in the region (override AMI with `AWS_AMI_ID` if needed). AMI owner `099720109477` is Canonical Ubuntu.

## Full gate (laptop)

```bash
cd infra/aws/spectra
chmod +x *.sh scripts/*.sh

export AWS_KEY_NAME=your-key
export SSH_KEY_PATH=$HOME/.ssh/your-key.pem

./provision.sh
./bootstrap.sh
./deploy-and-run-e2e.sh
./deploy-and-run-bench.sh
./fetch-reports.sh
./teardown.sh
```

After `fetch-reports.sh`, fill scoreboards in [`docs/bench/PERFORMANCE_STUDY.md`](../../../docs/bench/PERFORMANCE_STUDY.md) from JSON under `profiling/spectra-bench/reports/`.

## On-host scripts

| Script | Purpose |
|--------|---------|
| `run-e2e-aws.sh` | Embedded sanity + full ignored remote catalog + live contracts + examples |
| `run-bench-aws.sh` | Full BM-* matrix via [`scripts/run-bench-matrix.sh`](../../../scripts/run-bench-matrix.sh) |
| `scripts/export-env-aws.sh` | Localhost `SPECTRA_*_URL` |
| `scripts/wait-*.sh` / `cleanup-remote-tables.sh` | Service readiness + truncate |

## Env on host

```bash
SPECTRA_CLICKHOUSE_URL=http://127.0.0.1:8123
SPECTRA_TENSORBASE_URL=tcp://127.0.0.1:9528
SPECTRA_BENCH_HARDWARE=aws-t3-xlarge   # stamped into report JSON
# Optional: CARGO_BUILD_JOBS=4 (default on EC2 runners); set 1 on constrained laptops
```

DB ports stay on localhost (no public DB ingress).

## Services

| Service | Port | Bootstrap |
|---------|------|-----------|
| ClickHouse | 8123 | Docker (or `docker-compose.data.yml`) |
| TensorBase | 9528 | `base_linux.zip` from TensorBase release + bundled `clickhouse-client` |

## Instance sizing

Default: **t3.xlarge** (`SPECTRA_AWS_INSTANCE_TYPE`). `t3.large` can OOM or starve SSH under firehose (C≥32). Campaign default concurrency is 64 (`SPECTRA_BENCH_CONCURRENCY`).

## Local ClickHouse-only smoke

```bash
docker run -d -p 8123:8123 --name spectra-ch clickhouse/clickhouse-server:24
export SPECTRA_CLICKHOUSE_URL=http://127.0.0.1:8123
cargo test -p spectra-e2e --features clickhouse --test scenarios -- --ignored --test-threads=1
```

## Related

Multi-DW durable campaigns: [`../spectra-multidw/`](../spectra-multidw/README.md).
