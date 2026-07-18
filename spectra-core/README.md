# spectra-core (`uf-spectra-core`)

Storage traits, emit ports, router, registry, emit buffer, and query DTOs.

## Audience

| Reader | Use this crate for |
|--------|-------------------|
| **App developers** | Emit API surface re-exported by the `spectra` crate |
| **Adapter authors** | `MetricsStorageBackend`, `EventStorageBackend`, `SpectraRouter` |
| **Host integrators** | Wiring backends into `SpectraBuilder` |

## Role

- `MetricsStorageBackend` / `EventStorageBackend` async ports
- `SpectraSink`, dispatcher, emit buffer, classification metadata
- `SchemaRegistry`, topic naming, query DTOs
- No storage engine SDKs — those live in `spectra-backend-*` crates

## Status

Shipped in tag `v0.1.0`. See crate rustdoc and root [README.md](../README.md).
