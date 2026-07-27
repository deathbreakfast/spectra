# Spectra examples

Runnable sketches for wiring `Spectra::builder()`, typed schema emit, and the publish-consume split. Start with the canonical path below; branch when you need a specific backend or topology.

Full runbooks (env vars, Docker, multi-terminal rules): [`../README.md` — How to run examples](../README.md#how-to-run-examples).

Optional isolated target dir (avoids rebuild churn when hopping examples):

```bash
export CARGO_BUILD_JOBS=1 CARGO_TARGET_DIR=target-spectra-extract
```

## Canonical path

### 1. Embedded + schema emit — [`quickstart_schema_emit.rs`](quickstart_schema_emit.rs)

Proves typed helpers, inventory registration, and in-process query against mem backends — the baseline before you add transport or remote storage.

```bash
cargo run -p uf-spectra --example quickstart_schema_emit --features mem
```

Success: `schema emit OK: … metric point(s) persisted`.

### 2. Real publish-consume bus — [`bus_publish.rs`](bus_publish.rs) · [`bus_consume.rs`](bus_consume.rs)

Two processes over a TCP JSON-lines bus (host-owned adapter — swap for Photon/NATS in production). Start **consumer first**.

```bash
export SPECTRA_BUS_ADDR=127.0.0.1:9809
# Terminal 1
cargo run -p uf-spectra --example bus_consume --features mem
# Terminal 2
cargo run -p uf-spectra --example bus_publish --features mem
```

Success: `bus-consume OK: … metric point(s) in storage` and `bus-publish OK: counter sent …`.

Sketches without a process boundary (RecordingSink / in-process envelope): [`quickstart_publish_only.rs`](quickstart_publish_only.rs) · [`quickstart_consume_forward.rs`](quickstart_consume_forward.rs).

### 3. Remote ClickHouse — [`quickstart_clickhouse_emit.rs`](quickstart_clickhouse_emit.rs)

Direct persist into a live ClickHouse — validates remote adapter wiring before you split publisher and consumer across processes.

```bash
docker compose -f docker-compose.dev.yml up -d clickhouse
export SPECTRA_ALLOW_INSECURE_REMOTE=1
export SPECTRA_CLICKHOUSE_URL=http://127.0.0.1:8123
cargo run -p uf-spectra --example quickstart_clickhouse_emit --features clickhouse
```

Success: `clickhouse emit OK: … metric point(s), … event row(s)`.

## Other examples

| Example | When you'd open it | Command | Success signal |
|---------|-------------------|---------|----------------|
| [`quickstart.rs`](quickstart.rs) | Smallest boot — mem backends + `tracing_subscriber` | `cargo run -p uf-spectra --example quickstart --features mem` | `Spectra booted with in-memory backends (embedded)` |
| [`quickstart_sqlite.rs`](quickstart_sqlite.rs) | Durable embedded wiring on disk | `cargo run -p uf-spectra --example quickstart_sqlite --features sqlite` | `SQLite backends ready under …` |
| [`quickstart_transport.rs`](quickstart_transport.rs) | Dual-path — bus mirror and local persist in one process | `cargo run -p uf-spectra --example quickstart_transport --features mem` | `transport + persist OK: … counter(s) in transport, … point(s) in storage` |
| [`clickhouse_dual_path.rs`](clickhouse_dual_path.rs) | Dual-path against live ClickHouse (+ RecordingSink) | Docker CH + `cargo run -p uf-spectra --example clickhouse_dual_path --features clickhouse` | `clickhouse dual-path OK: …` |
| [`query_and_pii_console.rs`](query_and_pii_console.rs) | Query events + `mask_field_value` for PII | `cargo run -p uf-spectra --example query_and_pii_console --features mem` | `query-and-pii OK: …` |
| [`custom_backend_stub.rs`](custom_backend_stub.rs) | Implement `MetricsStorageBackend` / `EventStorageBackend` | `cargo run -p uf-spectra --example custom_backend_stub --features mem` | `custom-backend-stub OK: …` |
| [`quickstart_telemetry.rs`](quickstart_telemetry.rs) | NDJSON mirror under a temp dir | `cargo run -p uf-spectra --example quickstart_telemetry --features mem,telemetry-console` | `telemetry NDJSON written under …` |
| [`quickstart_tensorbase_emit.rs`](quickstart_tensorbase_emit.rs) | Remote TensorBase adapter (needs `SPECTRA_TENSORBASE_URL`) | `cargo run -p uf-spectra --example quickstart_tensorbase_emit --features tensorbase` | `tensorbase emit OK: … metric point(s), … event row(s)` |
| [`quickstart_publish_only.rs`](quickstart_publish_only.rs) / [`quickstart_consume_forward.rs`](quickstart_consume_forward.rs) | In-process publish/consume sketches (no TCP) | `--example quickstart_publish_only` / `quickstart_consume_forward` `--features mem` | `publish-only OK` / `consume-forward OK` |

Topology reference: [Direct persist](https://docs.rs/uf-spectra/latest/spectra/index.html#direct-persist-one-binary) · [Publish-consume](https://docs.rs/uf-spectra/latest/spectra/index.html#publish-consume-two-binaries) · [Dual-path](https://docs.rs/uf-spectra/latest/spectra/index.html#dual-path-optional).
