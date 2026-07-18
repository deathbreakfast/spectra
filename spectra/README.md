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

| Mode | Calls | Role |
|------|-------|------|
| Direct persist (default) | `.metrics_backend(..).events_backend(..).build()` | Emit process writes storage |
| Transport + persist | `.sink(transport).build()` | Dual-path: bus mirror + local persist |
| Publish only (distributed) | `.sink(transport).persist_disabled().build()` | **Publisher** — consumers write storage |

Publisher/consumer setup: `cargo doc -p uf-spectra --open` → **Getting started → Mode 2**, then
`SpectraSink`, `topics`, and examples `quickstart_publish_only` /
`quickstart_consume_forward`.

### Emit gate and sampling

Loaded by `SpectraConfig::from_env()` unless `.config(...)` overrides.

| Variable | Default | Effect |
|----------|---------|--------|
| `SPECTRA_GATE` | on | Set `0`/`false`/`no` to disable the emit gate (fail-open) |
| `SPECTRA_LEVEL` | `info` | Global minimum verbosity (`error` … `trace`) |
| `SPECTRA_SAMPLE_RATE` | `1.0` | Global sample floor after level check |
| `SPECTRA_SAMPLE_<NAME>` | — | Per metric/event name override (`0.0`–`1.0`) |
| `SPECTRA_CONFIG` | — | Path to TOML file with a `[spectra]` table |

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

## Examples

```bash
export CARGO_BUILD_JOBS=1 CARGO_TARGET_DIR=target-spectra-extract
cargo run -p uf-spectra --example quickstart --features mem
cargo run -p uf-spectra --example quickstart_transport --features mem
cargo run -p uf-spectra --example quickstart_publish_only --features mem
cargo run -p uf-spectra --example quickstart_consume_forward --features mem
cargo run -p uf-spectra --example quickstart_sqlite --features sqlite
cargo run -p uf-spectra --example quickstart_schema_emit --features mem
cargo run -p uf-spectra --example quickstart_telemetry --features mem,telemetry-console
# Remote (requires live ClickHouse):
SPECTRA_REMOTE_URL=http://localhost:8123 cargo run -p uf-spectra --example quickstart_remote --features clickhouse
```

## Status

Shipped in tag `v0.1.0`.
