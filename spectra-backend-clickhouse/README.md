# spectra-backend-clickhouse

Remote ClickHouse storage adapter for Spectra metrics and events.

## Audience

| Reader | Use this crate for |
|--------|-------------------|
| **Host integrators** | Network-backed analytics store for metrics and events |
| **Adapter authors** | ClickHouse client boundary |

## Role

- `ClickHouseMetricsBackend` and `ClickHouseEventsBackend`
- Enabled via the `spectra` feature `clickhouse`
- Canonical tables: `spectra_metrics`, `spectra_events` (JSON labels/fields as String columns)

## Connect

```rust
use std::sync::Arc;
use spectra::{ClickHouseEventsBackend, ClickHouseMetricsBackend, Spectra};

let url = std::env::var("SPECTRA_CLICKHOUSE_URL")?; // e.g. http://localhost:8123
let metrics = ClickHouseMetricsBackend::connect(&url).await?;
let events = ClickHouseEventsBackend::connect(&url).await?;
let _spectra = Spectra::builder()
    .metrics_backend(Arc::new(metrics))
    .events_backend(Arc::new(events))
    .build()?;
```

Integration tests: set `SPECTRA_CLICKHOUSE_URL` and run `cargo test -p spectra-backend-clickhouse -- --ignored`.

## Status

Shipped in tag `v0.1.0`. `query_aggregate` is not yet implemented (returns empty series).
