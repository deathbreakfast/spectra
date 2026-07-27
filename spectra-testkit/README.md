# spectra-testkit

Shared matrix bootstrap, scenarios, and fixtures for Spectra verification. Use `MatrixSpec`, `ScenarioSpec`, and `ScenarioRunner` for matrix tests and the same timed bench scenarios as `spectra-e2e`.

## Role

- Matrix dimensions: storage, transport, telemetry, topology
- `BootstrapSession` installs `Spectra::builder()` for one matrix row
- Shared by `spectra-e2e` and `spectra-bench`

## Status

Shipped in tag `v0.1.0`. CI preset: `ci_embedded_rows()` (mem/sqlite × direct/recording).
