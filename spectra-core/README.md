# spectra-core (`uf-spectra-core`)

Storage traits, emit ports, router, registry, emit buffer, and query DTOs. Depend on the emit APIs re-exported by the `spectra` crate rather than this crate directly for application code. Implement `MetricsStorageBackend`, `EventStorageBackend`, and `SpectraRouter` for storage adapters; wire backends into `SpectraBuilder` at host boot.

## Role

- `MetricsStorageBackend` / `EventStorageBackend` async ports
- `SpectraSink`, dispatcher, emit buffer, classification metadata
- `SchemaRegistry`, topic naming, query DTOs
- No storage engine SDKs — those live in `spectra-backend-*` crates

## Status

Shipped in tag `v0.1.0`. See crate rustdoc and root [README.md](../README.md).
