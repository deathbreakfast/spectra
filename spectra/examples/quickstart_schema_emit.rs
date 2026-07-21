//! Emit via CI demo generated helpers (`platform_smoke_*` schemas).
//!
//! ```bash
//! cargo run -p uf-spectra --example quickstart_schema_emit --features mem
//! ```

use std::sync::Arc;

use spectra::helpers::{PlatformSmokeCounterRecorder, PlatformSmokeEventLogger};
use spectra::{MemEventsBackend, MemMetricsBackend, Spectra};
use spectra_core::{
    current_emit_ts, EventStorageBackend, MetricsQueryRange, MetricsStorageBackend,
};

#[tokio::main]
async fn main() -> spectra::Result<()> {
    let metrics: Arc<dyn MetricsStorageBackend> = Arc::new(MemMetricsBackend::new());
    let events: Arc<dyn EventStorageBackend> = Arc::new(MemEventsBackend::new());

    let spectra = Spectra::builder()
        .metrics_backend(Arc::clone(&metrics))
        .events_backend(Arc::clone(&events))
        .embedded()
        .build()?;

    PlatformSmokeCounterRecorder::record(1, serde_json::json!({}));
    PlatformSmokeEventLogger::log("quickstart schema emit".to_string());

    tokio::time::sleep(std::time::Duration::from_millis(80)).await;

    let now = current_emit_ts();
    let points = spectra
        .router()
        .query_metrics(MetricsQueryRange {
            metric_name: "platform_smoke_counter".into(),
            start: now - chrono::Duration::seconds(5),
            end: now + chrono::Duration::seconds(1),
            label_matchers: vec![],
        })
        .await?;

    eprintln!("schema emit OK: {} metric point(s) persisted", points.len());
    Ok(())
}
