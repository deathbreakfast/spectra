use crate::sink::SpectraSink;

/// Sink that discards all records (default before host wiring).
#[derive(Debug, Default, Clone, Copy)]
pub struct NoOpSink;

impl SpectraSink for NoOpSink {
    fn record_counter(&self, _name: &str, _labels: &[(&str, &str)], _delta: i64) {}

    fn record_gauge(&self, _name: &str, _labels: &[(&str, &str)], _value: f64) {}

    fn log_event(&self, _table: &str, _fields: &serde_json::Value) {}
}
