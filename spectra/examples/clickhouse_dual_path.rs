//! Dual-path against live ClickHouse: transport sink **and** remote persist in one process.
//!
//! Combines `quickstart_transport` (RecordingSink) with `quickstart_clickhouse_emit`. After emit,
//! both the in-process sink and ClickHouse storage receive the smoke counter/event. Demonstrates
//! flush semantics via a short settle sleep before query.
//!
//! ```bash
//! docker compose -f docker-compose.dev.yml up -d clickhouse
//! export SPECTRA_ALLOW_INSECURE_REMOTE=1
//! export SPECTRA_CLICKHOUSE_URL=http://127.0.0.1:8123
//! cargo run -p uf-spectra --example clickhouse_dual_path --features clickhouse
//! ```

#![allow(clippy::print_stderr)]

use std::sync::Arc;

use spectra::helpers::{PlatformSmokeCounterRecorder, PlatformSmokeEventLogger};
use spectra::{
    ClickHouseEventsBackend, ClickHouseMetricsBackend, RecordingSink, Spectra, SpectraSink,
};
use spectra_core::{
    current_emit_ts, EventsQueryFilter, MetricsQueryRange, SharedEventBackend, SharedMetricsBackend,
};

#[tokio::main]
async fn main() -> spectra::Result<()> {
    let url = std::env::var("SPECTRA_CLICKHOUSE_URL").unwrap_or_else(|_| {
        eprintln!(
            "Set SPECTRA_CLICKHOUSE_URL (e.g. http://127.0.0.1:8123 with \
             SPECTRA_ALLOW_INSECURE_REMOTE=1)"
        );
        std::process::exit(1);
    });

    let metrics: SharedMetricsBackend = Arc::new(ClickHouseMetricsBackend::connect(&url).await?);
    let events: SharedEventBackend = Arc::new(ClickHouseEventsBackend::connect(&url).await?);
    let transport = Arc::new(RecordingSink::new());

    let spectra = Spectra::builder()
        .metrics_backend(Arc::clone(&metrics))
        .events_backend(Arc::clone(&events))
        .sink(Arc::clone(&transport) as Arc<dyn SpectraSink>)
        .embedded()
        .build()?;

    PlatformSmokeCounterRecorder::record(1, serde_json::json!({}));
    PlatformSmokeEventLogger::log("clickhouse dual-path emit".to_string());

    tokio::time::sleep(std::time::Duration::from_millis(120)).await;
    let _ = spectra.flush_persist().await;

    assert!(
        !transport.counters().is_empty() || !transport.events().is_empty(),
        "dual-path sink should receive at least one emit"
    );

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

    eprintln!(
        "clickhouse dual-path OK: {} transport counter(s), {} transport event(s), \
         {} metric point(s), {} event row(s)",
        transport.counters().len(),
        transport.events().len(),
        points.len(),
        event_rows.len()
    );

    if points.is_empty() || event_rows.is_empty() {
        std::process::exit(1);
    }
    Ok(())
}
