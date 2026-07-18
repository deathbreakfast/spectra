use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use chrono::{Duration as ChronoDuration, Utc};
use serde_json::json;
use spectra_core::{EventStorageBackend, MetricsStorageBackend};

pub const METRIC_PREFILL_NAME: &str = "bench.query.metric";
pub const EVENT_PREFILL_TABLE: &str = "bench_query_event";

#[derive(Debug, Clone)]
pub struct PrefillResult {
    pub count: u64,
    pub elapsed_ms: f64,
}

pub async fn prefill_metrics(
    metrics: Arc<dyn MetricsStorageBackend>,
    count: u64,
) -> Result<PrefillResult> {
    let started = Instant::now();
    let base_ts = Utc::now() - ChronoDuration::hours(1);
    for i in 0..count {
        let labels = json!({ "idx": i.to_string(), "region": if i % 2 == 0 { "us-west" } else { "eu-central" } });
        let ts = base_ts + ChronoDuration::milliseconds(i as i64);
        metrics
            .record_counter(METRIC_PREFILL_NAME, &labels, 1, ts)
            .await?;
    }
    Ok(PrefillResult {
        count,
        elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
    })
}

pub async fn prefill_events(
    events: Arc<dyn EventStorageBackend>,
    count: u64,
) -> Result<PrefillResult> {
    let started = Instant::now();
    let base_ts = Utc::now() - ChronoDuration::hours(1);
    for i in 0..count {
        let fields = json!({
            "idx": i,
            "region": if i % 2 == 0 { "us-west" } else { "eu-central" },
            "payload": format!("prefill row {i} padded for bench query depth study")
        });
        let ts = base_ts + ChronoDuration::milliseconds(i as i64);
        events
            .append_row(EVENT_PREFILL_TABLE, &fields, ts, None)
            .await?;
    }
    Ok(PrefillResult {
        count,
        elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
    })
}
