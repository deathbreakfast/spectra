//! Push helpers and label conversion for the active buffer.

use chrono::Utc;

use super::state::{EMIT_BUFFER, REPLAYING};
use super::types::BufferedEmit;

pub(super) fn take_records() -> Vec<BufferedEmit> {
    EMIT_BUFFER.with(|s| std::mem::take(&mut *s.borrow_mut()))
}

pub(crate) fn push_counter(name: &str, labels: &[(&str, &str)], delta: i64) -> bool {
    if REPLAYING.with(|r| r.get()) {
        return false;
    }
    push(BufferedEmit::Counter {
        name: name.into(),
        labels: own(labels),
        delta,
        ts: Utc::now(),
    })
}

pub(crate) fn push_gauge(name: &str, labels: &[(&str, &str)], value: f64) -> bool {
    if REPLAYING.with(|r| r.get()) {
        return false;
    }
    push(BufferedEmit::Gauge {
        name: name.into(),
        labels: own(labels),
        value,
        ts: Utc::now(),
    })
}

pub(crate) fn push_event(table: &str, fields: &serde_json::Value) -> bool {
    if REPLAYING.with(|r| r.get()) {
        return false;
    }
    push(BufferedEmit::Event {
        table: table.into(),
        fields: fields.clone(),
        ts: Utc::now(),
    })
}

/// Append one record to the active buffer; returns `true` when consumed.
fn push(emit: BufferedEmit) -> bool {
    match EMIT_BUFFER.try_with(|s| {
        s.borrow_mut().push(emit);
        crate::rootcause::record_buffer_push();
    }) {
        Ok(()) => true,
        Err(_) => false,
    }
}

pub(super) fn own(labels: &[(&str, &str)]) -> Vec<(String, String)> {
    labels
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect()
}

pub(super) fn borrow(labels: &[(String, String)]) -> Vec<(&str, &str)> {
    labels
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect()
}
