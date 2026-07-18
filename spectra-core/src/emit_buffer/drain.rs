//! Replay buffered records through the facade.

use std::time::Instant;

use chrono::{DateTime, Utc};

use super::push::borrow;
use super::state::{aggregate_enabled, EMIT_TS, REPLAYING};
use super::types::BufferedEmit;

/// Replay buffered records through the facade, stamping each with its captured emit
/// timestamp. Safe to call while a scope is still bound: the replay guard makes the
/// facade dispatch (not re-buffer) during the loop.
pub fn drain(records: Vec<BufferedEmit>) {
    if records.is_empty() {
        return;
    }
    let records = if aggregate_enabled() {
        let (collapsed, coalesced) = crate::aggregate::accumulate_counters(records);
        crate::rootcause::record_aggregate_coalesced(coalesced);
        collapsed
    } else {
        records
    };
    let start = crate::rootcause::enabled().then(Instant::now);
    let _replaying = ReplayGuard::enter();
    for rec in records {
        match rec {
            BufferedEmit::Counter {
                name,
                labels,
                delta,
                ts,
            } => with_emit_ts(ts, || {
                crate::try_record_counter(&name, &borrow(&labels), delta)
            }),
            BufferedEmit::Gauge {
                name,
                labels,
                value,
                ts,
            } => with_emit_ts(ts, || {
                crate::try_record_gauge(&name, &borrow(&labels), value)
            }),
            BufferedEmit::Event { table, fields, ts } => {
                with_emit_ts(ts, || crate::try_log_event(&table, &fields))
            }
        }
    }
    if let Some(start) = start {
        crate::rootcause::record_buffer_drain(start.elapsed());
    }
}

/// Run `f` with [`super::current_emit_ts`] returning `ts` for the duration of the call.
///
/// Used by buffer drain and by [`crate::try_record_counter_at`] / related `_at` façade APIs
/// so sinks that stamp via `current_emit_ts()` preserve caller-supplied emit time.
pub fn with_emit_ts<R>(ts: DateTime<Utc>, f: impl FnOnce() -> R) -> R {
    let _ts = EmitTsGuard::set(ts);
    f()
}

struct ReplayGuard;

impl ReplayGuard {
    fn enter() -> Self {
        REPLAYING.with(|r| r.set(true));
        ReplayGuard
    }
}

impl Drop for ReplayGuard {
    fn drop(&mut self) {
        REPLAYING.with(|r| r.set(false));
    }
}

struct EmitTsGuard;

impl EmitTsGuard {
    fn set(ts: DateTime<Utc>) -> Self {
        EMIT_TS.with(|c| c.set(Some(ts)));
        EmitTsGuard
    }
}

impl Drop for EmitTsGuard {
    fn drop(&mut self) {
        EMIT_TS.with(|c| c.set(None));
    }
}
