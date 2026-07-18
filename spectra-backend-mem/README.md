# spectra-backend-mem

In-memory metrics and events backend.

## Audience

| Reader | Use this crate for |
|--------|-------------------|
| **App developers** | Quick start via the `spectra` feature `mem` (default) |
| **Test authors** | Fast, non-durable storage in unit tests |

## Role

- `MemMetricsBackend` and `MemEventsBackend`
- Facade default backend (`default = ["mem"]`)

## Status

Shipped in tag `v0.1.0` (default `spectra` backend).
