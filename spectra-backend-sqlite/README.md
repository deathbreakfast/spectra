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

## Status

Shipped in tag `v0.1.0` (durable embedded storage).
