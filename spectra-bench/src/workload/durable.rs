//! Durable Spectra→DW firehose (adapter-direct / subscriber-shaped).
//!
//! Each successful awaited backend write counts toward durable ops/s. Used by BM-SW5/SW6.
//! BM-SW7 uses L2 `*_now` enqueue + [`Spectra::flush_persist`] instead.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use chrono::Utc;
use serde_json::json;
use spectra::Spectra;
use spectra_core::{
    try_record_counter_now, EventStorageBackend, MetricsStorageBackend, RootcauseSnapshot,
};
use tokio::task::JoinSet;

use super::firehose::{diff_rootcause, FirehoseResult};

pub const DURABLE_COUNTER_NAME: &str = "bench.durable.counter";
pub const DURABLE_EVENT_TABLE: &str = "bench_durable_event";
pub const BATCHED_DURABLE_COUNTER_NAME: &str = "bench.batched.counter";

/// Adapter-direct counter firehose: awaited `record_counter` successes = durable ops.
pub async fn run_durable_counter_firehose(
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
            let labels = json!({ "shard": shard.to_string(), "path": "subscriber-sim" });
            let mut n = 0u64;
            while !stop.load(Ordering::Relaxed) {
                let ts = Utc::now();
                match metrics
                    .record_counter(DURABLE_COUNTER_NAME, &labels, 1, ts)
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
    summarize_durable(
        total.load(Ordering::Relaxed),
        errors.load(Ordering::Relaxed),
        elapsed,
        before,
    )
}

/// Adapter-direct event firehose: awaited `append_row` successes = durable ops.
pub async fn run_durable_event_firehose(
    events: Arc<dyn EventStorageBackend>,
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
    let payload = json!({
        "msg": "bench durable event — Spectra→DW subscriber-sim path (~256B pad for parity)",
        "path": "subscriber-sim"
    });

    for worker in 0..concurrency {
        let total = Arc::clone(&total);
        let errors = Arc::clone(&errors);
        let stop = Arc::clone(&stop);
        let payload = payload.clone();
        let events = Arc::clone(&events);
        tasks.spawn(async move {
            let mut seq = 0u64;
            while !stop.load(Ordering::Relaxed) {
                let mut fields = payload.clone();
                if let Some(obj) = fields.as_object_mut() {
                    obj.insert("seq".into(), json!(seq));
                    obj.insert("worker".into(), json!(worker));
                }
                match events
                    .append_row(DURABLE_EVENT_TABLE, &fields, Utc::now(), None)
                    .await
                {
                    Ok(()) => {
                        total.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(_) => {
                        errors.fetch_add(1, Ordering::Relaxed);
                    }
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

    let elapsed = started.elapsed();
    summarize_durable(
        total.load(Ordering::Relaxed),
        errors.load(Ordering::Relaxed),
        elapsed,
        before,
    )
}

/// L2 batched durable counter: `try_record_counter_now` → persist queue → `flush_persist`.
///
/// Durable ops/s uses **rows actually written** (rootcause `storage_writes_metrics`) over
/// firehose **plus** flush wall — not raw enqueue attempts (which can vastly outrun DW).
pub async fn run_batched_durable_counter_firehose(
    spectra: &Spectra,
    concurrency: u32,
    duration: Duration,
) -> Result<FirehoseResult> {
    std::env::set_var("COUNTER_ROOTCAUSE", "1");
    let before = RootcauseSnapshot::capture();
    let enqueued = Arc::new(AtomicU64::new(0));
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let started = Instant::now();
    let mut tasks = JoinSet::new();

    for worker in 0..concurrency {
        let enqueued = Arc::clone(&enqueued);
        let stop = Arc::clone(&stop);
        tasks.spawn(async move {
            let shard = (worker % 8).to_string();
            let labels = [("shard", shard.as_str()), ("path", "l2-batch")];
            let mut n = 0u64;
            while !stop.load(Ordering::Relaxed) {
                try_record_counter_now(BATCHED_DURABLE_COUNTER_NAME, &labels, 1);
                enqueued.fetch_add(1, Ordering::Relaxed);
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

    spectra
        .flush_persist()
        .await
        .map_err(|e| anyhow::anyhow!("flush_persist: {e}"))?;

    let elapsed = started.elapsed();
    let after = RootcauseSnapshot::capture();
    let durable_rows = after
        .storage_writes_metrics
        .saturating_sub(before.storage_writes_metrics);
    let drops = after
        .persist_queue_drops
        .saturating_sub(before.persist_queue_drops);
    let _ = enqueued.load(Ordering::Relaxed);

    // Treat queue drops as soft errors for the durable path (enqueue outran capacity).
    summarize_durable(durable_rows, drops, elapsed, before)
}

fn summarize_durable(
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
