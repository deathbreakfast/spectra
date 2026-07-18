//! Shared storage port contract for the sqlite backend.

use std::sync::Arc;

use spectra_backend_sqlite::{SqliteEventsBackend, SqliteMetricsBackend};
use spectra_testkit::run_storage_contract;
use tempfile::TempDir;

#[tokio::test]
async fn sqlite_storage_contract() {
    let dir = TempDir::new().expect("tempdir");
    let metrics_path = dir.path().join("metrics.db");
    let events_path = dir.path().join("events.db");
    let metrics = Arc::new(
        SqliteMetricsBackend::new(&metrics_path).expect("sqlite metrics"),
    );
    let events = Arc::new(SqliteEventsBackend::new(&events_path).expect("sqlite events"));
    run_storage_contract(metrics, events)
        .await
        .expect("sqlite storage contract");
}
