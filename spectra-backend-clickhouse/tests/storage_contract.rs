//! Shared storage port contract for the clickhouse in-memory stub.

use std::sync::Arc;

use spectra_backend_clickhouse::{ClickHouseEventsBackend, ClickHouseMetricsBackend};
use spectra_testkit::run_storage_contract;

#[tokio::test]
async fn clickhouse_storage_contract_stub() {
    let metrics = Arc::new(ClickHouseMetricsBackend::in_memory_stub());
    let events = Arc::new(ClickHouseEventsBackend::in_memory_stub());
    run_storage_contract(metrics, events)
        .await
        .expect("clickhouse storage contract");
}

#[tokio::test]
#[ignore = "requires SPECTRA_CLICKHOUSE_URL"]
async fn clickhouse_storage_contract_live() {
    if std::env::var("SPECTRA_CLICKHOUSE_URL").is_err() {
        return;
    }
    let url = std::env::var("SPECTRA_CLICKHOUSE_URL").expect("SPECTRA_CLICKHOUSE_URL");
    let metrics = Arc::new(
        ClickHouseMetricsBackend::connect(&url)
            .await
            .expect("clickhouse metrics connect"),
    );
    let events = Arc::new(
        ClickHouseEventsBackend::connect(&url)
            .await
            .expect("clickhouse events connect"),
    );
    run_storage_contract(metrics, events)
        .await
        .expect("clickhouse live storage contract");
}
