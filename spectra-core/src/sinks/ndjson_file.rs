use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use serde_json::{json, Map, Value};

use crate::error::Result;
use crate::sink::SpectraSink;
use crate::types::MetricKind;

const FLUSH_EVERY_LINES: u32 = 64;

/// Append-only NDJSON sink for monolithic observability.
///
/// Host chooses paths (typically `{data_dir}/spectra/metrics.ndjson` and `events.ndjson`).
#[derive(Debug)]
pub struct NdjsonFileSink {
    metrics_path: PathBuf,
    events_path: PathBuf,
    metrics_file: Mutex<NdjsonFileState>,
    events_file: Mutex<NdjsonFileState>,
}

#[derive(Debug)]
struct NdjsonFileState {
    file: std::fs::File,
    lines_since_flush: AtomicU32,
}

impl NdjsonFileSink {
    /// Open (or create) metrics and events NDJSON files under `metrics_path` / `events_path`.
    pub fn new(metrics_path: impl AsRef<Path>, events_path: impl AsRef<Path>) -> Result<Self> {
        let metrics_path = metrics_path.as_ref().to_path_buf();
        let events_path = events_path.as_ref().to_path_buf();
        if let Some(parent) = metrics_path.parent() {
            fs::create_dir_all(parent)?;
        }
        if let Some(parent) = events_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let metrics_file = open_append(&metrics_path)?;
        let events_file = open_append(&events_path)?;
        Ok(Self {
            metrics_path,
            events_path,
            metrics_file: Mutex::new(NdjsonFileState::new(metrics_file)),
            events_file: Mutex::new(NdjsonFileState::new(events_file)),
        })
    }

    /// Returns the path to the metrics NDJSON file.
    pub fn metrics_path(&self) -> &Path {
        &self.metrics_path
    }

    /// Returns the path to the events NDJSON file.
    pub fn events_path(&self) -> &Path {
        &self.events_path
    }
}

impl NdjsonFileState {
    fn new(file: std::fs::File) -> Self {
        Self {
            file,
            lines_since_flush: AtomicU32::new(0),
        }
    }

    fn write_line(&mut self, value: Value) {
        if let Ok(line) = serde_json::to_string(&value) {
            let _ = self.file.write_all(line.as_bytes());
            let _ = self.file.write_all(b"\n");
            let n = self.lines_since_flush.fetch_add(1, Ordering::Relaxed) + 1;
            if n >= FLUSH_EVERY_LINES {
                let _ = self.file.flush();
                self.lines_since_flush.store(0, Ordering::Relaxed);
            }
        }
    }
}

impl Drop for NdjsonFileState {
    fn drop(&mut self) {
        let _ = self.file.flush();
    }
}

fn open_append(path: &Path) -> Result<std::fs::File> {
    Ok(OpenOptions::new().create(true).append(true).open(path)?)
}

fn labels_to_map(labels: &[(&str, &str)]) -> Map<String, Value> {
    let mut map = Map::new();
    for (k, v) in labels {
        map.insert((*k).to_string(), Value::String((*v).to_string()));
    }
    map
}

fn write_line(state: &Mutex<NdjsonFileState>, value: Value) {
    let start = crate::rootcause::enabled().then(std::time::Instant::now);
    if let Ok(mut guard) = state.lock() {
        guard.write_line(value);
    }
    if let Some(start) = start {
        crate::rootcause::record_ndjson_append(start.elapsed());
    }
}

impl SpectraSink for NdjsonFileSink {
    fn record_counter(&self, name: &str, labels: &[(&str, &str)], delta: i64) {
        write_line(
            &self.metrics_file,
            json!({
                "ts": crate::emit_buffer::current_emit_ts().to_rfc3339(),
                "kind": MetricKind::Counter,
                "name": name,
                "labels": labels_to_map(labels),
                "value": delta,
            }),
        );
    }

    fn record_gauge(&self, name: &str, labels: &[(&str, &str)], value: f64) {
        write_line(
            &self.metrics_file,
            json!({
                "ts": crate::emit_buffer::current_emit_ts().to_rfc3339(),
                "kind": MetricKind::Gauge,
                "name": name,
                "labels": labels_to_map(labels),
                "value": value,
            }),
        );
    }

    fn log_event(&self, table: &str, fields: &Value) {
        write_line(
            &self.events_file,
            json!({
                "ts": crate::emit_buffer::current_emit_ts().to_rfc3339(),
                "kind": "event",
                "table": table,
                "fields": fields,
            }),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};

    fn read_ndjson_lines(path: &Path) -> Vec<Value> {
        let file = std::fs::File::open(path).expect("open ndjson file");
        BufReader::new(file)
            .lines()
            .map(|line| {
                let line = line.expect("read line");
                serde_json::from_str(&line).expect("parse ndjson line")
            })
            .collect()
    }

    #[test]
    fn append_metrics_and_events() {
        let dir = tempfile::tempdir().expect("tempdir");
        let metrics = dir.path().join("metrics.ndjson");
        let events = dir.path().join("events.ndjson");
        let sink = NdjsonFileSink::new(&metrics, &events).expect("sink");

        sink.record_counter("requests_total", &[("service", "api")], 3);
        sink.record_gauge("queue_depth", &[("shard", "0")], 12.5);
        sink.log_event(
            "service_errors",
            &json!({"code": "timeout", "detail": "upstream slow"}),
        );

        let metric_lines = read_ndjson_lines(&metrics);
        assert_eq!(metric_lines.len(), 2);

        assert_eq!(metric_lines[0]["kind"], "counter");
        assert_eq!(metric_lines[0]["name"], "requests_total");
        assert_eq!(metric_lines[0]["value"], 3);
        assert_eq!(metric_lines[0]["labels"]["service"], "api");
        assert!(metric_lines[0]["ts"].is_string());

        assert_eq!(metric_lines[1]["kind"], "gauge");
        assert_eq!(metric_lines[1]["name"], "queue_depth");
        assert_eq!(metric_lines[1]["value"], 12.5);
        assert_eq!(metric_lines[1]["labels"]["shard"], "0");

        let event_lines = read_ndjson_lines(&events);
        assert_eq!(event_lines.len(), 1);
        assert_eq!(event_lines[0]["kind"], "event");
        assert_eq!(event_lines[0]["table"], "service_errors");
        assert_eq!(event_lines[0]["fields"]["code"], "timeout");
        assert!(event_lines[0]["ts"].is_string());
    }
}
