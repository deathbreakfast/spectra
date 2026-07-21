//! Consumer-side sketch: decode a [`MetricEmit`] and persist via [`try_record_counter_at`].
//!
//! Matches **Getting started → Mode 2 → Consumer binary** in the `spectra` crate docs.
//! In production, receive the envelope from your bus (for example a Photon subscriber), then
//! call `try_record_counter_at` / `try_log_event_at` with the envelope timestamp (or a storage
//! backend). This example builds the envelope in-process so the roundtrip is runnable without
//! a broker.
//!
//! See also [`spectra::topics`].
//!
//! ```bash
//! cargo run -p uf-spectra --example quickstart_consume_forward --features mem
//! ```

use std::sync::Arc;

use chrono::{TimeZone, Utc};
use spectra::topics::PlatformSmokeCounterPayload;
use spectra::{try_record_counter_at, MemEventsBackend, MemMetricsBackend, MetricEmit, Spectra};
use spectra_core::{SharedEventBackend, SharedMetricsBackend};

#[tokio::main]
async fn main() -> spectra::Result<()> {
    let metrics: SharedMetricsBackend = Arc::new(MemMetricsBackend::new());
    let events: SharedEventBackend = Arc::new(MemEventsBackend::new());

    // Consumer owns storage (direct persist).
    let spectra = Spectra::builder()
        .metrics_backend(Arc::clone(&metrics))
        .events_backend(Arc::clone(&events))
        .embedded()
        .build()?;

    // Publisher would build this payload and put it on the bus (with emit-time ts):
    let emit_ts = Utc.with_ymd_and_hms(2024, 6, 15, 12, 0, 0).unwrap();
    let payload = PlatformSmokeCounterPayload {
        name: "platform_smoke_counter",
        labels: serde_json::json!({"region": "us-west"}),
        delta: 1,
        ts: Some(emit_ts),
    };
    let emit: MetricEmit = payload.to_metric_emit();

    // Consumer: after subscribe/decode, persist via the timestamp-aware emit API.
    let delta = emit.delta.unwrap_or(1);
    let label_pairs: Vec<(&str, &str)> = emit
        .labels
        .as_object()
        .map(|obj| {
            obj.iter()
                .map(|(k, v)| (k.as_str(), v.as_str().unwrap_or("")))
                .collect()
        })
        .unwrap_or_default();
    let ts = emit.ts.unwrap_or_else(Utc::now);
    try_record_counter_at(&emit.name, &label_pairs, delta, ts);

    tokio::time::sleep(std::time::Duration::from_millis(80)).await;

    let points = spectra
        .router()
        .query_metrics(spectra_core::MetricsQueryRange {
            metric_name: "platform_smoke_counter".into(),
            start: emit_ts - chrono::Duration::seconds(30),
            end: emit_ts + chrono::Duration::seconds(5),
            label_matchers: vec![],
        })
        .await?;

    eprintln!(
        "consume-forward OK: topic={}, {} metric point(s) in storage (ts preserved)",
        PlatformSmokeCounterPayload::topic(),
        points.len()
    );
    if points.is_empty() {
        std::process::exit(1);
    }
    if points[0].ts != emit_ts {
        eprintln!(
            "timestamp mismatch: stored {:?} expected {:?}",
            points[0].ts, emit_ts
        );
        std::process::exit(1);
    }
    Ok(())
}
