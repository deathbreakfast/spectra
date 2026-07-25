# spectra-backend-sqlite

Durable embedded SQLite backend for Spectra metrics and events.

## Audience

| Reader | Use this crate for |
|--------|-------------------|
| **Test authors** | Durable embedded storage in `spectra-e2e` and examples |
| **Host integrators** | File-backed embedded store on a single host |

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
