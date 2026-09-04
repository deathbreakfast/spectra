//! Shared storage port contract for the mem backend.

use std::sync::Arc;

use spectra_backend_mem::{MemEventsBackend, MemMetricsBackend};
use spectra_testkit::run_storage_contract;

#[tokio::test]
async fn mem_storage_contract() {
    let metrics = Arc::new(MemMetricsBackend::new());
    let events = Arc::new(MemEventsBackend::new());
    run_storage_contract(metrics, events)
        .await
        .expect("mem storage contract");
}
