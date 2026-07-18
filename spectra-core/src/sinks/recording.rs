//! In-memory [`SpectraSink`] for tests.

use std::sync::{Arc, Mutex};

use serde_json::Value;

use crate::sink::SpectraSink;

/// Captured counter increment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedCounter {
    pub name: String,
    pub labels: Vec<(String, String)>,
    pub delta: i64,
}

/// Captured gauge sample.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordedGauge {
    pub name: String,
    pub labels: Vec<(String, String)>,
    pub value: f64,
}

/// Captured structured event row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedEvent {
    pub table: String,
    pub fields: Value,
}

#[derive(Debug, Default)]
struct Inner {
    counters: Vec<RecordedCounter>,
    gauges: Vec<RecordedGauge>,
    events: Vec<RecordedEvent>,
}

/// Append-only in-memory sink for assertions in unit and integration tests.
///
/// Clones share the same captured data. Use the typed accessors to inspect all emits or the
/// matching helpers to select one metric name, label subset, or event table.
///
/// # Examples
///
/// ```
/// use spectra_core::{RecordingSink, SpectraSink};
///
/// let sink = RecordingSink::new();
/// sink.record_counter("cache_hits", &[("region", "us")], 2);
///
/// let hits = sink.recorded_counters_matching("cache_hits", &[("region", "us")]);
/// assert_eq!(hits.len(), 1);
/// assert_eq!(hits[0].delta, 2);
///
/// sink.clear();
/// assert!(sink.counters().is_empty());
/// ```
#[derive(Debug, Clone)]
pub struct RecordingSink {
    inner: Arc<Mutex<Inner>>,
}

impl Default for RecordingSink {
    fn default() -> Self {
        Self::new()
    }
}

impl RecordingSink {
    /// Creates an empty recording sink.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner::default())),
        }
    }

    /// Clears all recorded emits.
    pub fn clear(&self) {
        let mut g = self.inner.lock().expect("recording sink lock");
        g.counters.clear();
        g.gauges.clear();
        g.events.clear();
    }

    /// Returns all recorded counter increments.
    pub fn counters(&self) -> Vec<RecordedCounter> {
        self.inner
            .lock()
            .expect("recording sink lock")
            .counters
            .clone()
    }

    /// Returns all recorded gauge samples.
    pub fn gauges(&self) -> Vec<RecordedGauge> {
        self.inner
            .lock()
            .expect("recording sink lock")
            .gauges
            .clone()
    }

    /// Returns all recorded events.
    pub fn events(&self) -> Vec<RecordedEvent> {
        self.inner
            .lock()
            .expect("recording sink lock")
            .events
            .clone()
    }

    /// Returns counters matching a name and label subset.
    pub fn recorded_counters_matching(
        &self,
        name: &str,
        label_subset: &[(&str, &str)],
    ) -> Vec<RecordedCounter> {
        self.counters()
            .into_iter()
            .filter(|c| c.name == name && labels_contain(&c.labels, label_subset))
            .collect()
    }

    /// Returns events recorded for a specific table.
    pub fn recorded_events_for(&self, table: &str) -> Vec<RecordedEvent> {
        self.events()
            .into_iter()
            .filter(|e| e.table == table)
            .collect()
    }
}

fn labels_contain(labels: &[(String, String)], subset: &[(&str, &str)]) -> bool {
    subset.iter().all(|(k, v)| {
        labels
            .iter()
            .any(|(lk, lv)| lk.as_str() == *k && lv.as_str() == *v)
    })
}

impl SpectraSink for RecordingSink {
    fn record_counter(&self, name: &str, labels: &[(&str, &str)], delta: i64) {
        let labels: Vec<(String, String)> = labels
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        self.inner
            .lock()
            .expect("recording sink lock")
            .counters
            .push(RecordedCounter {
                name: name.to_string(),
                labels,
                delta,
            });
    }

    fn record_gauge(&self, name: &str, labels: &[(&str, &str)], value: f64) {
        let labels: Vec<(String, String)> = labels
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        self.inner
            .lock()
            .expect("recording sink lock")
            .gauges
            .push(RecordedGauge {
                name: name.to_string(),
                labels,
                value,
            });
    }

    fn log_event(&self, table: &str, fields: &Value) {
        self.inner
            .lock()
            .expect("recording sink lock")
            .events
            .push(RecordedEvent {
                table: table.to_string(),
                fields: fields.clone(),
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn captures_counters_and_events() {
        let sink = RecordingSink::new();
        sink.record_counter("example_db_reads", &[("table", "t"), ("op", "get")], 1);
        sink.log_event("example_error_log", &json!({"source": "database"}));

        assert_eq!(sink.counters().len(), 1);
        assert_eq!(sink.events().len(), 1);
        let hits = sink.recorded_counters_matching("example_db_reads", &[("op", "get")]);
        assert_eq!(hits.len(), 1);
    }
}
