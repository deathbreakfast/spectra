//! Task-local Spectra emit buffer.
//!
//! Batches `try_record_*` / `try_log_event` emits during a unit of work and replays
//! them through the installed [`SpectraSink`](crate::SpectraSink) off the hot path.
//! Two entry points share one task-local buffer and differ only by drain timing:
//!
//! - [`request_scope`] (web): buffer everything, hand the records back so the caller
//!   drains them on a spawned task after the response is sent.
//! - [`worker_scope`] (background workers): buffer everything, then drain
//!   inline (blocking post-processing) before the worker picks up the next job.
//!
//! ## Prefer `*_now` → L2 for web and scripts
//!
//! For Spectra’s primary web and Write Now story, **do not rely on** [`request_scope`].
//! Prefer `try_record_*_now` / generated helpers, which enqueue into the L2
//! async persist sink immediately. That way emits already queued survive mid-request
//! failure or panic.
//!
//! **Warning:** [`request_scope`] discards undrained buffered emits if the handler panics
//! or returns without calling [`drain`]. That is incompatible with “log failures too.”
//! Keep `request_scope` only for niche hosts that intentionally want discard-until-drain.
//!
//! ## Defaults (embedded profile)
//!
//! `SPECTRA_REQUEST_BUFFER`, `SPECTRA_JOB_BUFFER`, and `SPECTRA_COUNTER_AGGREGATE` default
//! **on** unless explicitly set to `0`/`false`/`no`.
//!
//! Each record captures its wall-clock timestamp at emit time (push); [`drain`] sets a
//! thread-local override ([`current_emit_ts`]) so downstream writers stamp the emit time
//! rather than the (later) flush time.

mod drain;
mod push;
mod scopes;
mod state;
mod types;

#[cfg(test)]
mod tests;

pub use drain::{drain, with_emit_ts};
pub use scopes::{request_scope, worker_scope};
pub use state::{current_emit_ts, is_active, is_replaying, job_enabled, request_enabled};
pub use types::BufferedEmit;

pub(crate) use push::{push_counter, push_event, push_gauge};
