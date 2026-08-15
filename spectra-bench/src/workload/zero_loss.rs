//! Paced zero-loss L2 durable write (BM-SW8).
//!
//! Offers a target counter rate through `try_record_counter_now`, waits on
//! [`spectra::PersistOverflow::Block`] persist, then confirms sampled query visibility.
//! BM-SW7 remains the unbounded firehose ceiling; this path finds the highest
//! rate that lands without accepted-message loss.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use spectra_core::{try_record_counter_now, LabelMatcher, RootcauseSnapshot};
use spectra_testkit::InstalledSpectra;
use tokio::task::JoinSet;

use super::firehose::{diff_rootcause, FirehoseResult};
use super::query_bench::wait_until_metric_visible;
use crate::stats::percentile;

/// Counter name used by BM-SW8 cells.
pub const ZERO_LOSS_COUNTER_NAME: &str = "bench.zero_loss.counter";
/// Path label stamped on SW8 emits.
pub const ZERO_LOSS_PATH: &str = "zero-loss";
/// Minimum durable/offered ratio that still counts as a passing cell.
pub const DURABLE_RATE_RATIO_MIN: f64 = 0.98;
/// L2 `PersistConfig.batch_max` for BM-SW8.
pub const ZERO_LOSS_BATCH_MAX: usize = 2048;
/// Default number of visibility samples per cell.
pub const DEFAULT_VISIBILITY_SAMPLES: usize = 16;

/// Why a zero-loss cell failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZeroLossFailReason {
    /// `persist_queue_drops` was non-zero.
    QueueDrop,
    /// Persist worker / adapter write failed for an accepted emit.
    AdapterError,
    /// A visibility sample exceeded the timeout (or none completed).
    VisibilityTimeout,
    /// Durable ops/s was below 98% of the offered rate.
    DurableRateBelowFloor,
}

/// Inputs for the zero-loss pass/fail rule.
#[derive(Debug, Clone, Copy)]
pub struct ZeroLossInputs {
    /// Persist queue drops observed during the cell.
    pub persist_queue_drops: u64,
    /// Adapter / persist-worker failures (accepted emit not written).
    pub adapter_errors: u64,
    /// True when any visibility sample timed out.
    pub visibility_timed_out: bool,
    /// Durable ops/s over the paced window.
    pub durable_rate: f64,
    /// Target offered ops/s.
    pub offered_rate: f64,
}

/// Pass/fail verdict for one offered-rate cell.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZeroLossVerdict {
    /// True when the cell meets every zero-loss gate.
    pub zero_loss: bool,
    /// `durable_rate / offered_rate` (0 when offered is 0).
    pub durable_rate_ratio: f64,
    /// First failing gate, if any.
    pub fail_reason: Option<ZeroLossFailReason>,
}

/// Result of one paced BM-SW8 cell, including firehose-shaped write stats.
#[derive(Debug, Clone)]
pub struct ZeroLossCellResult {
    /// Offered ops/s for this cell.
    pub offered_rate: u64,
    /// Durable ops/s over the paced window (storage writes / paced secs).
    pub durable_rate: f64,
    /// `durable_rate / offered_rate`.
    pub durable_rate_ratio: f64,
    /// Zero-loss pass flag.
    pub zero_loss: bool,
    /// First failing gate, if any.
    pub fail_reason: Option<ZeroLossFailReason>,
    /// Persist queue drops in this cell.
    pub persist_queue_drops: u64,
    /// Adapter / persist-worker shortfall.
    pub adapter_errors: u64,
    /// Sampled visibility p95 in milliseconds, when samples completed.
    pub visibility_p95_ms: Option<f64>,
    /// True when every visibility sample completed inside the timeout.
    pub visibility_confirmed: bool,
    /// Enqueue/write stats for the report `write` object.
    pub write: FirehoseResult,
}

/// Per-worker sleep between emits so `concurrency` workers sum to `offered_rate` ops/s.
#[must_use]
pub fn worker_pace_interval(offered_rate: u64, concurrency: u32) -> Duration {
    let offered = offered_rate.max(1) as f64;
    let workers = f64::from(concurrency.max(1));
    Duration::from_secs_f64(workers / offered)
}

/// Expected emit count for a paced window: `offered_rate * duration`.
#[must_use]
pub fn expected_offered_ops(offered_rate: u64, duration: Duration) -> u64 {
    (offered_rate as f64 * duration.as_secs_f64()).round() as u64
}

/// `durable_rate / offered_rate`, or 0 when offered is not positive.
#[must_use]
pub fn durable_rate_ratio(durable_rate: f64, offered_rate: f64) -> f64 {
    if offered_rate <= 0.0 {
        0.0
    } else {
        durable_rate / offered_rate
    }
}

/// True when a sample's wait exceeded the visibility timeout.
#[must_use]
pub fn visibility_sample_timed_out(elapsed_ms: f64, timeout_ms: u64) -> bool {
    elapsed_ms > timeout_ms as f64
}

/// p95 of completed visibility samples; `None` when the set is empty.
#[must_use]
pub fn visibility_p95_ms(samples_ms: &[f64]) -> Option<f64> {
    if samples_ms.is_empty() {
        return None;
    }
    let mut sorted = samples_ms.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    Some(percentile(&sorted, 0.95))
}

/// Highest offered rate among passing cells (campaign aggregation).
#[must_use]
pub fn highest_passing_offered_rate(cells: &[(u64, bool)]) -> Option<u64> {
    cells
        .iter()
        .filter(|(_, pass)| *pass)
        .map(|(rate, _)| *rate)
        .max()
}

/// Stop the offered-rate sweep after the first failing cell.
#[must_use]
pub fn should_stop_sweep(zero_loss: bool) -> bool {
    !zero_loss
}

/// Derive pass/fail: any queue drop, adapter error, visibility timeout, or
/// durable rate below 98% of offered fails the cell.
#[must_use]
pub fn derive_zero_loss_pass(inputs: ZeroLossInputs) -> ZeroLossVerdict {
    let ratio = durable_rate_ratio(inputs.durable_rate, inputs.offered_rate);
    let fail_reason = if inputs.persist_queue_drops > 0 {
        Some(ZeroLossFailReason::QueueDrop)
    } else if inputs.adapter_errors > 0 {
        Some(ZeroLossFailReason::AdapterError)
    } else if inputs.visibility_timed_out {
        Some(ZeroLossFailReason::VisibilityTimeout)
    } else if ratio < DURABLE_RATE_RATIO_MIN {
        Some(ZeroLossFailReason::DurableRateBelowFloor)
    } else {
        None
    };
    ZeroLossVerdict {
        zero_loss: fail_reason.is_none(),
        durable_rate_ratio: ratio,
        fail_reason,
    }
}

/// Pace `offered_rate` counter emits, flush persist, confirm sampled visibility.
pub async fn run_paced_zero_loss_counter(
    installed: &InstalledSpectra,
    offered_rate: u64,
    concurrency: u32,
    duration: Duration,
    visibility_timeout_ms: u64,
) -> Result<ZeroLossCellResult> {
    std::env::set_var("COUNTER_ROOTCAUSE", "1");
    let before = RootcauseSnapshot::capture();
    let accepted = Arc::new(AtomicU64::new(0));
    let sample_slots = Arc::new(AtomicU64::new(0));
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let sample_budget = visibility_sample_budget(offered_rate, duration);
    let sample_times: Arc<std::sync::Mutex<Vec<Option<Instant>>>> =
        Arc::new(std::sync::Mutex::new(vec![None; sample_budget]));
    let cell_label = offered_rate.to_string();
    let interval = worker_pace_interval(offered_rate, concurrency);
    let stride = sample_stride(offered_rate, duration, sample_budget);
    let paced_started = Instant::now();
    let mut tasks = JoinSet::new();

    for worker in 0..concurrency.max(1) {
        let accepted = Arc::clone(&accepted);
        let sample_slots = Arc::clone(&sample_slots);
        let sample_times = Arc::clone(&sample_times);
        let stop = Arc::clone(&stop);
        let cell_label = cell_label.clone();
        tasks.spawn(async move {
            let worker_s = worker.to_string();
            let mut next = Instant::now();
            let mut n = 0u64;
            while !stop.load(Ordering::Relaxed) {
                let now = Instant::now();
                if now < next {
                    tokio::time::sleep(next - now).await;
                } else {
                    // Backpressure missed a tick: do not burst-catch-up past offered rate.
                    next = now;
                }
                next += interval;

                let sample_idx = if n % stride == 0 {
                    let idx = sample_slots.fetch_add(1, Ordering::Relaxed);
                    if (idx as usize) < sample_budget {
                        Some(idx)
                    } else {
                        None
                    }
                } else {
                    None
                };

                let sample_owned = sample_idx.map(|i| i.to_string());
                let mut labels: Vec<(&str, &str)> = vec![
                    ("path", ZERO_LOSS_PATH),
                    ("cell", cell_label.as_str()),
                    ("worker", worker_s.as_str()),
                ];
                if let Some(ref sample) = sample_owned {
                    labels.push(("sample", sample.as_str()));
                }
                let emit_at = Instant::now();
                try_record_counter_now(ZERO_LOSS_COUNTER_NAME, &labels, 1);
                if let Some(idx) = sample_idx {
                    if let Ok(mut times) = sample_times.lock() {
                        if let Some(slot) = times.get_mut(idx as usize) {
                            *slot = Some(emit_at);
                        }
                    }
                }
                accepted.fetch_add(1, Ordering::Relaxed);
                n = n.wrapping_add(1);
            }
        });
    }

    tokio::time::sleep(duration).await;
    stop.store(true, Ordering::Relaxed);
    while tasks.join_next().await.is_some() {}
    let paced_elapsed = paced_started.elapsed();

    installed
        .spectra
        .flush_persist()
        .await
        .map_err(|e| anyhow::anyhow!("flush_persist: {e}"))?;

    let after = RootcauseSnapshot::capture();
    let rootcause = diff_rootcause(before, after);
    let persist_queue_drops = rootcause.persist_queue_drops;
    let durable_ops = rootcause.storage_writes_metrics;
    let enqueued = accepted.load(Ordering::Relaxed);
    let adapter_errors = enqueued.saturating_sub(durable_ops);
    let paced_secs = paced_elapsed.as_secs_f64().max(0.001);
    let durable_rate = durable_ops as f64 / paced_secs;

    let sample_count = (sample_slots.load(Ordering::Relaxed) as usize).min(sample_budget);
    let emit_times = sample_times
        .lock()
        .map(|g| g.clone())
        .unwrap_or_else(|e| e.into_inner().clone());
    let (visibility_confirmed, visibility_p95_ms, visibility_timed_out) =
        confirm_sampled_visibility(
            installed,
            &cell_label,
            &emit_times[..sample_count],
            visibility_timeout_ms,
        )
        .await?;

    let verdict = derive_zero_loss_pass(ZeroLossInputs {
        persist_queue_drops,
        adapter_errors,
        visibility_timed_out,
        durable_rate,
        offered_rate: offered_rate as f64,
    });

    let write = FirehoseResult {
        achieved_ops_per_sec: durable_rate,
        total_ops: durable_ops,
        error_count: persist_queue_drops + adapter_errors,
        error_rate: if enqueued == 0 {
            0.0
        } else {
            (persist_queue_drops + adapter_errors) as f64 / enqueued as f64
        },
        rootcause,
    };

    Ok(ZeroLossCellResult {
        offered_rate,
        durable_rate,
        durable_rate_ratio: verdict.durable_rate_ratio,
        zero_loss: verdict.zero_loss,
        fail_reason: verdict.fail_reason,
        persist_queue_drops,
        adapter_errors,
        visibility_p95_ms,
        visibility_confirmed,
        write,
    })
}

fn visibility_sample_budget(offered_rate: u64, duration: Duration) -> usize {
    let expected = expected_offered_ops(offered_rate, duration);
    (expected as usize).clamp(1, DEFAULT_VISIBILITY_SAMPLES)
}

fn sample_stride(offered_rate: u64, duration: Duration, budget: usize) -> u64 {
    let expected = expected_offered_ops(offered_rate, duration).max(1);
    (expected / budget.max(1) as u64).max(1)
}

async fn confirm_sampled_visibility(
    installed: &InstalledSpectra,
    cell: &str,
    emit_times: &[Option<Instant>],
    timeout_ms: u64,
) -> Result<(bool, Option<f64>, bool)> {
    if emit_times.is_empty() {
        return Ok((false, None, true));
    }
    let mut samples_ms = Vec::with_capacity(emit_times.len());
    for (idx, emit_at) in emit_times.iter().enumerate() {
        let Some(emit_at) = *emit_at else {
            return Ok((false, visibility_p95_ms(&samples_ms), true));
        };
        let matchers = [
            LabelMatcher {
                key: "path".into(),
                value: ZERO_LOSS_PATH.into(),
            },
            LabelMatcher {
                key: "cell".into(),
                value: cell.into(),
            },
            LabelMatcher {
                key: "sample".into(),
                value: idx.to_string(),
            },
        ];
        match wait_until_metric_visible(
            installed,
            ZERO_LOSS_COUNTER_NAME,
            &matchers,
            1,
            timeout_ms.max(1),
        )
        .await
        {
            Ok(()) => {
                let elapsed_ms = emit_at.elapsed().as_secs_f64() * 1000.0;
                if visibility_sample_timed_out(elapsed_ms, timeout_ms) {
                    return Ok((false, visibility_p95_ms(&samples_ms), true));
                }
                samples_ms.push(elapsed_ms);
            }
            Err(_) => return Ok((false, visibility_p95_ms(&samples_ms), true)),
        }
    }
    Ok((true, visibility_p95_ms(&samples_ms), false))
}

#[cfg(test)]
use spectra::Spectra;
#[cfg(test)]
use spectra_core::SpectraRouter;

/// Flush helper used by tests that install Spectra directly.
#[cfg(test)]
pub async fn flush_installed(spectra: &Spectra) -> Result<()> {
    spectra
        .flush_persist()
        .await
        .map_err(|e| anyhow::anyhow!("flush_persist: {e}"))
}

/// Query-visible point count for a SW8 cell label (tests / diagnostics).
#[cfg(test)]
pub async fn count_cell_points(router: &SpectraRouter, cell: &str) -> Result<u64> {
    let now = chrono::Utc::now();
    let points = router
        .query_metrics(spectra_core::MetricsQueryRange {
            metric_name: ZERO_LOSS_COUNTER_NAME.to_string(),
            start: now - chrono::Duration::hours(2),
            end: now + chrono::Duration::seconds(5),
            label_matchers: vec![
                LabelMatcher {
                    key: "path".into(),
                    value: ZERO_LOSS_PATH.into(),
                },
                LabelMatcher {
                    key: "cell".into(),
                    value: cell.into(),
                },
            ],
        })
        .await?;
    Ok(points.len() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use spectra::{PersistConfig, PersistOverflow};
    use spectra_testkit::{
        install_bench_matrix_with_persist, MatrixSpec, StorageAdapter, Topology,
    };

    static TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    #[test]
    fn paced_interval_splits_offered_rate_across_workers() {
        assert_eq!(worker_pace_interval(1_000, 1), Duration::from_millis(1));
        assert_eq!(worker_pace_interval(1_000, 10), Duration::from_millis(10));
        assert_eq!(expected_offered_ops(25_000, Duration::from_secs(2)), 50_000);
        assert_eq!(expected_offered_ops(100, Duration::from_millis(500)), 50);
    }

    #[test]
    fn zero_drop_pass_requires_all_gates() {
        let pass = derive_zero_loss_pass(ZeroLossInputs {
            persist_queue_drops: 0,
            adapter_errors: 0,
            visibility_timed_out: false,
            durable_rate: 98.0,
            offered_rate: 100.0,
        });
        assert!(pass.zero_loss);
        assert!(pass.fail_reason.is_none());
        assert!((pass.durable_rate_ratio - 0.98).abs() < f64::EPSILON);

        let drop_fail = derive_zero_loss_pass(ZeroLossInputs {
            persist_queue_drops: 1,
            adapter_errors: 0,
            visibility_timed_out: false,
            durable_rate: 100.0,
            offered_rate: 100.0,
        });
        assert!(!drop_fail.zero_loss);
        assert_eq!(drop_fail.fail_reason, Some(ZeroLossFailReason::QueueDrop));

        let adapter_fail = derive_zero_loss_pass(ZeroLossInputs {
            persist_queue_drops: 0,
            adapter_errors: 3,
            visibility_timed_out: false,
            durable_rate: 100.0,
            offered_rate: 100.0,
        });
        assert_eq!(
            adapter_fail.fail_reason,
            Some(ZeroLossFailReason::AdapterError)
        );

        let vis_fail = derive_zero_loss_pass(ZeroLossInputs {
            persist_queue_drops: 0,
            adapter_errors: 0,
            visibility_timed_out: true,
            durable_rate: 100.0,
            offered_rate: 100.0,
        });
        assert_eq!(
            vis_fail.fail_reason,
            Some(ZeroLossFailReason::VisibilityTimeout)
        );

        let rate_fail = derive_zero_loss_pass(ZeroLossInputs {
            persist_queue_drops: 0,
            adapter_errors: 0,
            visibility_timed_out: false,
            durable_rate: 97.0,
            offered_rate: 100.0,
        });
        assert_eq!(
            rate_fail.fail_reason,
            Some(ZeroLossFailReason::DurableRateBelowFloor)
        );
        assert!(rate_fail.durable_rate_ratio < DURABLE_RATE_RATIO_MIN);
    }

    #[test]
    fn visibility_timeout_sad_path_and_p95() {
        assert!(visibility_sample_timed_out(15_001.0, 15_000));
        assert!(!visibility_sample_timed_out(15_000.0, 15_000));
        assert!(visibility_p95_ms(&[]).is_none());
        let p95 = visibility_p95_ms(&[1.0, 2.0, 3.0, 4.0, 100.0]).unwrap();
        assert!(p95 >= 4.0);
        assert!(should_stop_sweep(false));
        assert!(!should_stop_sweep(true));
        assert_eq!(
            highest_passing_offered_rate(&[(5_000, true), (10_000, true), (15_000, false)]),
            Some(10_000)
        );
        assert_eq!(highest_passing_offered_rate(&[(5_000, false)]), None);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn block_overflow_does_not_silently_drop() {
        let _g = TEST_LOCK.lock().await;
        std::env::set_var("COUNTER_ROOTCAUSE", "1");
        let matrix = MatrixSpec {
            storage: StorageAdapter::Mem,
            topology: Topology::Embedded,
            persist_enabled: true,
            ..MatrixSpec::default()
        };
        let persist = PersistConfig {
            queue_max: 8,
            batch_max: 32,
            batch_enabled: true,
            overflow: PersistOverflow::Block,
            ..PersistConfig::default()
        };
        let installed = install_bench_matrix_with_persist(matrix, "sw8-block", Some(persist))
            .await
            .expect("install");
        let before = RootcauseSnapshot::capture();
        for i in 0..256u32 {
            let sample = i.to_string();
            try_record_counter_now(
                ZERO_LOSS_COUNTER_NAME,
                &[
                    ("path", ZERO_LOSS_PATH),
                    ("cell", "block"),
                    ("sample", sample.as_str()),
                ],
                1,
            );
        }
        flush_installed(&installed.spectra).await.expect("flush");
        let drops = RootcauseSnapshot::capture()
            .persist_queue_drops
            .saturating_sub(before.persist_queue_drops);
        assert_eq!(drops, 0, "Block overflow must not drop jobs");
        let visible = count_cell_points(installed.spectra.router().as_ref(), "block")
            .await
            .expect("query");
        assert_eq!(visible, 256, "every blocked enqueue must become visible");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn mem_smoke_paced_block_zero_drops() {
        let _g = TEST_LOCK.lock().await;
        std::env::set_var("COUNTER_ROOTCAUSE", "1");
        let matrix = MatrixSpec {
            storage: StorageAdapter::Mem,
            topology: Topology::Embedded,
            persist_enabled: true,
            ..MatrixSpec::default()
        };
        let persist = PersistConfig {
            overflow: PersistOverflow::Block,
            batch_max: ZERO_LOSS_BATCH_MAX,
            batch_enabled: true,
            ..PersistConfig::default()
        };
        let installed = install_bench_matrix_with_persist(matrix, "sw8-smoke", Some(persist))
            .await
            .expect("install");
        let cell =
            run_paced_zero_loss_counter(&installed, 80, 2, Duration::from_millis(400), 2_000)
                .await
                .expect("paced cell");
        assert_eq!(cell.offered_rate, 80);
        assert_eq!(cell.persist_queue_drops, 0);
        assert_eq!(cell.adapter_errors, 0);
        assert!(cell.visibility_confirmed);
        assert!(cell.zero_loss, "fail_reason={:?}", cell.fail_reason);
        assert!(cell.durable_rate_ratio >= DURABLE_RATE_RATIO_MIN);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn sqlite_smoke_paced_block_zero_drops() {
        let _g = TEST_LOCK.lock().await;
        std::env::set_var("COUNTER_ROOTCAUSE", "1");
        let matrix = MatrixSpec {
            storage: StorageAdapter::Sqlite,
            topology: Topology::Embedded,
            persist_enabled: true,
            ..MatrixSpec::default()
        };
        let persist = PersistConfig {
            overflow: PersistOverflow::Block,
            batch_max: ZERO_LOSS_BATCH_MAX,
            batch_enabled: true,
            ..PersistConfig::default()
        };
        let installed = install_bench_matrix_with_persist(matrix, "sw8-sqlite", Some(persist))
            .await
            .expect("install");
        let cell =
            run_paced_zero_loss_counter(&installed, 80, 2, Duration::from_millis(400), 2_000)
                .await
                .expect("paced cell");
        assert_eq!(cell.offered_rate, 80);
        assert_eq!(cell.persist_queue_drops, 0);
        assert_eq!(cell.adapter_errors, 0);
        assert!(cell.visibility_confirmed);
        assert!(cell.zero_loss, "fail_reason={:?}", cell.fail_reason);
        assert!(cell.durable_rate_ratio >= DURABLE_RATE_RATIO_MIN);
    }
}
