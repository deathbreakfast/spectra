//! Publisher-only wiring: transport sink receives emits; storage persist is disabled.
//!
//! Matches **Getting started → Mode 2 → Publisher binary** in the `spectra` crate docs.
//! Replace [`RecordingSink`] with a host `SpectraSink` that publishes schema `*Payload`
//! / `*_TOPIC` values onto your bus (for example Photon). Pair with a separate consumer binary
//! (`quickstart_consume_forward` sketches the other half).
//!
//! ```bash
//! cargo run -p uf-spectra --example quickstart_publish_only --features mem
//! ```

use std::sync::Arc;

use spectra::{MemEventsBackend, MemMetricsBackend, RecordingSink, Spectra, SpectraSink};
use spectra_core::{try_record_counter_now, SharedEventBackend, SharedMetricsBackend};

#[tokio::main]
async fn main() -> spectra::Result<()> {
    // Backends are still required by the builder, but persist_disabled means they receive nothing.
    let metrics: SharedMetricsBackend = Arc::new(MemMetricsBackend::new());
    let events: SharedEventBackend = Arc::new(MemEventsBackend::new());
    let transport = Arc::new(RecordingSink::new());

    let spectra = Spectra::builder()
        .metrics_backend(Arc::clone(&metrics))
        .events_backend(Arc::clone(&events))
        .sink(Arc::clone(&transport) as Arc<dyn SpectraSink>)
        .persist_disabled()
        .build()?;

    try_record_counter_now("cache_hits", &[("region", "us-west")], 1);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    assert_eq!(
        transport.counters().len(),
        1,
        "publisher sink should receive the emit"
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
    assert!(
        points.is_empty(),
        "publisher process must not persist when persist_disabled"
    );

    eprintln!(
        "publish-only OK: {} counter(s) on transport, {} storage point(s)",
        transport.counters().len(),
        points.len()
    );
    Ok(())
}
