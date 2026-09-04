//! Real two-process publisher: TCP JSON-lines bus + `.persist_disabled()`.
//!
//! Spectra does not ship a message bus — this example implements a minimal host sink so the
//! publish-consume topology is runnable without Photon. Replace [`TcpJsonSink`] with a Photon /
//! NATS adapter in production. Pair with `bus_consume`.
//!
//! Start order: **consumer first**, then this publisher.
//!
//! ```bash
//! export SPECTRA_BUS_ADDR=127.0.0.1:9809
//! # Terminal 1
//! cargo run -p uf-spectra --example bus_consume --features mem
//! # Terminal 2
//! cargo run -p uf-spectra --example bus_publish --features mem
//! ```

#![allow(clippy::print_stderr)]

use std::io::Write;
use std::net::TcpStream;
use std::sync::{Arc, Mutex};

use serde_json::{json, Map, Value};
use spectra::{try_record_counter_now, MemEventsBackend, MemMetricsBackend, Spectra, SpectraSink};
use spectra_core::{SharedEventBackend, SharedMetricsBackend};

/// Host-owned bus adapter: each emit becomes one JSON line on a TCP connection.
struct TcpJsonSink {
    stream: Mutex<TcpStream>,
}

impl TcpJsonSink {
    fn connect(addr: &str) -> spectra::Result<Self> {
        let stream = TcpStream::connect(addr).map_err(|e| {
            spectra::Error::Config(format!(
                "connect {addr}: {e} (start bus_consume first; SPECTRA_BUS_ADDR)"
            ))
        })?;
        stream.set_nodelay(true).ok();
        Ok(Self {
            stream: Mutex::new(stream),
        })
    }

    fn write_line(&self, value: Value) {
        let Ok(mut guard) = self.stream.lock() else {
            return;
        };
        let mut line = value.to_string();
        line.push('\n');
        let _ = guard.write_all(line.as_bytes());
        let _ = guard.flush();
    }

    fn labels_object(labels: &[(&str, &str)]) -> Value {
        let mut map = Map::new();
        for (k, v) in labels {
            map.insert((*k).into(), json!(*v));
        }
        Value::Object(map)
    }
}

impl SpectraSink for TcpJsonSink {
    fn record_counter(&self, name: &str, labels: &[(&str, &str)], delta: i64) {
        self.write_line(json!({
            "kind": "counter",
            "name": name,
            "labels": Self::labels_object(labels),
            "delta": delta,
        }));
    }

    fn record_gauge(&self, name: &str, labels: &[(&str, &str)], value: f64) {
        self.write_line(json!({
            "kind": "gauge",
            "name": name,
            "labels": Self::labels_object(labels),
            "value": value,
        }));
    }

    fn log_event(&self, table: &str, fields: &Value) {
        self.write_line(json!({
            "kind": "event",
            "table": table,
            "fields": fields,
        }));
    }
}

#[tokio::main]
async fn main() -> spectra::Result<()> {
    let addr = std::env::var("SPECTRA_BUS_ADDR").unwrap_or_else(|_| "127.0.0.1:9809".into());

    let metrics: SharedMetricsBackend = Arc::new(MemMetricsBackend::new());
    let events: SharedEventBackend = Arc::new(MemEventsBackend::new());
    let sink = Arc::new(TcpJsonSink::connect(&addr)?);

    let spectra = Spectra::builder()
        .metrics_backend(Arc::clone(&metrics))
        .events_backend(Arc::clone(&events))
        .sink(Arc::clone(&sink) as Arc<dyn SpectraSink>)
        .persist_disabled()
        .build()?;

    try_record_counter_now("bus_demo_hits", &[("region", "us-west")], 1);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let now = spectra_core::current_emit_ts();
    let points = spectra
        .router()
        .query_metrics(spectra_core::MetricsQueryRange {
            metric_name: "bus_demo_hits".into(),
            start: now - chrono::Duration::seconds(5),
            end: now + chrono::Duration::seconds(1),
            label_matchers: vec![],
        })
        .await?;
    assert!(
        points.is_empty(),
        "publisher must not persist when persist_disabled"
    );

    eprintln!("bus-publish OK: counter sent to {addr}; local storage empty");
    Ok(())
}
