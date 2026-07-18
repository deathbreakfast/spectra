//! Shared matrix bootstrap, scenarios, and fixtures for Spectra verification.
//!
//! **Internal** — consumed by `spectra-e2e` and `spectra-bench`; not part of the public host
//! integration surface.
//!
//! Installs `spectra::Spectra::builder()` for one [MatrixSpec](crate::MatrixSpec) row and executes
//! declarative [ScenarioSpec](crate::ScenarioSpec) steps.
//!
//! - [MatrixSpec](crate::MatrixSpec) — storage × transport × telemetry × topology selector
//! - [BootstrapSession::install_async](crate::BootstrapSession::install_async) — install backends and global sink
//! - [ScenarioRunner::run](crate::ScenarioRunner::run) — execute a scenario under a process-wide test lock
//! - Remote rows need env URLs; gate with [remote_env_ready](crate::remote_env_ready).
//! - Mem/sqlite require [Topology::Embedded](crate::Topology::Embedded); remote adapters require
//!   [Topology::RemoteIngest](crate::Topology::RemoteIngest).

mod bootstrap;
mod catalog;
mod fixtures;
mod matrix;
mod runner;
mod scenario;
mod shard;
mod storage_contract;

use anyhow::Result;

pub use bootstrap::{BootstrapSession, InstalledSpectra};
pub use catalog::{
    catalog_entries, remote_catalog_entries, run_catalog_scenario, run_remote_catalog_scenario,
    CatalogEntry, PathKind,
};
pub use fixtures::{assert_embedded_topology, remote_env_ready, validate_matrix_env};
pub use shard::{
    client_index, clickhouse_url_sharded, dw_n, dw_url_fingerprint, remote_url_for, shard_index,
    tensorbase_url_sharded,
};
pub use matrix::{
    ci_embedded_rows, ci_recording_rows, ci_telemetry_rows, remote_ingest_rows, MatrixSpec,
    StorageAdapter, TelemetryAdapter, Topology, TransportAdapter,
};
pub use runner::{DriverKind, ScenarioResult, ScenarioRunner, StepTiming};
pub use scenario::{ScenarioSpec, ScenarioStep, DEFAULT_VISIBILITY_TIMEOUT_MS};
pub use storage_contract::run_storage_contract;

/// Install one matrix row for bench workloads (holds process-wide test lock).
pub async fn install_bench_matrix(matrix: MatrixSpec, slug_suffix: &str) -> Result<InstalledSpectra> {
    install_bench_matrix_with_persist(matrix, slug_suffix, None).await
}

/// Like [`install_bench_matrix`] with optional L2 [`spectra::PersistConfig`].
pub async fn install_bench_matrix_with_persist(
    matrix: MatrixSpec,
    slug_suffix: &str,
    persist: Option<spectra::PersistConfig>,
) -> Result<InstalledSpectra> {
    let _lock = bootstrap::MATRIX_TEST_LOCK.lock().await;
    let mut session = BootstrapSession::new(matrix).with_slug_suffix(slug_suffix);
    if let Some(config) = persist {
        session = session.with_persist_config(config);
    }
    session.install_async().await
}
