//! Request and worker buffer scopes.

use std::cell::RefCell;
use std::future::Future;

use super::drain::drain;
use super::push::take_records;
use super::state::{is_active, job_enabled, request_enabled, EMIT_BUFFER};
use super::types::BufferedEmit;

/// Web entry point: buffer everything, returning the future output plus the buffered
/// records for the caller to flush off the response path. Runs unbuffered (empty `Vec`)
/// when the gate is off or already inside a scope.
///
/// **Not recommended for Spectra’s primary web path.** Undrained records are dropped if
/// the handler panics or returns without [`drain`](super::drain). Prefer
/// `try_record_*_now` / generated helpers so emits enqueue to L2 persist immediately and
/// survive mid-request failure.
pub async fn request_scope<F, T>(fut: F) -> (T, Vec<BufferedEmit>)
where
    F: Future<Output = T>,
{
    request_scope_gated(request_enabled(), fut).await
}

/// Worker entry point: buffer emits for the duration of a job, then drain them
/// (blocking) before returning, so the worker does not pick up the next job until
/// telemetry is replayed through the sink. No-op when the gate is off or nested.
pub async fn worker_scope<F, T>(fut: F) -> T
where
    F: Future<Output = T>,
{
    worker_scope_gated(job_enabled(), fut).await
}

pub(crate) async fn request_scope_gated<F, T>(enabled: bool, fut: F) -> (T, Vec<BufferedEmit>)
where
    F: Future<Output = T>,
{
    if !enabled || is_active() {
        return (fut.await, Vec::new());
    }
    EMIT_BUFFER
        .scope(RefCell::new(Vec::new()), async move {
            let out = fut.await;
            (out, take_records())
        })
        .await
}

pub(crate) async fn worker_scope_gated<F, T>(enabled: bool, fut: F) -> T
where
    F: Future<Output = T>,
{
    if !enabled || is_active() {
        return fut.await;
    }
    EMIT_BUFFER
        .scope(RefCell::new(Vec::new()), async move {
            let out = fut.await;
            drain(take_records());
            out
        })
        .await
}
