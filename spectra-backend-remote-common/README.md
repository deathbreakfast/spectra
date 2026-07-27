# spectra-backend-remote-common

Shared remote storage logic for ClickHouse-protocol Spectra backends. HTTP/native client wiring shared by the `clickhouse` and `tensorbase` backends — **not** a public application dependency; do not depend on this crate directly from apps.

## Role

- `RemoteClient`, `RemoteMetricsBackend`, `RemoteEventsBackend`
- Used by `spectra-backend-clickhouse` and `spectra-backend-tensorbase`
- **Not** re-exported from the public `spectra` crate — enable `clickhouse` or `tensorbase` features instead

## Status

Shipped in tag `v0.1.0`.
