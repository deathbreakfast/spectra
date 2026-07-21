//! Builder-facing persist queue and batch settings.

use std::time::Duration;

/// Behavior when the L2 persist queue is full.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PersistOverflow {
    /// Non-blocking `try_send`; drop the job and increment `persist_queue_drops` (default).
    #[default]
    Drop,
    /// Apply backpressure until the job is enqueued (or the queue closes).
    ///
    /// **Runtime contract:** true blocking backpressure requires a **multi-thread** Tokio
    /// runtime (`rt-multi-thread`). On a multi-thread worker the sink uses
    /// [`tokio::task::block_in_place`] so other tasks can run while waiting.
    ///
    /// Outside any Tokio runtime, uses [`tokio::sync::mpsc::Sender::blocking_send`].
    ///
    /// On a **current-thread** Tokio runtime, `block_in_place` is unsafe/unavailable, so
    /// Spectra degrades to non-blocking `try_send` (same drop + `persist_queue_drops`
    /// accounting as [`Self::Drop`]) and logs a warning. Prefer [`Self::Drop`] explicitly
    /// on current-thread runtimes, or use a multi-thread runtime when loss is unacceptable.
    Block,
}

/// Queue and batch settings for [`crate::StoragePersistSink`].
///
/// Set via [`crate::SpectraBuilder::persist`]. Defaults match the previous in-process
/// behavior (batching on, overflow drop). Callers raise `batch_max` for high-throughput DW ingest.
#[derive(Debug, Clone)]
pub struct PersistConfig {
    /// Max queued persist jobs before overflow policy applies. Default `8192`.
    pub queue_max: usize,
    /// Max jobs collected into one batch flush. Default `32`.
    pub batch_max: usize,
    /// Extra wait when the first collected job is alone, to coalesce. Default `5ms`.
    pub batch_wait: Duration,
    /// When `true`, use `record_metrics_batch` / `append_rows_batch`. Default `true`.
    pub batch_enabled: bool,
    /// Overflow policy when `queue_max` is reached. Default [`PersistOverflow::Drop`].
    pub overflow: PersistOverflow,
}

impl Default for PersistConfig {
    fn default() -> Self {
        Self {
            queue_max: 8192,
            batch_max: 32,
            batch_wait: Duration::from_millis(5),
            batch_enabled: true,
            overflow: PersistOverflow::Drop,
        }
    }
}

impl PersistConfig {
    pub(crate) fn normalized(self) -> Self {
        Self {
            queue_max: self.queue_max.max(1),
            batch_max: self.batch_max.max(1),
            batch_wait: self.batch_wait,
            batch_enabled: self.batch_enabled,
            overflow: self.overflow,
        }
    }
}
