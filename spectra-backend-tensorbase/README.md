# spectra-backend-tensorbase

Scale-out TensorBase storage adapter (ClickHouse-compatible native protocol).

## Audience

| Reader | Use this crate for |
|--------|-------------------|
| **Host integrators** | Remote ingest and multi-node metric/event persistence |
| **Adapter authors** | TensorBase wire protocol boundary |

## Role

- `TensorBaseMetricsBackend` and `TensorBaseEventsBackend`
- Enabled via the `spectra` feature `tensorbase`
- Uses the official `clickhouse` Rust client against TensorBase's ClickHouse-compatible TCP/HTTP endpoint
- DDL uses `ENGINE = BaseStorage` (TensorBase dialect)

## Connect

```rust
use std::sync::Arc;
use spectra::{Spectra, TensorBaseEventsBackend, TensorBaseMetricsBackend};

// Native protocol default port 9528:
let metrics = TensorBaseMetricsBackend::connect_host("127.0.0.1").await?;
let events = TensorBaseEventsBackend::connect_host("127.0.0.1").await?;
let _spectra = Spectra::builder()
    .metrics_backend(Arc::new(metrics))
    .events_backend(Arc::new(events))
    .build()?;

// Or explicit URL (tcp://host:9528 or http://host:8123):
let url = std::env::var("SPECTRA_TENSORBASE_URL")?;
let metrics = TensorBaseMetricsBackend::connect(&url).await?;
let events = TensorBaseEventsBackend::connect(&url).await?;
let _spectra = Spectra::builder()
    .metrics_backend(Arc::new(metrics))
    .events_backend(Arc::new(events))
    .build()?;
```

**Note:** [TensorBase upstream](https://github.com/tensorbase/tensorbase) is maintenance-only; the trait boundary allows swapping engines without changing emit code.

Integration tests: set `SPECTRA_TENSORBASE_URL` and run `cargo test -p spectra-backend-tensorbase -- --ignored`.

## Status

Shipped in tag `v0.1.0`. `query_aggregate` is not yet implemented (returns empty series).
