use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use chrono::Utc;
use serde_json::json;
use spectra_core::{
    try_log_event_now, try_record_counter_now, EventStorageBackend, MetricsStorageBackend,
    RootcauseSnapshot,
};
use tokio::task::JoinSet;

#[derive(Debug, Clone)]
pub struct FirehoseResult {
    pub achieved_ops_per_sec: f64,
    pub total_ops: u64,
    pub error_count: u64,
    pub error_rate: f64,
    pub rootcause: RootcauseSnapshot,
}

pub async fn run_adapter_counter_firehose(
    metrics: Arc<dyn MetricsStorageBackend>,
    concurrency: u32,
    duration: Duration,
) -> Result<FirehoseResult> {
    std::env::set_var("COUNTER_ROOTCAUSE", "1");
    let before = RootcauseSnapshot::capture();
    let total = Arc::new(AtomicU64::new(0));
    let errors = Arc::new(AtomicU64::new(0));
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let started = Instant::now();
    let mut tasks = JoinSet::new();

    for worker in 0..concurrency {
        let metrics = Arc::clone(&metrics);
        let total = Arc::clone(&total);
        let errors = Arc::clone(&errors);
        let stop = Arc::clone(&stop);
        tasks.spawn(async move {
            let shard = worker % 8;
            let labels = json!({ "shard": shard.to_string() });
            let mut n = 0u64;
            while !stop.load(Ordering::Relaxed) {
                let ts = Utc::now();
                match metrics
                    .record_counter("bench.adapter.counter", &labels, 1, ts)
                    .await
                {
                    Ok(()) => {
                        total.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(_) => {
                        errors.fetch_add(1, Ordering::Relaxed);
                    }
                }
                n = n.wrapping_add(1);
                if n % 256 == 0 {
                    tokio::time::sleep(Duration::from_micros(50)).await;
                }
            }
        });
    }

    tokio::time::sleep(duration).await;
    stop.store(true, Ordering::Relaxed);
    while tasks.join_next().await.is_some() {}

    let elapsed = started.elapsed();
    summarize(
        total.load(Ordering::Relaxed),
        errors.load(Ordering::Relaxed),
        elapsed,
        before,
    )
}

pub async fn run_full_stack_counter_firehose(
    concurrency: u32,
    duration: Duration,
    drain_ms: u64,
) -> Result<FirehoseResult> {
    std::env::set_var("COUNTER_ROOTCAUSE", "1");
    let before = RootcauseSnapshot::capture();
    let total = Arc::new(AtomicU64::new(0));
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let started = Instant::now();
    let mut tasks = JoinSet::new();

    for worker in 0..concurrency {
        let total = Arc::clone(&total);
        let stop = Arc::clone(&stop);
        tasks.spawn(async move {
            let shard = (worker % 8).to_string();
            let labels = [("shard", shard.as_str())];
            let mut n = 0u64;
            while !stop.load(Ordering::Relaxed) {
                try_record_counter_now("bench.fullstack.counter", &labels, 1);
                total.fetch_add(1, Ordering::Relaxed);
                n = n.wrapping_add(1);
                // Brief sleep forces OS scheduling so sshd stays responsive on small hosts.
                if n % 256 == 0 {
                    tokio::time::sleep(Duration::from_micros(50)).await;
                }
            }
        });
    }

    tokio::time::sleep(duration).await;
    stop.store(true, Ordering::Relaxed);
    while tasks.join_next().await.is_some() {}

    // Allow async persist queue to drain (longer for remote-ingest).
    tokio::time::sleep(Duration::from_millis(drain_ms)).await;

    let elapsed = started.elapsed();
    summarize(total.load(Ordering::Relaxed), 0, elapsed, before)
}

pub async fn run_event_firehose(
    concurrency: u32,
    duration: Duration,
    adapter_direct: bool,
    events: Option<Arc<dyn EventStorageBackend>>,
    drain_ms: u64,
) -> Result<FirehoseResult> {
    std::env::set_var("COUNTER_ROOTCAUSE", "1");
    let before = RootcauseSnapshot::capture();
    let total = Arc::new(AtomicU64::new(0));
    let errors = Arc::new(AtomicU64::new(0));
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let started = Instant::now();
    let mut tasks = JoinSet::new();
    let payload = json!({
        "msg": "bench event payload for spectra capacity study — padded to ~256 bytes total size in JSON field content for comparability across Spectra capacity studies"
    });

    for worker in 0..concurrency {
        let total = Arc::clone(&total);
        let errors = Arc::clone(&errors);
        let stop = Arc::clone(&stop);
        let payload = payload.clone();
        let events = events.as_ref().map(Arc::clone);
        tasks.spawn(async move {
            let mut seq = 0u64;
            while !stop.load(Ordering::Relaxed) {
                if adapter_direct {
                    if let Some(events) = &events {
                        let mut fields = payload.clone();
                        if let Some(obj) = fields.as_object_mut() {
                            obj.insert("seq".into(), json!(seq));
                            obj.insert("worker".into(), json!(worker));
                        }
                        match events
                            .append_row("bench_event", &fields, Utc::now(), None)
                            .await
                        {
                            Ok(()) => {
                                total.fetch_add(1, Ordering::Relaxed);
                            }
                            Err(_) => {
                                errors.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                    }
                } else {
                    let mut fields = payload.clone();
                    if let Some(obj) = fields.as_object_mut() {
                        obj.insert("seq".into(), json!(seq));
                        obj.insert("worker".into(), json!(worker));
                    }
                    try_log_event_now("bench_event", &fields);
                    total.fetch_add(1, Ordering::Relaxed);
                }
                seq = seq.wrapping_add(1);
                if seq % 256 == 0 {
                    tokio::time::sleep(Duration::from_micros(50)).await;
                }
            }
        });
    }

    tokio::time::sleep(duration).await;
    stop.store(true, Ordering::Relaxed);
    while tasks.join_next().await.is_some() {}

    if !adapter_direct {
        tokio::time::sleep(Duration::from_millis(drain_ms)).await;
    }

    let elapsed = started.elapsed();
    summarize(
        total.load(Ordering::Relaxed),
        errors.load(Ordering::Relaxed),
        elapsed,
        before,
    )
}

fn summarize(
    total_ops: u64,
    error_count: u64,
    elapsed: Duration,
    before: RootcauseSnapshot,
) -> Result<FirehoseResult> {
    let after = RootcauseSnapshot::capture();
    let attempts = total_ops + error_count;
    let error_rate = if attempts == 0 {
        0.0
    } else {
        error_count as f64 / attempts as f64
    };
    let secs = elapsed.as_secs_f64().max(0.001);
    Ok(FirehoseResult {
        achieved_ops_per_sec: total_ops as f64 / secs,
        total_ops,
        error_count,
        error_rate,
        rootcause: diff_rootcause(before, after),
    })
}

pub(crate) fn diff_rootcause(
    before: RootcauseSnapshot,
    after: RootcauseSnapshot,
) -> RootcauseSnapshot {
    RootcauseSnapshot {
        emits_counter: after.emits_counter.saturating_sub(before.emits_counter),
        emits_gauge: after.emits_gauge.saturating_sub(before.emits_gauge),
        emits_event: after.emits_event.saturating_sub(before.emits_event),
        storage_writes_metrics: after
            .storage_writes_metrics
            .saturating_sub(before.storage_writes_metrics),
        storage_writes_events: after
            .storage_writes_events
            .saturating_sub(before.storage_writes_events),
        storage_batch_flushes_metrics: after
            .storage_batch_flushes_metrics
            .saturating_sub(before.storage_batch_flushes_metrics),
        storage_batch_flushes_events: after
            .storage_batch_flushes_events
            .saturating_sub(before.storage_batch_flushes_events),
        storage_wall_ms: (after.storage_wall_ms - before.storage_wall_ms).max(0.0),
        ndjson_appends: after.ndjson_appends.saturating_sub(before.ndjson_appends),
        ndjson_wall_ms: (after.ndjson_wall_ms - before.ndjson_wall_ms).max(0.0),
        inline_wall_ms: (after.inline_wall_ms - before.inline_wall_ms).max(0.0),
        buffer_pushes: after.buffer_pushes.saturating_sub(before.buffer_pushes),
        buffer_drains: after.buffer_drains.saturating_sub(before.buffer_drains),
        drain_wall_ms: (after.drain_wall_ms - before.drain_wall_ms).max(0.0),
        gate_drops: after.gate_drops.saturating_sub(before.gate_drops),
        aggregate_coalesced: after
            .aggregate_coalesced
            .saturating_sub(before.aggregate_coalesced),
        persist_queue_drops: after
            .persist_queue_drops
            .saturating_sub(before.persist_queue_drops),
    }
}
