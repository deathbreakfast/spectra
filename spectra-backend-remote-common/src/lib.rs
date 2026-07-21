//! Shared remote storage logic for ClickHouse-protocol Spectra backends.
//!
//! **Internal** — used by `spectra-backend-clickhouse` and `spectra-backend-tensorbase`;
//! not re-exported from the public `spectra` facade.
//!
//! # Stack position
//!
//! Internal library between `spectra_core` storage traits and engine-specific adapter crates.
//! Provides HTTP/native client wiring, DDL execution, and shared insert/query paths.
//!
//! # Entry points
//!
//! - [`RemoteClient::connect`] — ClickHouse-protocol client wrapper
//! - [`RemoteMetricsBackend::connect`] / [`RemoteEventsBackend::connect`] — parameterized backends
//! - [`MetricInsertRow`] / [`EventInsertRow`] — row shapes for streaming inserts
//!
//! # Prerequisites and gotchas
//!
//! - Expects canonical table names from `spectra_core` (`spectra_metrics`, `spectra_events`).
//! - `query_aggregate` is not yet implemented (returns empty series).
//! - Label filtering on metric range queries happens client-side after fetch.
//!
//! See also: the `spectra` facade crate documentation map (`cargo doc -p uf-spectra --open`).

mod client;
mod events;
mod mem_store;
mod metrics;
mod query_sql;

pub use client::{
    datetime_to_ch_ts, parse_rfc3339_ts, EventInsertRow, MetricInsertRow, RemoteClient,
};
pub use events::RemoteEventsBackend;
pub use metrics::RemoteMetricsBackend;
