//! Remote ClickHouse storage adapter for Spectra metrics and events.
//!
//! Enable with the `spectra` feature `clickhouse` and wire through `Spectra::builder()`.
//!
//! - [`ClickHouseMetricsBackend::connect`] / [`ClickHouseEventsBackend::connect`]
//! - [`crate::ddl::metrics_ddl`] / [`crate::ddl::events_ddl`] — canonical MergeTree DDL
//! - Set `SPECTRA_CLICKHOUSE_URL` for integration tests (`cargo test -- --ignored`).
//! - JSON labels and event fields are stored as `String` columns.
//! - `query_aggregate` is not yet implemented (returns empty series).

mod ddl;
mod events;
mod metrics;

pub use events::ClickHouseEventsBackend;
pub use metrics::ClickHouseMetricsBackend;
