//! Task-local buffer state and environment flags.

use std::cell::{Cell, RefCell};

use chrono::{DateTime, Utc};

use super::types::BufferedEmit;

tokio::task_local! {
    pub(super) static EMIT_BUFFER: RefCell<Vec<BufferedEmit>>;
}

thread_local! {
    /// Flush-time emit-timestamp override. Read synchronously by downstream writers
    /// (NDJSON sink, host_sink routing) during replay; `None` on the normal
    /// (non-buffered) path so they fall back to `Utc::now()`.
    pub(super) static EMIT_TS: Cell<Option<DateTime<Utc>>> = const { Cell::new(None) };

    /// Set while [`super::drain::drain`] replays records so the facade dispatches replayed emits
    /// instead of re-buffering them (a drain may run while the scope is still bound).
    pub(super) static REPLAYING: Cell<bool> = const { Cell::new(false) };
}

/// Env flag default-on unless explicitly disabled (`0`/`false`/`no`).
pub(super) fn env_flag_default_on(key: &str) -> bool {
    match std::env::var(key).as_deref() {
        Ok("0") | Ok("false") | Ok("FALSE") | Ok("no") | Ok("NO") => false,
        _ => true,
    }
}

/// `SPECTRA_REQUEST_BUFFER` (1/true/yes). Default **on** for embedded profile; set `0` to disable.
pub fn request_enabled() -> bool {
    env_flag_default_on("SPECTRA_REQUEST_BUFFER")
}

/// `SPECTRA_JOB_BUFFER` (1/true/yes). Default **on** for worker scopes; set `0` to disable.
pub fn job_enabled() -> bool {
    env_flag_default_on("SPECTRA_JOB_BUFFER")
}

/// `SPECTRA_COUNTER_AGGREGATE` (1/true/yes). Default **on** when buffering is active; set `0` to disable.
pub fn aggregate_enabled() -> bool {
    env_flag_default_on("SPECTRA_COUNTER_AGGREGATE")
}

/// Emit timestamp to stamp right now: the flush override if set, else wall-clock now.
pub fn current_emit_ts() -> chrono::DateTime<Utc> {
    EMIT_TS.with(|c| c.get()).unwrap_or_else(Utc::now)
}

/// True when the current async task is inside an active buffer scope.
pub fn is_active() -> bool {
    EMIT_BUFFER.try_with(|_| ()).is_ok()
}

/// True while [`super::drain::drain`] is replaying buffered records (gate bypass during replay).
pub fn is_replaying() -> bool {
    REPLAYING.with(|r| r.get())
}
