//! Dev telemetry via NDJSON files and optional console mirror.
//!
//! ```bash
//! cargo run -p uf-spectra --example quickstart_telemetry --features mem,telemetry-console
//! ```

use std::sync::Arc;

use spectra::{MemEventsBackend, MemMetricsBackend, Spectra};
use spectra_core::try_record_counter_now;

#[tokio::main]
async fn main() -> spectra::Result<()> {
    let dir = std::env::temp_dir().join("spectra-quickstart-telemetry");
    std::fs::create_dir_all(&dir)?;

    let _spectra = Spectra::builder()
        .metrics_backend(Arc::new(MemMetricsBackend::new()))
        .events_backend(Arc::new(MemEventsBackend::new()))
        .telemetry_ndjson(&dir)?
        .embedded()
        .build()?;

    try_record_counter_now("telemetry_demo", &[], 1);
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    println!(
        "telemetry NDJSON written under {} (metrics.ndjson, events.ndjson)",
        dir.display()
    );
    Ok(())
}
