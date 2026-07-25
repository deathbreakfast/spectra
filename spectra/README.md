# spectra (`uf-spectra`)

Typed metrics, structured event logs, and pluggable storage for Rust services.

## Audience

| Reader | Use this crate for |
|--------|-------------------|
| **App developers** | Primary dependency; enable backend features explicitly |
| **Integrators** | `Spectra::builder()` and prelude re-exports |

## Role

- Re-exports `spectra-core`, `spectra-runtime`, and feature-gated backends
- `default = ["mem"]`; optional `sqlite`, `tensorbase`, `clickhouse`, `telemetry-console`
- CI demo schemas (`platform_smoke_*`) register via inventory; macros emit typed helpers and topics in linked modules

## Configuration

Spectra has no global config file loader. Settings merge in this order (highest wins):

1. **Explicit builder calls** — `.config(SpectraConfig { ... })`, `.sink(...)`, `.persist_disabled()`, backend constructors
2. **`SpectraConfig`** — programmatic overrides passed to `.config()`
3. **Environment variables** — read by `SpectraConfig::from_env()` when `.config()` is omitted
4. **Schema defaults** — per-metric/event levels and sample rates from the DSL
5. **Library defaults** — documented below

### Cargo features

Enable backends at compile time on your `spectra` dependency. See the root [README](../README.md#cargo-features).

### Builder composition

| Wiring | Calls | Role |
|--------|-------|------|
| Direct persist (default) | `.metrics_backend(..).events_backend(..).build()` | Emit process writes storage |
| Dual-path | `.sink(transport).build()` | Bus mirror + local persist |
| Publish only (publish-consume) | `.sink(transport).persist_disabled().build()` | **Publisher** — consumers write storage |

Publisher/consumer setup: `cargo doc -p uf-spectra --open` → **Getting started → Publish-consume**, then
`SpectraSink`, `topics`, and examples `quickstart_publish_only` /
`quickstart_consume_forward`.

### Emit gate and sampling

Loaded by `SpectraConfig::from_env()` unless `.config(...)` overrides.

| Variable | Default | Effect |
|----------|---------|--------|
| `SPECTRA_GATE` | on | Request disable with `0`/`false`/`no` (ignored unless force-off is set) |
| `SPECTRA_GATE_FORCE_OFF` | unset | Set `1`/`true`/`yes` with `SPECTRA_GATE=0` to actually disable the emit gate |
| `SPECTRA_LEVEL` | `info` | Global minimum verbosity (`error` … `trace`) |
| `SPECTRA_SAMPLE_RATE` | `1.0` | Global sample floor after level check |
| `SPECTRA_SAMPLE_<NAME>` | — | Per metric/event name override (`0.0`–`1.0`) |
| `SPECTRA_CONFIG` | — | Path to TOML file with a `[spectra]` table |

Event query paging is clamped in-library (`MAX_EVENT_QUERY_LIMIT` = 1000). Metric/table/field identifiers must match `validate_spectra_ident`. See repository [`SECURITY.md`](../SECURITY.md).

### Emit buffer (embedded profile)

| Variable | Default | Effect |
|----------|---------|--------|
| `SPECTRA_REQUEST_BUFFER` | on | Buffer emits for web request scopes |
| `SPECTRA_JOB_BUFFER` | on | Buffer emits for worker scopes |
| `SPECTRA_COUNTER_AGGREGATE` | on | Coalesce counter deltas while buffering |

Set any of these to `0`/`false`/`no` to disable.

**Web note:** prefer `try_record_*_now` / generated helpers (L2 enqueue) over `request_scope`.
`request_scope` drops undrained emits on panic or early exit — avoid if you need failure telemetry.

### Async storage persist (builder)

Configure L2 queue/batch on `Spectra::builder()` — **not** environment variables:

```rust
use std::time::Duration;
use spectra::{PersistConfig, Spectra};

Spectra::builder()
    // …backends…
    .persist(PersistConfig {
        queue_max: 8192,                      // overflow policy applies when full
        batch_max: 2048,                      // raise for DW firehose
        batch_wait: Duration::from_millis(5), // coalesce delay
        batch_enabled: true,                  // use batch insert APIs
    })
    .build()?;
```

| Field | Default | Role |
|-------|---------|------|
| `queue_max` | 8192 | Bound L2 mpsc; see `overflow` |
| `overflow` | `Drop` | `Drop` (lossy, default) or `Block` (backpressure) |
| `batch_max` | 32 | Max jobs per batch insert |
| `batch_wait` | 5ms | Coalesce delay when batch still size 1 |
| `batch_enabled` | true | Use `record_*_batch` APIs |

After fire-and-forget `*_now` emits, scripts that need durability before exit call
`spectra.flush_persist().await`.

### Telemetry console (`telemetry-console` feature)

| Variable | Default | Effect |
|----------|---------|--------|
| `SPECTRA_CONSOLE` | off | Mirror safe fields to stderr |
| `SPECTRA_SYNC_HOT_PATH` | off | Invoke transport sink on emit thread |

Use `.telemetry_ndjson(dir)` on the builder to write `{dir}/metrics.ndjson` and `{dir}/events.ndjson`.

### Remote backends

| Variable | Used by |
|----------|---------|
| `SPECTRA_TENSORBASE_URL` | `tensorbase` feature — integration tests and adapters |
| `SPECTRA_CLICKHOUSE_URL` | `clickhouse` feature — integration tests and adapters |

### Debug

| Variable | Effect |
|----------|--------|
| `COUNTER_ROOTCAUSE` | Enable internal persist-path counters (debugging) |

## Backend wiring

### In-memory (default)

```rust
use spectra::{MemEventsBackend, MemMetricsBackend, Spectra};

let _spectra = Spectra::builder()
    .metrics_backend(std::sync::Arc::new(MemMetricsBackend::new()))
    .events_backend(std::sync::Arc::new(MemEventsBackend::new()))
    .embedded()
    .build()?;
```

### SQLite (durable embedded)

```rust
use spectra::{SqliteEventsBackend, SqliteMetricsBackend, Spectra};

let metrics = SqliteMetricsBackend::new("/tmp/spectra-metrics.db")?;
let events = SqliteEventsBackend::new("/tmp/spectra-events.db")?;
let _spectra = Spectra::builder()
    .metrics_backend(std::sync::Arc::new(metrics))
    .events_backend(std::sync::Arc::new(events))
    .embedded()
    .build()?;
```

Requires `features = ["sqlite"]`.

### Remote (ClickHouse / TensorBase)

See [`spectra-backend-clickhouse/README.md`](../spectra-backend-clickhouse/README.md) and [`spectra-backend-tensorbase/README.md`](../spectra-backend-tensorbase/README.md).

## Schema collection

Your application owns telemetry DSL modules and links them with an explicit `mod` list.
This repository demonstrates the contract with CI demo schemas under `schemas/` and re-exports
smoke `helpers` and `topics` from those expansions.

## How to run examples

Canonical teaching path (start here). Topology docs:
[Direct persist](https://docs.rs/uf-spectra/latest/spectra/index.html#direct-persist-one-binary) /
[Publish-consume](https://docs.rs/uf-spectra/latest/spectra/index.html#publish-consume-two-binaries) /
[Dual-path](https://docs.rs/uf-spectra/latest/spectra/index.html#dual-path-optional).

```bash
export CARGO_BUILD_JOBS=1 CARGO_TARGET_DIR=target-spectra-extract
```

### 1. Embedded + schema emit — `quickstart_schema_emit` (standalone)

One process, in-memory store, typed helpers + query. No external services.

```bash
cargo run -p uf-spectra --example quickstart_schema_emit --features mem
```

Success: stderr prints `schema emit OK: … metric point(s) persisted`.

### 2. Publish-consume (conceptual set — run as sketches)

Publisher and consumer are **separate binaries** in production. Spectra does **not** ship a
message bus; your host owns that piece. The examples below are **standalone sketches**
(`RecordingSink` / in-process envelope) so each command runs alone without a broker.

| Rule | Detail |
|------|--------|
| Shared schemas | Same `spectra_*!` modules (`mod`-linked) on publisher and consumer |
| Start order (production) | Consumer + bus first, then one or more publishers |
| Publishers | Each app process uses `.sink(...).persist_disabled()`; unique host instance IDs if you run a fleet |
| Consumers | Own storage (persist on); decode `*Payload` → `try_record_*_at` / `try_log_event_at` |
| Auth / bus | Host responsibility (Photon, NATS, Kafka, …) |

**Local sketches** (no bus required):

```bash
# Terminal A — publisher sketch (transport receives emit; storage empty)
cargo run -p uf-spectra --example quickstart_publish_only --features mem

# Terminal B — consumer sketch (decode envelope → persist; timestamp preserved)
cargo run -p uf-spectra --example quickstart_consume_forward --features mem
```

**Production-shaped remote consumer** (host bus + ClickHouse) — wire your subscriber, then:

```bash
export SPECTRA_CLICKHOUSE_URL=http://127.0.0.1:8123
# Consumer binary: backends + .build() (persist on); subscribe on your bus
# Publisher binary: .sink(bus).persist_disabled().build(); emit with typed helpers
```

See rustdoc **Getting started → Publish-consume** for sink and consumer code sketches.

### 3. Remote storage — `quickstart_clickhouse_emit` (standalone)

Direct persist into ClickHouse. Requires a live ClickHouse (Docker Compose below).

```bash
docker compose -f docker-compose.dev.yml up -d clickhouse
export SPECTRA_CLICKHOUSE_URL=http://127.0.0.1:8123
cargo run -p uf-spectra --example quickstart_clickhouse_emit --features clickhouse
```

Success: stderr prints `clickhouse emit OK: … metric point(s), … event row(s)`.

### Other examples

| Example | Topology | Features | Notes |
|---------|----------|----------|-------|
| `quickstart` | Direct persist | `mem` | Minimal boot + `tracing_subscriber` |
| `quickstart_sqlite` | Direct persist | `sqlite` | Durable embedded wiring |
| `quickstart_transport` | Dual-path | `mem` | Sink + persist in one process |
| `quickstart_telemetry` | Direct persist | `mem,telemetry-console` | NDJSON under a temp dir |
| `quickstart_tensorbase_emit` | Direct persist (remote) | `tensorbase` | Needs `SPECTRA_TENSORBASE_URL` |

## Status

Shipped in tag `v0.1.0`.
