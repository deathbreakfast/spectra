//! Shared storage port contract for the tensorbase in-memory stub.

use std::sync::Arc;

use spectra_backend_tensorbase::{TensorBaseEventsBackend, TensorBaseMetricsBackend};
use spectra_testkit::run_storage_contract;

#[tokio::test]
async fn tensorbase_storage_contract_stub() {
    let metrics = Arc::new(TensorBaseMetricsBackend::in_memory_stub());
    let events = Arc::new(TensorBaseEventsBackend::in_memory_stub());
    run_storage_contract(metrics, events)
        .await
        .expect("tensorbase storage contract");
}

#[tokio::test]
#[ignore = "requires SPECTRA_TENSORBASE_URL"]
async fn tensorbase_storage_contract_live() {
    if std::env::var("SPECTRA_TENSORBASE_URL").is_err() {
        return;
    }
    let url = std::env::var("SPECTRA_TENSORBASE_URL").expect("SPECTRA_TENSORBASE_URL");
    let metrics = Arc::new(
        TensorBaseMetricsBackend::connect(&url)
            .await
            .expect("tensorbase metrics connect"),
    );
    let events = Arc::new(
        TensorBaseEventsBackend::connect(&url)
            .await
            .expect("tensorbase events connect"),
    );
    run_storage_contract(metrics, events)
        .await
        .expect("tensorbase live storage contract");
}
