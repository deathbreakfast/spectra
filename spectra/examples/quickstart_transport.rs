//! Transport + storage **dual-path**: sink receives emits *and* storage persists.
//!
//! Matches **Getting started → Mode 3** in the `spectra` crate docs. Contrast with
//! `quickstart_publish_only` (Mode 2 publisher) and `quickstart_consume_forward` (Mode 2
//! consumer).
//!
//! ```bash
//! cargo run -p uf-spectra --example quickstart_transport --features mem
//! ```

use std::sync::Arc;

use spectra::{MemEventsBackend, MemMetricsBackend, RecordingSink, Spectra, SpectraSink};
use spectra_core::{try_record_counter_now, SharedEventBackend, SharedMetricsBackend};

#[tokio::main]
async fn main() -> spectra::Result<()> {
    let metrics: SharedMetricsBackend = Arc::new(MemMetricsBackend::new());
    let events: SharedEventBackend = Arc::new(MemEventsBackend::new());
    let transport = Arc::new(RecordingSink::new());

    let spectra = Spectra::builder()
        .metrics_backend(Arc::clone(&metrics))
        .events_backend(Arc::clone(&events))
        .sink(Arc::clone(&transport) as Arc<dyn SpectraSink>)
        .embedded()
        .build()?;

    try_record_counter_now("cache_hits", &[("region", "us-west")], 1);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    assert_eq!(
        transport.counters().len(),
        1,
        "transport sink should receive emit"
    );

    let now = spectra_core::current_emit_ts();
    let points = spectra
        .router()
        .query_metrics(spectra_core::MetricsQueryRange {
            metric_name: "cache_hits".into(),
            start: now - chrono::Duration::seconds(5),
            end: now + chrono::Duration::seconds(1),
            label_matchers: vec![],
        })
        .await?;
    assert_eq!(points.len(), 1, "storage backend should persist emit");

    eprintln!(
        "transport + persist OK: {} counter(s) in transport, {} point(s) in storage",
        transport.counters().len(),
        points.len()
    );
    Ok(())
}
