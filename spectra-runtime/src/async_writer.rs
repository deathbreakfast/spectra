//! Off-thread console + NDJSON I/O so the Spectra hot path only enqueues.

use std::sync::mpsc::{self, SyncSender, TrySendError};
use std::sync::{Arc, OnceLock};
use std::thread::{self, JoinHandle};

use serde_json::Value;
use spectra_core::{NdjsonFileSink, SchemaRegistry, SpectraSink};

const CHANNEL_CAPACITY: usize = 65_536;

enum EmitJob {
    Counter {
        name: String,
        labels: Vec<(String, String)>,
        delta: i64,
    },
    Gauge {
        name: String,
        labels: Vec<(String, String)>,
        value: f64,
    },
    Event {
        table: String,
        fields: Value,
    },
}

static WRITER_TX: OnceLock<SyncSender<EmitJob>> = OnceLock::new();

/// Returns true when stderr console mirror is enabled (default: true).
pub fn console_mirror_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        !matches!(
            std::env::var("SPECTRA_CONSOLE").as_deref(),
            Ok("0") | Ok("false") | Ok("FALSE")
        )
    })
}

/// Returns true when hot-path emit should use the off-thread writer (default: true).
pub fn off_thread_emit_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        !matches!(
            std::env::var("SPECTRA_SYNC_HOT_PATH").as_deref(),
            Ok("1") | Ok("true") | Ok("TRUE")
        )
    })
}

/// Sink that enqueues NDJSON + optional console mirror work for a background thread.
pub struct OffThreadSpectraSink {
    ndjson: Arc<NdjsonFileSink>,
    tx: SyncSender<EmitJob>,
    _writer: Arc<JoinHandle<()>>,
}

impl OffThreadSpectraSink {
    /// Spawn the background writer thread and wire the shared emit queue.
    pub fn new(ndjson: NdjsonFileSink) -> Self {
        let ndjson = Arc::new(ndjson);
        let (tx, rx) = mpsc::sync_channel(CHANNEL_CAPACITY);
        let ndjson_for_thread = Arc::clone(&ndjson);
        let writer = thread::Builder::new()
            .name("spectra-async-writer".into())
            .spawn(move || writer_loop(rx, ndjson_for_thread))
            .expect("spawn spectra-async-writer");
        let _ = WRITER_TX.set(tx.clone());
        Self {
            ndjson,
            tx,
            _writer: Arc::new(writer),
        }
    }

    /// Underlying NDJSON file sink (used by the worker thread).
    pub fn ndjson(&self) -> &NdjsonFileSink {
        &self.ndjson
    }
}

impl SpectraSink for OffThreadSpectraSink {
    fn record_counter(&self, name: &str, labels: &[(&str, &str)], delta: i64) {
        if !off_thread_emit_enabled() {
            self.ndjson.record_counter(name, labels, delta);
            return;
        }
        let job = EmitJob::Counter {
            name: name.to_string(),
            labels: labels
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            delta,
        };
        try_enqueue(&self.tx, job);
    }

    fn record_gauge(&self, name: &str, labels: &[(&str, &str)], value: f64) {
        if !off_thread_emit_enabled() {
            self.ndjson.record_gauge(name, labels, value);
            return;
        }
        let job = EmitJob::Gauge {
            name: name.to_string(),
            labels: labels
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            value,
        };
        try_enqueue(&self.tx, job);
    }

    fn log_event(&self, table: &str, fields: &Value) {
        if !off_thread_emit_enabled() {
            if console_mirror_enabled() {
                if let Some(line) = format_console_line(table, fields) {
                    eprintln!("{line}");
                }
            }
            self.ndjson.log_event(table, fields);
            return;
        }
        let job = EmitJob::Event {
            table: table.to_string(),
            fields: fields.clone(),
        };
        try_enqueue(&self.tx, job);
    }
}

fn try_enqueue(tx: &SyncSender<EmitJob>, job: EmitJob) {
    match tx.try_send(job) {
        Ok(()) => {}
        Err(TrySendError::Full(_)) => {
            log::warn!("[spectra:async_writer] emit queue full; dropping telemetry job");
        }
        Err(TrySendError::Disconnected(_)) => {
            log::warn!("[spectra:async_writer] emit queue disconnected");
        }
    }
}

fn writer_loop(rx: mpsc::Receiver<EmitJob>, ndjson: Arc<NdjsonFileSink>) {
    while let Ok(job) = rx.recv() {
        match job {
            EmitJob::Counter {
                name,
                labels,
                delta,
            } => {
                let label_refs: Vec<(&str, &str)> = labels
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.as_str()))
                    .collect();
                ndjson.record_counter(&name, &label_refs, delta);
            }
            EmitJob::Gauge {
                name,
                labels,
                value,
            } => {
                let label_refs: Vec<(&str, &str)> = labels
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.as_str()))
                    .collect();
                ndjson.record_gauge(&name, &label_refs, value);
            }
            EmitJob::Event { table, fields } => {
                if console_mirror_enabled() {
                    if let Some(line) = format_console_line(&table, &fields) {
                        eprintln!("{line}");
                    }
                }
                ndjson.log_event(&table, &fields);
            }
        }
    }
}

/// Build a single `[spectra:console]` line when schema fields are console-safe.
pub fn format_console_line(table: &str, fields: &Value) -> Option<String> {
    let parts = mirror_safe_console_parts(table, fields)?;
    Some(format!("[spectra:console] {table} {}", parts.join(" ")))
}

fn mirror_safe_console_parts(table: &str, fields: &Value) -> Option<Vec<String>> {
    let meta = SchemaRegistry::global().get_schema(table)?;
    let obj = fields.as_object()?;

    let mut parts = Vec::new();
    for field in &meta.fields {
        if !field.classification.safe_for_console {
            continue;
        }
        if let Some(v) = obj.get(&field.name) {
            if let Some(s) = value_as_str(v) {
                if !s.is_empty() {
                    parts.push(format!("{}={s}", field.name));
                }
            }
        }
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts)
    }
}

fn value_as_str(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}
