//! Synchronous fan-out to multiple [`SpectraSink`] implementations.

use std::sync::Arc;

use serde_json::Value;

use crate::sink::SpectraSink;

/// Forwards each emit to every sink in the chain, synchronously and in registration order.
///
/// Use a chain when one emit must reach multiple transports or telemetry destinations. A slow
/// child sink delays later children and the caller.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use spectra_core::{ChainedSink, RecordingSink, SpectraSink};
///
/// let first = Arc::new(RecordingSink::new());
/// let second = Arc::new(RecordingSink::new());
/// let chain = ChainedSink::new()
///     .push(Arc::clone(&first) as Arc<dyn SpectraSink>)
///     .push(Arc::clone(&second) as Arc<dyn SpectraSink>);
///
/// chain.record_counter("cache_hits", &[("region", "us")], 1);
/// assert_eq!(first.counters().len(), 1);
/// assert_eq!(second.counters().len(), 1);
/// ```
#[derive(Default)]
pub struct ChainedSink {
    sinks: Vec<Arc<dyn SpectraSink>>,
}

impl ChainedSink {
    /// Creates an empty sink chain.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a sink to the end of the chain.
    pub fn push(mut self, sink: Arc<dyn SpectraSink>) -> Self {
        self.sinks.push(sink);
        self
    }

    /// Returns the number of sinks in the chain.
    pub fn len(&self) -> usize {
        self.sinks.len()
    }

    /// Returns whether the chain has no sinks.
    pub fn is_empty(&self) -> bool {
        self.sinks.is_empty()
    }
}

impl SpectraSink for ChainedSink {
    fn record_counter(&self, name: &str, labels: &[(&str, &str)], delta: i64) {
        for sink in &self.sinks {
            sink.record_counter(name, labels, delta);
        }
    }

    fn record_gauge(&self, name: &str, labels: &[(&str, &str)], value: f64) {
        for sink in &self.sinks {
            sink.record_gauge(name, labels, value);
        }
    }

    fn log_event(&self, table: &str, fields: &Value) {
        for sink in &self.sinks {
            sink.log_event(table, fields);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sinks::RecordingSink;
    use serde_json::json;

    #[test]
    fn fans_out_to_all_sinks() {
        let a = Arc::new(RecordingSink::new());
        let b = Arc::new(RecordingSink::new());
        let chain = ChainedSink::new()
            .push(Arc::clone(&a) as Arc<dyn SpectraSink>)
            .push(Arc::clone(&b) as Arc<dyn SpectraSink>);

        chain.record_counter("hits", &[("region", "us")], 2);
        chain.record_gauge("load", &[("host", "a")], 0.5);
        chain.log_event("request_log", &json!({"status": 200}));

        assert_eq!(a.counters().len(), 1);
        assert_eq!(b.counters().len(), 1);
        assert_eq!(a.gauges().len(), 1);
        assert_eq!(b.gauges().len(), 1);
        assert_eq!(a.events().len(), 1);
        assert_eq!(b.events().len(), 1);
    }
}
