# spectra-runtime

Runtime assembly: builder, composite sink, and process install.

## Audience

| Reader | Use this crate for |
|--------|-------------------|
| **Host integrators** | `SpectraBuilder`, `.persist()`, `.sink()`, `.embedded()`, `.telemetry_ndjson()` |
| **Library maintainers** | Wiring storage backends into a running `Spectra` instance |

## Role

- `SpectraBuilder` — inject metrics/events backends, optional transport sink, telemetry
- `PersistConfig` via `.persist(...)` — L2 queue depth and batch size (no env knobs)
- `Spectra::flush_persist()` — durable barrier after `*_now` emits
- `StoragePersistSink` — async direct storage persist (default); supports inner transport sink
- Process-scoped install helpers

## Builder composition

| Mode | Calls | Role |
|------|-------|------|
| Direct persist (default) | `.metrics_backend(..).events_backend(..).build()` | Emit process writes storage |
| High-throughput batch | `.persist(PersistConfig { batch_max: 2048, ..Default::default() })` | Larger L2 batches to DW |
| Transport + persist | `.sink(transport).build()` | Dual-path mirror + persist |
| Publish only | `.sink(transport).persist_disabled().build()` | Publisher; consumers own storage |

Canonical emit path for web and scripts: `try_record_*_now` / generated helpers → L2 batch worker.
Use `flush_persist().await` when a script must wait for durable writes before exit.

See the `spectra` crate rustdoc **Getting started → Mode 2** for the publisher/consumer split.

## Status

Shipped in tag `v0.1.0`.
