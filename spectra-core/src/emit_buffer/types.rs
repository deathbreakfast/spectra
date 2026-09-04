//! Buffered emit record types.

use chrono::{DateTime, Utc};

/// A single emit captured in the buffer, with the timestamp taken at emit time.
pub enum BufferedEmit {
    /// Buffered counter increment.
    Counter {
        /// Metric family name.
        name: String,
        /// Label key-value pairs.
        labels: Vec<(String, String)>,
        /// Increment amount.
        delta: i64,
        /// Wall-clock timestamp captured at emit time.
        ts: DateTime<Utc>,
    },
    /// Buffered gauge sample.
    Gauge {
        /// Metric family name.
        name: String,
        /// Label key-value pairs.
        labels: Vec<(String, String)>,
        /// Gauge value.
        value: f64,
        /// Wall-clock timestamp captured at emit time.
        ts: DateTime<Utc>,
    },
    /// Buffered structured event row.
    Event {
        /// Event table name.
        table: String,
        /// Event field payload.
        fields: serde_json::Value,
        /// Wall-clock timestamp captured at emit time.
        ts: DateTime<Utc>,
    },
}
