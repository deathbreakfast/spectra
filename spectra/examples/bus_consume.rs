//! Real two-process consumer: TCP JSON-lines bus → mem persist.
//!
//! Pair with `bus_publish`. In production, replace the TCP listener with a Photon / NATS
//! subscriber that decodes schema `*Payload` envelopes, then call `try_record_*_at`.
//!
//! ```bash
//! export SPECTRA_BUS_ADDR=127.0.0.1:9809
//! # Terminal 1 — start this first
//! cargo run -p uf-spectra --example bus_consume --features mem
//! # Terminal 2
//! cargo run -p uf-spectra --example bus_publish --features mem
//! ```
//!
//! Set `SPECTRA_BUS_IDLE_SECS` (default 8) to exit after waiting for a publisher.

#![allow(clippy::print_stderr)]

use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use serde_json::Value;
use spectra::{
    try_log_event_at, try_record_counter_at, try_record_gauge_at, MemEventsBackend,
    MemMetricsBackend, Spectra,
};
use spectra_core::{SharedEventBackend, SharedMetricsBackend};

#[tokio::main]
async fn main() -> spectra::Result<()> {
    let addr = std::env::var("SPECTRA_BUS_ADDR").unwrap_or_else(|_| "127.0.0.1:9809".into());
    let idle_secs: u64 = std::env::var("SPECTRA_BUS_IDLE_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8);

    let metrics: SharedMetricsBackend = Arc::new(MemMetricsBackend::new());
    let events: SharedEventBackend = Arc::new(MemEventsBackend::new());
    let spectra = Spectra::builder()
        .metrics_backend(Arc::clone(&metrics))
        .events_backend(Arc::clone(&events))
        .embedded()
        .build()?;

    let listener = TcpListener::bind(&addr)
        .map_err(|e| spectra::Error::Config(format!("bind {addr}: {e}")))?;
    listener
        .set_nonblocking(true)
        .map_err(|e| spectra::Error::Config(format!("nonblocking: {e}")))?;
    eprintln!("bus-consume listening on {addr}; waiting for publisher…");

    let deadline = std::time::Instant::now() + Duration::from_secs(idle_secs);
    let (stream, peer) = loop {
        match listener.accept() {
            Ok(pair) => break pair,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if std::time::Instant::now() >= deadline {
                    eprintln!("bus-consume timed out waiting for a publisher on {addr}");
                    std::process::exit(1);
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Err(e) => return Err(spectra::Error::Config(format!("accept: {e}"))),
        }
    };
    eprintln!("bus-consume accepted {peer}");
    stream
        .set_nonblocking(false)
        .map_err(|e| spectra::Error::Config(format!("blocking stream: {e}")))?;
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok();

    let mut handled = 0usize;
    let reader = BufReader::new(stream);
    for line in reader.lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let msg: Value = serde_json::from_str(&line)
            .map_err(|e| spectra::Error::Config(format!("bus JSON: {e}")))?;
        handled += handle_message(&msg)?;
    }

    tokio::time::sleep(Duration::from_millis(80)).await;

    let now = spectra_core::current_emit_ts();
    let points = spectra
        .router()
        .query_metrics(spectra_core::MetricsQueryRange {
            metric_name: "bus_demo_hits".into(),
            start: now - chrono::Duration::seconds(30),
            end: now + chrono::Duration::seconds(5),
            label_matchers: vec![],
        })
        .await?;

    eprintln!(
        "bus-consume OK: handled {handled} message(s); {} metric point(s) in storage",
        points.len()
    );
    if points.is_empty() {
        std::process::exit(1);
    }
    Ok(())
}

fn handle_message(msg: &Value) -> spectra::Result<usize> {
    let kind = msg.get("kind").and_then(|v| v.as_str()).unwrap_or("");
    let ts = Utc::now();
    match kind {
        "counter" => {
            let name = msg["name"].as_str().unwrap_or("unknown");
            let delta = msg["delta"].as_i64().unwrap_or(1);
            let owned = owned_labels(&msg["labels"]);
            let pairs: Vec<(&str, &str)> = owned
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();
            try_record_counter_at(name, &pairs, delta, ts);
            Ok(1)
        }
        "gauge" => {
            let name = msg["name"].as_str().unwrap_or("unknown");
            let value = msg["value"].as_f64().unwrap_or(0.0);
            let owned = owned_labels(&msg["labels"]);
            let pairs: Vec<(&str, &str)> = owned
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();
            try_record_gauge_at(name, &pairs, value, ts);
            Ok(1)
        }
        "event" => {
            let table = msg["table"].as_str().unwrap_or("unknown");
            let fields = msg.get("fields").cloned().unwrap_or(Value::Null);
            try_log_event_at(table, &fields, ts);
            Ok(1)
        }
        other => {
            eprintln!("bus-consume ignoring unknown kind={other}");
            Ok(0)
        }
    }
}

fn owned_labels(labels: &Value) -> Vec<(String, String)> {
    labels
        .as_object()
        .map(|obj| {
            obj.iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_owned()))
                .collect()
        })
        .unwrap_or_default()
}
