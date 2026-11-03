//! In-memory store for unit tests (no ClickHouse server required).

use parking_lot::RwLock;

use chrono::{DateTime, Utc};
use serde_json::Value;
use spectra_core::{
    EventRow, EventsQueryFilter, LabelMatcher, MetricPoint, MetricsQueryRange, Result, StorageStats,
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
        self.metrics.write().push(StoredMetric {
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
        self.metrics.write().push(StoredMetric {
            name: name.to_string(),
            value,
            labels: labels.clone(),
            ts,
        });
        Ok(())
    }

    pub fn query_range(&self, query: MetricsQueryRange) -> Result<Vec<MetricPoint>> {
        let guard = self.metrics.read();
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

    pub fn metric_stats(&self, name: &str) -> StorageStats {
        stats_of(
            self.metrics
                .read()
                .iter()
                .filter(|m| m.name == name)
                .map(|m| m.ts),
        )
    }

    pub fn event_stats(&self, table: &str) -> StorageStats {
        stats_of(
            self.events
                .read()
                .iter()
                .filter(|e| e.table == table)
                .map(|e| e.ts),
        )
    }

    /// Removes up to `max_rows` metrics older than `before`, oldest first.
    pub fn prune_metrics(&self, before: DateTime<Utc>, max_rows: u32) -> u64 {
        let mut guard = self.metrics.write();
        let mut stale: Vec<(DateTime<Utc>, usize)> = guard
            .iter()
            .enumerate()
            .filter(|(_, m)| m.ts < before)
            .map(|(i, m)| (m.ts, i))
            .collect();
        stale.sort();
        stale.truncate(max_rows as usize);
        remove_indices(&mut guard, stale)
    }

    /// Removes up to `max_rows` rows of `table` older than `before`, oldest first.
    pub fn prune_events(&self, table: &str, before: DateTime<Utc>, max_rows: u32) -> u64 {
        let mut guard = self.events.write();
        let mut stale: Vec<(DateTime<Utc>, usize)> = guard
            .iter()
            .enumerate()
            .filter(|(_, e)| e.table == table && e.ts < before)
            .map(|(i, e)| (e.ts, i))
            .collect();
        stale.sort();
        stale.truncate(max_rows as usize);
        remove_indices(&mut guard, stale)
    }

    pub fn append_row(&self, table: &str, fields: &Value, ts: DateTime<Utc>) -> Result<()> {
        self.events.write().push(StoredEvent {
            table: table.to_string(),
            fields: fields.clone(),
            ts,
        });
        Ok(())
    }

    pub fn query_rows(&self, filter: EventsQueryFilter) -> Result<Vec<EventRow>> {
        let out = self.scoped_events(&filter.table, filter.start, filter.end);
        Ok(spectra_core::finalize_event_rows(out, &filter))
    }

    /// Rows for `table` within the inclusive time bounds, with no filter or paging.
    pub fn scoped_rows(
        &self,
        table: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Vec<EventRow> {
        self.scoped_events(table, Some(start), Some(end))
    }

    fn scoped_events(
        &self,
        table: &str,
        start: Option<DateTime<Utc>>,
        end: Option<DateTime<Utc>>,
    ) -> Vec<EventRow> {
        let guard = self.events.read();
        guard
            .iter()
            .filter(|r| r.table == table)
            .filter(|r| start.map(|s| r.ts >= s).unwrap_or(true))
            .filter(|r| end.map(|e| r.ts <= e).unwrap_or(true))
            .map(|r| EventRow {
                ts: r.ts,
                fields: r.fields.clone(),
            })
            .collect()
    }
}

fn stats_of(timestamps: impl Iterator<Item = DateTime<Utc>>) -> StorageStats {
    let mut stats = StorageStats::default();
    for ts in timestamps {
        stats.rows += 1;
        stats.first_ts = Some(stats.first_ts.map_or(ts, |t| t.min(ts)));
        stats.last_ts = Some(stats.last_ts.map_or(ts, |t| t.max(ts)));
    }
    stats
}

fn remove_indices<T>(rows: &mut Vec<T>, picked: Vec<(DateTime<Utc>, usize)>) -> u64 {
    let mut idx: Vec<usize> = picked.into_iter().map(|(_, i)| i).collect();
    idx.sort_unstable_by(|a, b| b.cmp(a));
    for i in &idx {
        rows.remove(*i);
    }
    idx.len() as u64
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
    fn prune_respects_cap_and_table() {
        let store = MemStore::new();
        let now = Utc::now();
        let old = now - chrono::Duration::days(10);
        for _ in 0..3 {
            store.append_row("a", &json!({}), old).expect("write");
        }
        store.append_row("b", &json!({}), old).expect("write");
        store.append_row("a", &json!({}), now).expect("write");
        let cutoff = now - chrono::Duration::days(1);
        assert_eq!(store.prune_events("a", cutoff, 2), 2);
        assert_eq!(store.prune_events("a", cutoff, 2), 1);
        assert_eq!(store.prune_events("a", cutoff, 2), 0);
        assert_eq!(store.prune_events("b", cutoff, 2), 1);
        store
            .record_counter("m", &json!({}), 1, old)
            .expect("write");
        assert_eq!(store.prune_metrics(cutoff, 5), 1);
    }

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
