//! Wraps an inner [`SpectraSink`] while tallying rootcause emit counters per kind.

use std::sync::Arc;

use crate::rootcause;
use crate::sink::SpectraSink;

/// Forwards to an inner sink while incrementing process-global emit counters.
#[derive(Clone)]
pub struct CountingSink {
    inner: Arc<dyn SpectraSink>,
}

impl CountingSink {
    /// Wraps an inner sink with rootcause emit counters.
    pub fn new(inner: Arc<dyn SpectraSink>) -> Self {
        Self { inner }
    }

    /// Installs this sink as the process-global [`SpectraSink`](crate::SpectraSink).
    pub fn install(self) {
        crate::set_sink(Arc::new(self));
    }
}

impl SpectraSink for CountingSink {
    fn record_counter(&self, name: &str, labels: &[(&str, &str)], delta: i64) {
        rootcause::record_emit_counter();
        self.inner.record_counter(name, labels, delta);
    }

    fn record_gauge(&self, name: &str, labels: &[(&str, &str)], value: f64) {
        rootcause::record_emit_gauge();
        self.inner.record_gauge(name, labels, value);
    }

    fn log_event(&self, table: &str, fields: &serde_json::Value) {
        rootcause::record_emit_event();
        self.inner.log_event(table, fields);
    }
}
