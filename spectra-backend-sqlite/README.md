# spectra-backend-sqlite

Durable embedded SQLite backend for Spectra metrics and events. Use for file-backed embedded storage on a single host and in durable e2e/examples within this repository.

## Role

- `SqliteMetricsBackend` and `SqliteEventsBackend`
- Primary durable **embedded** backend inside this repository's testkit and e2e

## Runnable

```bash
cargo run -p uf-spectra --example quickstart_sqlite --features sqlite
```

See [`spectra/README.md` — How to run examples](../spectra/README.md#how-to-run-examples).

## Status

Shipped in tag `v0.1.0` (durable embedded storage).
