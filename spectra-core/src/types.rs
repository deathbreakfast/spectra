//! Emit envelope types for host transport publish wrappers.
//!
//! These DTOs cross the **publisher → bus → consumer** boundary. Publishers serialize them
//! (or the `*Payload` types expanded beside each schema); consumers deserialize and
//! persist. Direct in-process emit usually uses typed helpers from the same macros.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Metric kind written to NDJSON / storage adapters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MetricKind {
    /// Monotonically increasing counter.
    Counter,
    /// Point-in-time gauge sample.
    Gauge,
}

/// Structured event envelope for transport publish wrappers.
///
/// **Consumer side:** deserialize from your bus, then append via
/// [`crate::EventStorageBackend`] or a Spectra runtime that owns events storage.
/// Direct in-process logging usually uses a generated `*Logger` or `try_log_event`.
///
/// # Examples
///
/// ```
/// use serde_json::json;
/// use spectra_core::SpectraEvent;
///
/// let event = SpectraEvent::new(
///     "request_log",
///     json!({"message": "handled", "status": 200}),
/// );
/// assert_eq!(event.table, "request_log");
/// assert!(event.ts.is_none());
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpectraEvent {
    /// Event table name.
    pub table: String,
    /// Event field payload.
    pub fields: Value,
    /// Optional explicit timestamp (defaults to sink time when absent).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ts: Option<DateTime<Utc>>,
}

impl SpectraEvent {
    /// Creates an event without an explicit timestamp.
    pub fn new(table: impl Into<String>, fields: Value) -> Self {
        Self {
            table: table.into(),
            fields,
            ts: None,
        }
    }

    /// Creates an event with an explicit timestamp.
    pub fn with_ts(table: impl Into<String>, fields: Value, ts: DateTime<Utc>) -> Self {
        Self {
            table: table.into(),
            fields,
            ts: Some(ts),
        }
    }
}

/// Metric emit envelope for transport publish wrappers.
///
/// **Publisher side:** build from a schema `*Payload` (or these constructors) and
/// publish on your bus. **Consumer side:** deserialize, then record via
/// [`crate::MetricsStorageBackend`] or `try_record_counter_now` /
/// `try_log_event_now` on the `spectra` facade.
///
/// The constructors preserve an explicit emit timestamp and set only the value field associated
/// with the selected [`MetricKind`]. Direct in-process counters normally use typed recorders
/// or `try_record_counter`.
///
/// # Examples
///
/// ```
/// use chrono::Utc;
/// use serde_json::json;
/// use spectra_core::{MetricEmit, MetricKind};
///
/// let emit = MetricEmit::counter(
///     "cache_hits",
///     json!({"region": "us-west"}),
///     1,
///     Utc::now(),
/// );
/// assert_eq!(emit.kind, MetricKind::Counter);
/// assert_eq!(emit.delta, Some(1));
/// assert!(emit.value.is_none());
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricEmit {
    /// Metric family name.
    pub name: String,
    /// Counter or gauge kind.
    pub kind: MetricKind,
    /// Label set as JSON.
    pub labels: Value,
    /// Counter delta when `kind` is [`MetricKind::Counter`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta: Option<i64>,
    /// Gauge value when `kind` is [`MetricKind::Gauge`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    /// Optional explicit timestamp.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ts: Option<DateTime<Utc>>,
}

impl MetricEmit {
    /// Builds a counter emit envelope.
    pub fn counter(name: impl Into<String>, labels: Value, delta: i64, ts: DateTime<Utc>) -> Self {
        Self {
            name: name.into(),
            kind: MetricKind::Counter,
            labels,
            delta: Some(delta),
            value: None,
            ts: Some(ts),
        }
    }

    /// Builds a gauge emit envelope.
    pub fn gauge(name: impl Into<String>, labels: Value, value: f64, ts: DateTime<Utc>) -> Self {
        Self {
            name: name.into(),
            kind: MetricKind::Gauge,
            labels,
            delta: None,
            value: Some(value),
            ts: Some(ts),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn metric_emit_serializes_counter() {
        let emit = MetricEmit::counter("hits", json!({"region": "us"}), 3, Utc::now());
        let v = serde_json::to_value(&emit).expect("serialize");
        assert_eq!(v["kind"], "counter");
        assert_eq!(v["delta"], 3);
    }

    #[test]
    fn spectra_event_optional_ts() {
        let ev = SpectraEvent::new("log", json!({"msg": "hi"}));
        assert!(ev.ts.is_none());
        let ts = Utc::now();
        let ev = SpectraEvent::with_ts("log", json!({}), ts);
        assert_eq!(ev.ts, Some(ts));
    }
}
