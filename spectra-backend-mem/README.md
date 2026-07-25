# spectra-backend-mem

In-memory metrics and events backend.

## Audience

| Reader | Use this crate for |
|--------|-------------------|
| **App developers** | Quick start via the `spectra` feature `mem` (default) |
| **Test authors** | Fast, non-durable storage in unit tests |

## Role

- `MemMetricsBackend` and `MemEventsBackend`
- Public crate default backend (`default = ["mem"]`)

## Runnable

```bash
cargo run -p uf-spectra --example quickstart_schema_emit --features mem
```

See [`spectra/README.md` — How to run examples](../spectra/README.md#how-to-run-examples).

## Status

Shipped in tag `v0.1.0` (default `spectra` backend).
