//! In-memory store for unit tests (no ClickHouse server required).

use std::sync::RwLock;

use chrono::{DateTime, Utc};
use serde_json::Value;
use spectra_core::{
    EventRow, EventsQueryFilter, LabelMatcher, MetricPoint, MetricsQueryRange, Result,
};

#[derive(Default)]
pub struct MemStore {
    metrics: RwLock<Vec<StoredMetric>>,
    events: RwLock<Vec<StoredEvent>>,
}

#[derive(Clone)]
struct StoredMetric {
    name: String,
    value: f64,
    labels: Value,
    ts: DateTime<Utc>,
}

#[derive(Clone)]
struct StoredEvent {
    table: String,
    fields: Value,
    ts: DateTime<Utc>,
}

impl MemStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_counter(
        &self,
        name: &str,
        labels: &Value,
        delta: i64,
        ts: DateTime<Utc>,
    ) -> Result<()> {
        self.metrics
            .write()
            .expect("mem store lock")
            .push(StoredMetric {
                name: name.to_string(),
                value: delta as f64,
                labels: labels.clone(),
                ts,
            });
        Ok(())
    }

    pub fn record_gauge(
        &self,
        name: &str,
        labels: &Value,
        value: f64,
        ts: DateTime<Utc>,
    ) -> Result<()> {
        self.metrics
            .write()
            .expect("mem store lock")
            .push(StoredMetric {
                name: name.to_string(),
                value,
                labels: labels.clone(),
                ts,
            });
        Ok(())
    }

    pub fn query_range(&self, query: MetricsQueryRange) -> Result<Vec<MetricPoint>> {
        let guard = self.metrics.read().expect("mem store lock");
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

    pub fn append_row(
        &self,
        table: &str,
        fields: &Value,
        ts: DateTime<Utc>,
    ) -> Result<()> {
        self.events
            .write()
            .expect("mem store lock")
            .push(StoredEvent {
                table: table.to_string(),
                fields: fields.clone(),
                ts,
            });
        Ok(())
    }

    pub fn query_rows(&self, filter: EventsQueryFilter) -> Result<Vec<EventRow>> {
        let guard = self.events.read().expect("mem store lock");
        let out: Vec<EventRow> = guard
            .iter()
            .filter(|r| r.table == filter.table)
            .filter(|r| filter.start.map(|s| r.ts >= s).unwrap_or(true))
            .filter(|r| filter.end.map(|e| r.ts <= e).unwrap_or(true))
            .map(|r| EventRow {
                ts: r.ts,
                fields: r.fields.clone(),
            })
            .collect();
        Ok(spectra_core::finalize_event_rows(out, &filter))
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
    use serde_json::json;

    #[test]
    fn metrics_roundtrip() {
        let store = MemStore::new();
        let ts = Utc::now();
        store
            .record_counter("hits", &json!({}), 2, ts)
            .expect("write");
        let points = store
            .query_range(MetricsQueryRange {
                metric_name: "hits".into(),
                start: ts - chrono::Duration::seconds(1),
                end: ts + chrono::Duration::seconds(1),
                label_matchers: vec![],
            })
            .expect("query");
        assert_eq!(points.len(), 1);
    }
}
