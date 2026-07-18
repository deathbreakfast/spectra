//! `COUNTER_ROOTCAUSE`-gated process-global counters for Spectra write-amplification forensics.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

static ENABLED: OnceLock<bool> = OnceLock::new();

static EMITS_COUNTER: AtomicU64 = AtomicU64::new(0);
static EMITS_GAUGE: AtomicU64 = AtomicU64::new(0);
static EMITS_EVENT: AtomicU64 = AtomicU64::new(0);

static STORAGE_WRITES_METRICS: AtomicU64 = AtomicU64::new(0);
static STORAGE_WRITES_EVENTS: AtomicU64 = AtomicU64::new(0);
static STORAGE_BATCH_FLUSHES_METRICS: AtomicU64 = AtomicU64::new(0);
static STORAGE_BATCH_FLUSHES_EVENTS: AtomicU64 = AtomicU64::new(0);
static STORAGE_WALL_NS: AtomicU64 = AtomicU64::new(0);

static NDJSON_APPENDS: AtomicU64 = AtomicU64::new(0);
static NDJSON_WALL_NS: AtomicU64 = AtomicU64::new(0);

static INLINE_WALL_NS: AtomicU64 = AtomicU64::new(0);

static BUFFER_PUSHES: AtomicU64 = AtomicU64::new(0);
static BUFFER_DRAINS: AtomicU64 = AtomicU64::new(0);
static DRAIN_WALL_NS: AtomicU64 = AtomicU64::new(0);

static GATE_DROPS: AtomicU64 = AtomicU64::new(0);
static AGGREGATE_COALESCED: AtomicU64 = AtomicU64::new(0);
static PERSIST_QUEUE_DROPS: AtomicU64 = AtomicU64::new(0);

/// True when `COUNTER_ROOTCAUSE=1` (or `true`/`yes`).
pub fn enabled() -> bool {
    *ENABLED.get_or_init(|| {
        matches!(
            std::env::var("COUNTER_ROOTCAUSE").as_deref(),
            Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes") | Ok("YES")
        )
    })
}

/// Snapshot of all process-global rootcause counters.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RootcauseSnapshot {
    /// Counter emits observed.
    pub emits_counter: u64,
    /// Gauge emits observed.
    pub emits_gauge: u64,
    /// Event emits observed.
    pub emits_event: u64,
    /// Individual metrics storage write operations.
    pub storage_writes_metrics: u64,
    /// Individual event storage write operations.
    pub storage_writes_events: u64,
    /// Batched metrics INSERT flushes.
    pub storage_batch_flushes_metrics: u64,
    /// Batched event INSERT flushes.
    pub storage_batch_flushes_events: u64,
    /// Cumulative storage wall time in milliseconds.
    pub storage_wall_ms: f64,
    /// NDJSON append operations.
    pub ndjson_appends: u64,
    /// Cumulative NDJSON wall time in milliseconds.
    pub ndjson_wall_ms: f64,
    /// Cumulative inline sink dispatch wall time in milliseconds.
    pub inline_wall_ms: f64,
    /// Emits pushed into request/job buffers.
    pub buffer_pushes: u64,
    /// Buffer drain operations.
    pub buffer_drains: u64,
    /// Cumulative buffer drain wall time in milliseconds.
    pub drain_wall_ms: f64,
    /// Emits dropped by the level/sample/coalesce gate.
    pub gate_drops: u64,
    /// Counter emits merged during drain-time aggregation.
    pub aggregate_coalesced: u64,
    /// Persist jobs dropped because the bounded queue was full.
    pub persist_queue_drops: u64,
}

impl RootcauseSnapshot {
    /// Captures the current values of all process-global counters.
    pub fn capture() -> Self {
        Self {
            emits_counter: EMITS_COUNTER.load(Ordering::Relaxed),
            emits_gauge: EMITS_GAUGE.load(Ordering::Relaxed),
            emits_event: EMITS_EVENT.load(Ordering::Relaxed),
            storage_writes_metrics: STORAGE_WRITES_METRICS.load(Ordering::Relaxed),
            storage_writes_events: STORAGE_WRITES_EVENTS.load(Ordering::Relaxed),
            storage_batch_flushes_metrics: STORAGE_BATCH_FLUSHES_METRICS.load(Ordering::Relaxed),
            storage_batch_flushes_events: STORAGE_BATCH_FLUSHES_EVENTS.load(Ordering::Relaxed),
            storage_wall_ms: ns_to_ms(STORAGE_WALL_NS.load(Ordering::Relaxed)),
            ndjson_appends: NDJSON_APPENDS.load(Ordering::Relaxed),
            ndjson_wall_ms: ns_to_ms(NDJSON_WALL_NS.load(Ordering::Relaxed)),
            inline_wall_ms: ns_to_ms(INLINE_WALL_NS.load(Ordering::Relaxed)),
            buffer_pushes: BUFFER_PUSHES.load(Ordering::Relaxed),
            buffer_drains: BUFFER_DRAINS.load(Ordering::Relaxed),
            drain_wall_ms: ns_to_ms(DRAIN_WALL_NS.load(Ordering::Relaxed)),
            gate_drops: GATE_DROPS.load(Ordering::Relaxed),
            aggregate_coalesced: AGGREGATE_COALESCED.load(Ordering::Relaxed),
            persist_queue_drops: PERSIST_QUEUE_DROPS.load(Ordering::Relaxed),
        }
    }

    /// Returns the per-field delta between two snapshots.
    pub fn delta(before: Self, after: Self) -> Self {
        Self {
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

    /// Logs absolute counter values when rootcause tracing is enabled.
    pub fn log_per_increment(label: &str) {
        if !enabled() {
            return;
        }
        let s = Self::capture();
        log::info!(
            "[rootcause] span=spectra.per_increment label={label} \
             emits_counter={} emits_gauge={} emits_event={} \
             storage_writes_metrics={} storage_writes_events={} \
             storage_batch_flushes_metrics={} storage_batch_flushes_events={} storage_wall_ms={:.3} \
             ndjson_appends={} ndjson_wall_ms={:.3} inline_wall_ms={:.3} \
             buffer_pushes={} buffer_drains={} drain_wall_ms={:.3} aggregate_coalesced={} persist_queue_drops={}",
            s.emits_counter,
            s.emits_gauge,
            s.emits_event,
            s.storage_writes_metrics,
            s.storage_writes_events,
            s.storage_batch_flushes_metrics,
            s.storage_batch_flushes_events,
            s.storage_wall_ms,
            s.ndjson_appends,
            s.ndjson_wall_ms,
            s.inline_wall_ms,
            s.buffer_pushes,
            s.buffer_drains,
            s.drain_wall_ms,
            s.aggregate_coalesced,
            s.persist_queue_drops,
        );
    }

    /// Logs counter deltas between two snapshots when rootcause tracing is enabled.
    pub fn log_per_increment_delta(label: &str, before: Self, after: Self) {
        if !enabled() {
            return;
        }
        let d = Self::delta(before, after);
        log::info!(
            "[rootcause] span=spectra.per_increment label={label} \
             emits_counter={} emits_gauge={} emits_event={} \
             storage_writes_metrics={} storage_writes_events={} \
             storage_batch_flushes_metrics={} storage_batch_flushes_events={} storage_wall_ms={:.3} \
             ndjson_appends={} ndjson_wall_ms={:.3} inline_wall_ms={:.3} \
             buffer_pushes={} buffer_drains={} drain_wall_ms={:.3} aggregate_coalesced={} persist_queue_drops={}",
            d.emits_counter,
            d.emits_gauge,
            d.emits_event,
            d.storage_writes_metrics,
            d.storage_writes_events,
            d.storage_batch_flushes_metrics,
            d.storage_batch_flushes_events,
            d.storage_wall_ms,
            d.ndjson_appends,
            d.ndjson_wall_ms,
            d.inline_wall_ms,
            d.buffer_pushes,
            d.buffer_drains,
            d.drain_wall_ms,
            d.aggregate_coalesced,
            d.persist_queue_drops,
        );
    }
}

pub(crate) fn record_emit_counter() {
    if enabled() {
        EMITS_COUNTER.fetch_add(1, Ordering::Relaxed);
    }
}

pub(crate) fn record_emit_gauge() {
    if enabled() {
        EMITS_GAUGE.fetch_add(1, Ordering::Relaxed);
    }
}

pub(crate) fn record_emit_event() {
    if enabled() {
        EMITS_EVENT.fetch_add(1, Ordering::Relaxed);
    }
}

/// Records one metrics storage write and its wall time.
pub fn record_storage_write_metrics(duration: Duration) {
    if enabled() {
        STORAGE_WRITES_METRICS.fetch_add(1, Ordering::Relaxed);
        STORAGE_WALL_NS.fetch_add(duration.as_nanos() as u64, Ordering::Relaxed);
    }
}

/// Records one event storage write and its wall time.
pub fn record_storage_write_events(duration: Duration) {
    if enabled() {
        STORAGE_WRITES_EVENTS.fetch_add(1, Ordering::Relaxed);
        STORAGE_WALL_NS.fetch_add(duration.as_nanos() as u64, Ordering::Relaxed);
    }
}

/// One batched metrics INSERT: counts `row_count` rows and one statement flush.
pub fn record_storage_batch_write_metrics(duration: Duration, row_count: u64) {
    if enabled() && row_count > 0 {
        STORAGE_WRITES_METRICS.fetch_add(row_count, Ordering::Relaxed);
        STORAGE_BATCH_FLUSHES_METRICS.fetch_add(1, Ordering::Relaxed);
        STORAGE_WALL_NS.fetch_add(duration.as_nanos() as u64, Ordering::Relaxed);
    }
}

/// One batched events INSERT: counts `row_count` rows and one statement flush.
pub fn record_storage_batch_write_events(duration: Duration, row_count: u64) {
    if enabled() && row_count > 0 {
        STORAGE_WRITES_EVENTS.fetch_add(row_count, Ordering::Relaxed);
        STORAGE_BATCH_FLUSHES_EVENTS.fetch_add(1, Ordering::Relaxed);
        STORAGE_WALL_NS.fetch_add(duration.as_nanos() as u64, Ordering::Relaxed);
    }
}

/// Records one NDJSON append and its wall time.
pub fn record_ndjson_append(duration: Duration) {
    if enabled() {
        NDJSON_APPENDS.fetch_add(1, Ordering::Relaxed);
        NDJSON_WALL_NS.fetch_add(duration.as_nanos() as u64, Ordering::Relaxed);
    }
}

pub(crate) fn record_inline_dispatch(duration: Duration) {
    if enabled() {
        INLINE_WALL_NS.fetch_add(duration.as_nanos() as u64, Ordering::Relaxed);
    }
}

/// One emit appended to a request/job emit buffer instead of dispatched inline.
pub(crate) fn record_buffer_push() {
    if enabled() {
        BUFFER_PUSHES.fetch_add(1, Ordering::Relaxed);
    }
}

/// One buffer drain (one batch replayed through the sink), with its wall time.
pub(crate) fn record_buffer_drain(duration: Duration) {
    if enabled() {
        BUFFER_DRAINS.fetch_add(1, Ordering::Relaxed);
        DRAIN_WALL_NS.fetch_add(duration.as_nanos() as u64, Ordering::Relaxed);
    }
}

/// One emit dropped by the level/sample/coalesce gate.
pub(crate) fn record_gate_drop() {
    if enabled() {
        GATE_DROPS.fetch_add(1, Ordering::Relaxed);
    }
}

/// Counter emits merged into an existing key during drain-time aggregation.
pub(crate) fn record_aggregate_coalesced(n: u64) {
    if enabled() && n > 0 {
        AGGREGATE_COALESCED.fetch_add(n, Ordering::Relaxed);
    }
}

/// Persist job dropped because the bounded queue was full.
pub fn record_persist_queue_drop() {
    PERSIST_QUEUE_DROPS.fetch_add(1, Ordering::Relaxed);
}

/// Total persist jobs dropped because the bounded queue was full.
pub fn persist_queue_drop_count() -> u64 {
    PERSIST_QUEUE_DROPS.load(Ordering::Relaxed)
}

/// Returns elapsed milliseconds since `start`.
pub fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

fn ns_to_ms(ns: u64) -> f64 {
    ns as f64 / 1_000_000.0
}
