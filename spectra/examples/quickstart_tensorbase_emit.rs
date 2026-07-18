//! TensorBase remote storage: connect, emit smoke counter + event, query roundtrip.
//!
//! ```bash
//! export SPECTRA_TENSORBASE_URL=tcp://127.0.0.1:9528
//! cargo run -p uf-spectra --example quickstart_tensorbase_emit --features tensorbase
//! ```

use std::sync::Arc;

use spectra::helpers::{PlatformSmokeCounterRecorder, PlatformSmokeEventLogger};
use spectra::{Spectra, TensorBaseEventsBackend, TensorBaseMetricsBackend};
use spectra_core::{
    current_emit_ts, EventsQueryFilter, MetricsQueryRange, SharedEventBackend,
    SharedMetricsBackend,
};

#[tokio::main]
async fn main() -> spectra::Result<()> {
    let url = std::env::var("SPECTRA_TENSORBASE_URL").unwrap_or_else(|_| {
        eprintln!("Set SPECTRA_TENSORBASE_URL (e.g. tcp://127.0.0.1:9528)");
        std::process::exit(1);
    });

    let metrics: SharedMetricsBackend =
        Arc::new(TensorBaseMetricsBackend::connect(&url).await?);
    let events: SharedEventBackend = Arc::new(TensorBaseEventsBackend::connect(&url).await?);

    let spectra = Spectra::builder()
        .metrics_backend(Arc::clone(&metrics))
        .events_backend(Arc::clone(&events))
        .build()?;

    PlatformSmokeCounterRecorder::record(1, serde_json::json!({}));
    PlatformSmokeEventLogger::log("tensorbase remote emit".to_string());

    tokio::time::sleep(std::time::Duration::from_millis(80)).await;

    let now = current_emit_ts();
    let points = spectra
        .router()
        .query_metrics(MetricsQueryRange {
            metric_name: "platform_smoke_counter".into(),
            start: now - chrono::Duration::seconds(30),
            end: now + chrono::Duration::seconds(5),
            label_matchers: vec![],
        })
        .await?;

    let event_rows = spectra
        .router()
        .query_events(EventsQueryFilter {
            table: "platform_smoke_event".into(),
            start: Some(now - chrono::Duration::seconds(30)),
            end: Some(now + chrono::Duration::seconds(5)),
            ..Default::default()
        })
        .await?;

    println!(
        "tensorbase emit OK: {} metric point(s), {} event row(s)",
        points.len(),
        event_rows.len()
    );

    if points.is_empty() || event_rows.is_empty() {
        std::process::exit(1);
    }
    Ok(())
}
