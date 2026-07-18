# spectra-backend-remote-common

Shared remote storage logic for ClickHouse-protocol Spectra backends.

## Audience

| Reader | Use this crate for |
|--------|-------------------|
| **Adapter authors** | HTTP/native client wiring shared by `clickhouse` and `tensorbase` backends |
| **Library maintainers** | Internal insert/query paths — not a public application dependency |

## Role

- `RemoteClient`, `RemoteMetricsBackend`, `RemoteEventsBackend`
- Used by `spectra-backend-clickhouse` and `spectra-backend-tensorbase`
- **Not** re-exported from the public `spectra` facade — enable `clickhouse` or `tensorbase` features instead

## Status

Shipped in tag `v0.1.0`.
