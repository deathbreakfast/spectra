# Spectra AWS multi-DW durable write campaign

Separate **writer EC2** and **dedicated warehouse EC2(s)** for durable Spectra→DW measurement.
Primary experiment: **BM-SW7** (L2 batched persist). BM-SW5/SW6 remain available as single-row protocol floor.

Co-located single-host campaign: [`../spectra/`](../spectra/).

## Prerequisites

- AWS CLI (`aws sts get-caller-identity`)
- `export AWS_KEY_NAME=your-key`
- `export SSH_KEY_PATH=~/.ssh/your-key.pem`
- Optional: `AWS_REGION`, `SPECTRA_AWS_INSTANCE_TYPE` (default `t3.xlarge`)
- Required: `SPECTRA_MULTIDW_DW_KIND=clickhouse` or `tensorbase`
- Optional: `SPECTRA_BENCH_DW_N=1` (default; n=2 not required for the L2 ceiling)

## Isolation

| Role | Count | Notes |
|------|-------|-------|
| Writer / bench | 1+ | Separate from DW for decision-grade |
| DW (CH or TB) | n | One EC2 per warehouse |
| Brokers | 0 | — |

## Operator flow

```bash
cd infra/aws/spectra-multidw
chmod +x *.sh scripts/*.sh

export AWS_KEY_NAME=your-key
export SSH_KEY_PATH=~/.ssh/your-key.pem
export SPECTRA_MULTIDW_DW_KIND=clickhouse
export SPECTRA_BENCH_DW_N=1
export SPECTRA_BENCH_BATCH_SWEEP=512,2048

./provision.sh
./bootstrap.sh
./deploy-and-run.sh
./fetch-reports.sh
# Scoreboard: docs/bench/PERFORMANCE_STUDY.md
./teardown.sh
```

## Env on writer (set by export-env-aws.sh)

```bash
SPECTRA_BENCH_DW_N=1
SPECTRA_CLICKHOUSE_URL_0=http://10.0.0.10:8123
SPECTRA_BENCH_HARDWARE=aws-t3-xlarge
SPECTRA_BENCH_HOST_UTIL_JSON=/path/to/host-util-summary.json
```

Shard: `SPECTRA_BENCH_CLIENT_INDEX % SPECTRA_BENCH_DW_N`.

## BM-SW7 (batched L2)

| Env | Default | Meaning |
|-----|---------|---------|
| `SPECTRA_MULTIDW_RUN_SW7` | `1` | Set `0` to skip SW7 |
| `SPECTRA_MULTIDW_RUN_BASE` | `1` | Set `0` to skip SW5/SW6 |
| `SPECTRA_BENCH_WRITER_LADDER` | `1,2` | Concurrent writer processes per cell |
| `SPECTRA_BENCH_BATCH_SWEEP` | `512,2048` | `PersistConfig.batch_max` via `--batch-max` |

## CPU / memory sampling

`scripts/sample-host-util.sh` runs on writer + each DW during the firehose window.
`deploy-and-run.sh` merges samples into `host-util-summary.json` and exports
`SPECTRA_BENCH_HOST_UTIL_JSON` for the bench report `host_util` field.

## Experiments

| ID | Workload |
|----|----------|
| **bm-sw7** | Batched durable counter (L2 `*_now` + `flush_persist`) — **primary** |
| bm-sw5 | Durable counter (single-row adapter) — floor |
| bm-sw6 | Durable event (single-row adapter) — floor |

Primary metric: `durable_*_ops_per_sec`. Prefer `binding_tier = dw` for instance projection.
