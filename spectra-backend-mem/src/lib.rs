//! In-memory [`MetricsStorageBackend`] and [`EventStorageBackend`] for tests and quick start.
//!
//! Inject through `SpectraBuilder::metrics_backend` / `events_backend`, or use the
//! re-exports from the `spectra` crate (`MemMetricsBackend`, `MemEventsBackend`).
//!
//! - Data is process-local and lost on exit; not suitable for production durability.
//! - `query_aggregate` supports `Count` measure only; other measures return empty series.
//! - Uses `parking_lot::RwLock`; contended writes may block the async runtime thread briefly.

use std::collections::HashMap;

use parking_lot::RwLock;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use spectra_core::{
    EventAggregateResult, EventMeasure, EventRow, EventStorageBackend, EventsAggregateFilter,
    EventsQueryFilter, LabelMatcher, MetricPoint, MetricPointDto, MetricsQueryRange,
    MetricsStorageBackend, Result, StorageEngineType, TimeSeriesDto,
};

/// Non-durable in-memory metrics storage.
///
/// Counter and gauge writes are appended as points and can be queried immediately. This backend
/// is the default `spectra` backend and is useful for local development and tests.
///
/// # Examples
///
/// Facade wiring through `Spectra::builder()` (requires the `spectra` crate):
///
/// ```ignore
/// use std::sync::Arc;
/// use spectra::{MemEventsBackend, MemMetricsBackend, Spectra};
///
/// let spectra = Spectra::builder()
///     .metrics_backend(Arc::new(MemMetricsBackend::new()))
///     .events_backend(Arc::new(MemEventsBackend::new()))
///     .embedded()
///     .build()?;
/// ```
///
/// Direct backend usage:
///
/// ```no_run
/// use chrono::{Duration, Utc};
/// use serde_json::json;
/// use spectra_backend_mem::MemMetricsBackend;
/// use spectra_core::{MetricsQueryRange, MetricsStorageBackend};
///
/// # async fn example() -> spectra_core::Result<()> {
/// let backend = MemMetricsBackend::new();
/// let now = Utc::now();
/// backend.record_counter("cache_hits", &json!({"region": "us"}), 1, now).await?;
///
/// let points = backend.query_range(MetricsQueryRange {
///     metric_name: "cache_hits".into(),
///     start: now - Duration::seconds(1),
///     end: now + Duration::seconds(1),
///     label_matchers: vec![],
/// }).await?;
/// assert_eq!(points.len(), 1);
/// # Ok(())
/// # }
/// ```
#[derive(Default)]
pub struct MemMetricsBackend {
    points: RwLock<Vec<StoredMetric>>,
}

#[derive(Clone)]
struct StoredMetric {
    name: String,
    value: f64,
    labels: Value,
    ts: DateTime<Utc>,
}

impl MemMetricsBackend {
    /// Create an empty in-memory metrics backend.
    ///
    /// Writes are process-local and discarded when the process exits. Inject the result into
    /// `Spectra::builder().metrics_backend(...)` for a full runtime, or call storage trait
    /// methods directly in tests.
    ///
    /// # Examples
    ///
    /// ```
    /// use spectra_backend_mem::MemMetricsBackend;
    /// use spectra_core::{MetricsStorageBackend, StorageEngineType};
    ///
    /// let backend = MemMetricsBackend::new();
    /// assert_eq!(backend.engine_type(), StorageEngineType::Mem);
    /// ```
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl MetricsStorageBackend for MemMetricsBackend {
    fn engine_type(&self) -> StorageEngineType {
        StorageEngineType::Mem
    }

    async fn record_counter(
        &self,
        name: &str,
        labels: &Value,
        delta: i64,
        ts: DateTime<Utc>,
    ) -> Result<()> {
        let mut guard = self.points.write();
        guard.push(StoredMetric {
            name: name.to_string(),
            value: delta as f64,
            labels: labels.clone(),
            ts,
        });
        Ok(())
    }

    async fn record_gauge(
        &self,
        name: &str,
        labels: &Value,
        value: f64,
        ts: DateTime<Utc>,
    ) -> Result<()> {
        let mut guard = self.points.write();
        guard.push(StoredMetric {
            name: name.to_string(),
            value,
            labels: labels.clone(),
            ts,
        });
        Ok(())
    }

    async fn query_range(&self, query: MetricsQueryRange) -> Result<Vec<MetricPoint>> {
        let guard = self.points.read();
        Ok(guard
            .iter()
            .filter(|p| p.name == query.metric_name)
            .filter(|p| p.ts >= query.start && p.ts <= query.end)
            .filter(|p| labels_match(&p.labels, &query.label_matchers))
            .map(|p| MetricPoint {
                ts: p.ts,
                value: p.value,
                labels: p.labels.clone(),
            })
            .collect())
    }
}

/// Non-durable in-memory structured-event storage keyed by logical table name.
///
/// Rows are available immediately after append, which makes this backend useful for tests and
/// local development.
///
/// # Examples
///
/// Facade wiring through `Spectra::builder()` (requires the `spectra` crate):
///
/// ```ignore
/// use std::sync::Arc;
/// use spectra::{MemEventsBackend, MemMetricsBackend, Spectra};
///
/// let spectra = Spectra::builder()
///     .metrics_backend(Arc::new(MemMetricsBackend::new()))
///     .events_backend(Arc::new(MemEventsBackend::new()))
///     .embedded()
///     .build()?;
/// ```
///
/// Direct backend usage:
///
/// ```no_run
/// use chrono::Utc;
/// use serde_json::json;
/// use spectra_backend_mem::MemEventsBackend;
/// use spectra_core::{EventStorageBackend, EventsQueryFilter};
///
/// # async fn example() -> spectra_core::Result<()> {
/// let backend = MemEventsBackend::new();
/// backend.append_row(
///     "request_log",
///     &json!({"message": "handled"}),
///     Utc::now(),
///     None,
/// ).await?;
///
/// let rows = backend.query_rows(EventsQueryFilter {
///     table: "request_log".into(),
///     ..Default::default()
/// }).await?;
/// assert_eq!(rows.len(), 1);
/// # Ok(())
/// # }
/// ```
#[derive(Default)]
pub struct MemEventsBackend {
    rows: RwLock<HashMap<String, Vec<StoredEvent>>>,
}

#[derive(Clone)]
struct StoredEvent {
    fields: Value,
    ts: DateTime<Utc>,
}

impl MemEventsBackend {
    /// Create an empty in-memory events backend.
    ///
    /// Rows are keyed by logical table name and discarded when the process exits. Inject the
    /// result into `Spectra::builder().events_backend(...)` for a full runtime.
    ///
    /// # Examples
    ///
    /// ```
    /// use spectra_backend_mem::MemEventsBackend;
    /// use spectra_core::{EventStorageBackend, StorageEngineType};
    ///
    /// let backend = MemEventsBackend::new();
    /// assert_eq!(backend.engine_type(), StorageEngineType::Mem);
    /// ```
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl EventStorageBackend for MemEventsBackend {
    fn engine_type(&self) -> StorageEngineType {
        StorageEngineType::Mem
    }

    async fn append_row(
        &self,
        table: &str,
        fields: &Value,
        ts: DateTime<Utc>,
        _correlation_id: Option<&str>,
    ) -> Result<()> {
        let mut guard = self.rows.write();
        guard
            .entry(table.to_string())
            .or_default()
            .push(StoredEvent {
                fields: fields.clone(),
                ts,
            });
        Ok(())
    }

    async fn query_rows(&self, filter: EventsQueryFilter) -> Result<Vec<EventRow>> {
        let guard = self.rows.read();
        let Some(rows) = guard.get(&filter.table) else {
            return Ok(Vec::new());
        };
        let out: Vec<EventRow> = rows
            .iter()
            .filter(|r| filter.start.map(|s| r.ts >= s).unwrap_or(true))
            .filter(|r| filter.end.map(|e| r.ts <= e).unwrap_or(true))
            .map(|r| EventRow {
                ts: r.ts,
                fields: r.fields.clone(),
            })
            .collect();
        Ok(spectra_core::finalize_event_rows(out, &filter))
    }

    async fn query_aggregate(&self, filter: EventsAggregateFilter) -> Result<EventAggregateResult> {
        let rows = self
            .query_rows(EventsQueryFilter {
                table: filter.table.clone(),
                start: Some(filter.start),
                end: Some(filter.end),
                partition: filter.partition.clone(),
                limit: None,
                offset: None,
                sort_field: None,
                sort_desc: false,
                filter: filter.filter.clone(),
            })
            .await?;
        let count = rows.len() as u64;
        match filter.measure {
            EventMeasure::Count => Ok(EventAggregateResult::TimeSeries {
                series: vec![TimeSeriesDto {
                    labels: json!({}),
                    points: vec![MetricPointDto {
                        ts: filter.end,
                        value: count as f64,
                    }],
                }],
                headline: vec![],
            }),
            _ => Ok(EventAggregateResult::TimeSeries {
                series: vec![],
                headline: vec![],
            }),
        }
    }
}

fn labels_match(labels: &Value, matchers: &[LabelMatcher]) -> bool {
    matchers.iter().all(|m| {
        labels
            .get(&m.key)
            .and_then(|v| v.as_str())
            .map(|v| v == m.value)
            .unwrap_or(false)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use serde_json::json;

    #[tokio::test]
    async fn metrics_roundtrip() {
        let backend = MemMetricsBackend::new();
        let ts = Utc::now();
        backend
            .record_counter("hits", &json!({"k": "v"}), 3, ts)
            .await
            .expect("write");
        let points = backend
            .query_range(MetricsQueryRange {
                metric_name: "hits".into(),
                start: ts - Duration::seconds(1),
                end: ts + Duration::seconds(1),
                label_matchers: vec![],
            })
            .await
            .expect("query");
        assert_eq!(points.len(), 1);
        assert!((points[0].value - 3.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn events_roundtrip() {
        let backend = MemEventsBackend::new();
        let ts = Utc::now();
        backend
            .append_row("req_log", &json!({"msg": "ok"}), ts, None)
            .await
            .expect("write");
        let rows = backend
            .query_rows(EventsQueryFilter {
                table: "req_log".into(),
                start: Some(ts - Duration::seconds(1)),
                end: Some(ts + Duration::seconds(1)),
                ..Default::default()
            })
            .await
            .expect("query");
        assert_eq!(rows.len(), 1);
    }
}
