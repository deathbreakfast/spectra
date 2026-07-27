//! Custom metrics/events backend stub — implement the storage traits and inject them.
//!
//! Shows the minimum surface hosts need when bringing their own warehouse. The stub records
//! writes in process memory (like mem) and returns `StorageEngineType::Mem` so the builder
//! accepts it. Replace the bodies with your adapter.
//!
//! ```bash
//! cargo run -p uf-spectra --example custom_backend_stub --features mem
//! ```

#![allow(clippy::print_stderr)]

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use serde_json::Value;
use spectra::{try_record_counter_now, Spectra};
use spectra_core::{
    EventRow, EventStorageBackend, EventsQueryFilter, MetricPoint, MetricsQueryRange,
    MetricsStorageBackend, Result, SharedEventBackend, SharedMetricsBackend, StorageEngineType,
};

struct StubMetrics {
    writes: Mutex<usize>,
}

struct StubEvents {
    writes: Mutex<usize>,
}

#[async_trait]
impl MetricsStorageBackend for StubMetrics {
    fn engine_type(&self) -> StorageEngineType {
        StorageEngineType::Mem
    }

    async fn record_counter(
        &self,
        _name: &str,
        _labels: &Value,
        _delta: i64,
        _ts: DateTime<Utc>,
    ) -> Result<()> {
        *self.writes.lock() += 1;
        Ok(())
    }

    async fn record_gauge(
        &self,
        _name: &str,
        _labels: &Value,
        _value: f64,
        _ts: DateTime<Utc>,
    ) -> Result<()> {
        *self.writes.lock() += 1;
        Ok(())
    }

    async fn query_range(&self, _query: MetricsQueryRange) -> Result<Vec<MetricPoint>> {
        Ok(Vec::new())
    }
}

#[async_trait]
impl EventStorageBackend for StubEvents {
    fn engine_type(&self) -> StorageEngineType {
        StorageEngineType::Mem
    }

    async fn append_row(
        &self,
        _table: &str,
        _fields: &Value,
        _ts: DateTime<Utc>,
        _correlation_id: Option<&str>,
    ) -> Result<()> {
        *self.writes.lock() += 1;
        Ok(())
    }

    async fn query_rows(&self, _filter: EventsQueryFilter) -> Result<Vec<EventRow>> {
        Ok(Vec::new())
    }
}

#[tokio::main]
async fn main() -> spectra::Result<()> {
    let metrics = Arc::new(StubMetrics {
        writes: Mutex::new(0),
    });
    let events = Arc::new(StubEvents {
        writes: Mutex::new(0),
    });
    let metrics_be: SharedMetricsBackend = Arc::clone(&metrics) as SharedMetricsBackend;
    let events_be: SharedEventBackend = Arc::clone(&events) as SharedEventBackend;

    let _spectra = Spectra::builder()
        .metrics_backend(metrics_be)
        .events_backend(events_be)
        .embedded()
        .build()?;

    try_record_counter_now("custom_stub_hits", &[("src", "example")], 1);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let metric_writes = *metrics.writes.lock();
    eprintln!(
        "custom-backend-stub OK: metrics writes={metric_writes}, events writes={}",
        *events.writes.lock()
    );
    if metric_writes == 0 {
        std::process::exit(1);
    }
    Ok(())
}
