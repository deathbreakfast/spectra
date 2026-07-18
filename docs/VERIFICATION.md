# Verification baseline

Re-run after test-harness or coverage changes. See `./scripts/verify-release.sh` for release gates.

## Commands

```bash
export CARGO_BUILD_JOBS=1
export CARGO_TARGET_DIR=target-spectra-extract

# Upstream gates
./scripts/gate-check.sh

# Unit + integration (exclude e2e/bench drivers)
cargo test --workspace --exclude spectra-e2e --exclude spectra-bench

# Matrix correctness
export CARGO_TARGET_DIR=target-spectra-e2e
cargo test -p spectra-e2e

# Storage port contract (PR CI)
cargo test -p spectra-backend-mem --test storage_contract
cargo test -p spectra-backend-sqlite --test storage_contract
cargo test -p spectra-backend-tensorbase --test storage_contract
cargo test -p spectra-backend-clickhouse --test storage_contract

# Release verification
./scripts/verify-release.sh

# Supply-chain (CI also runs this)
cargo deny check
# Optional complementary advisory scan
cargo audit
```

## AWS full E2E + bench (manual / tag-adjacent)

PR CI: embedded e2e + stub contracts only. Live remote catalog and capacity campaigns run on AWS.

```bash
cd infra/aws/spectra
export AWS_KEY_NAME=your-key
export SSH_KEY_PATH=~/.ssh/your-key.pem

./provision.sh
./bootstrap.sh
./deploy-and-run-e2e.sh      # full ignored remote catalog + live contracts
./deploy-and-run-bench.sh    # full BM-* × all backends
./fetch-reports.sh           # → profiling/spectra-bench/reports/
./teardown.sh
```

Then fill scoreboards in [`docs/bench/PERFORMANCE_STUDY.md`](bench/PERFORMANCE_STUDY.md) from the fetched JSON.

On-host only (after bootstrap):

```bash
./infra/aws/spectra/run-e2e-aws.sh
./infra/aws/spectra/run-bench-aws.sh
```

Details: [`infra/aws/spectra/README.md`](../infra/aws/spectra/README.md).

### Multi-DW durable write (BM-SW7 primary)

Separate writer + DW EC2s. Primary capacity experiment is **BM-SW7** (L2 batch). BM-SW5/SW6 are single-row protocol floor.

```bash
cd infra/aws/spectra-multidw
export AWS_KEY_NAME=your-key SSH_KEY_PATH=~/.ssh/your-key.pem
export SPECTRA_MULTIDW_DW_KIND=clickhouse
export SPECTRA_BENCH_DW_N=1
export SPECTRA_BENCH_BATCH_SWEEP=512,2048

./provision.sh && ./bootstrap.sh
./deploy-and-run.sh && ./fetch-reports.sh
./teardown.sh
```

Details: [`infra/aws/spectra-multidw/README.md`](../infra/aws/spectra-multidw/README.md). Scoreboard: [`docs/bench/PERFORMANCE_STUDY.md`](bench/PERFORMANCE_STUDY.md).

## Baseline results

| Check | Result |
|-------|--------|
| `cargo test --workspace --exclude spectra-e2e --exclude spectra-bench` | Run after changes |
| `cargo test -p spectra-e2e` | CI embedded matrix scenarios |
| Storage contract tests (mem/sqlite/tensorbase/clickhouse stubs) | Run after changes |
| `./scripts/verify-release.sh` | Required before release tag |

## Line coverage (CI artifact)

PR CI runs a non-blocking [`coverage`](../.github/workflows/ci.yml) job with `cargo-llvm-cov`:

```bash
# Install once
cargo install cargo-llvm-cov --locked

# Summary to stdout (CI scope — excludes e2e/bench)
./scripts/coverage.sh --summary-only

# Full workspace including e2e
./scripts/coverage.sh --full --summary-only

# LCOV for local inspection
./scripts/coverage.sh --lcov --output-path lcov.info
```

Download `coverage-lcov` from the GitHub Actions run artifacts for the CI report.

**Baseline (2026-07-08):** ~63% line coverage on the CI-scoped slice (excludes `spectra-e2e` and `spectra-bench`). Run with `--test-threads=1` under instrumentation to avoid timing flakes in `spectra-runtime` builder tests.

## Coverage notes

- Behavioral coverage matrix: [`spectra-e2e/README.md`](../spectra-e2e/README.md)
- Shared storage contract: [`spectra-testkit/src/storage_contract.rs`](../spectra-testkit/src/storage_contract.rs)
- Scenario catalog: [`spectra-testkit/src/catalog.rs`](../spectra-testkit/src/catalog.rs)
