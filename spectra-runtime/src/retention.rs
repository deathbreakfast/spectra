//! Scheduled, bounded retention prune for Spectra stores.
//!
//! [`RetentionConfig`] says how long to keep metrics and each event table.
//! [`RetentionPruner::prune_once`] deletes rows older than that in capped batches, and
//! [`RetentionPruner::spawn`] repeats it on an interval. Hosts that already have a scheduler
//! can skip `spawn` and call `prune_once` from a job.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use spectra_core::{PruneOutcome, SharedMetricsBackend, SpectraRouter};
use tokio::task::JoinHandle;

/// What to keep and how hard each prune run may work.
///
/// With no `metrics_days`, no `default_event_days` and no `table_days`, nothing is pruned.
#[derive(Debug, Clone)]
pub struct RetentionConfig {
    /// Time between scheduled runs (default 1 hour).
    pub interval: Duration,
    /// Rows deleted per backend call (default 5000).
    pub batch_rows: u32,
    /// Backend calls per target in one run (default 20), so one run is bounded even when a
    /// store is far behind. Leftover rows go in the next run.
    pub max_batches: u32,
    /// Days of metrics to keep, store-wide. `None` keeps all metrics.
    pub metrics_days: Option<u32>,
    /// Days to keep for every registered event table without a `table_days` entry.
    pub default_event_days: Option<u32>,
    /// Days to keep per event table, overriding `default_event_days`.
    pub table_days: HashMap<String, u32>,
}

impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(3600),
            batch_rows: 5000,
            max_batches: 20,
            metrics_days: None,
            default_event_days: None,
            table_days: HashMap::new(),
        }
    }
}

/// Rows deleted by one [`RetentionPruner::prune_once`] run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PruneReport {
    /// Metric points deleted across all metrics backends.
    pub metrics_deleted: u64,
    /// Event rows deleted, by table.
    pub events_deleted: HashMap<String, u64>,
    /// Targets whose prune call failed; the rest of the run still happened.
    pub failures: u32,
}

/// Prunes the configured stores. Cheap to clone; share it with the scheduler.
#[derive(Clone)]
pub struct RetentionPruner {
    config: RetentionConfig,
    router: Arc<SpectraRouter>,
    metrics_backends: Vec<SharedMetricsBackend>,
    event_tables: Vec<String>,
}

impl RetentionPruner {
    /// Builds a pruner over `metrics_backends` (one entry per distinct metrics store) and
    /// `event_tables`, each resolved to its store through `router`.
    #[must_use]
    pub fn new(
        config: RetentionConfig,
        router: Arc<SpectraRouter>,
        metrics_backends: Vec<SharedMetricsBackend>,
        event_tables: Vec<String>,
    ) -> Self {
        Self {
            config,
            router,
            metrics_backends,
            event_tables,
        }
    }

    fn event_days(&self, table: &str) -> Option<u32> {
        self.config
            .table_days
            .get(table)
            .copied()
            .or(self.config.default_event_days)
    }

    /// Runs one bounded prune pass as of `now`.
    pub async fn prune_once(&self, now: DateTime<Utc>) -> PruneReport {
        let mut report = PruneReport::default();

        if let Some(days) = self.config.metrics_days {
            let before = now - chrono::Duration::days(i64::from(days));
            let mut seen: Vec<usize> = Vec::new();
            for backend in &self.metrics_backends {
                let id = Arc::as_ptr(backend).cast::<()>() as usize;
                if seen.contains(&id) {
                    continue;
                }
                seen.push(id);
                let result = self
                    .run_batches(|rows| backend.prune_before(before, rows))
                    .await;
                match result {
                    Ok(n) => report.metrics_deleted += n,
                    Err(e) => {
                        report.failures += 1;
                        tracing::warn!(error = %e, "spectra retention: metrics prune failed");
                    }
                }
            }
        }

        let mut tables: Vec<&String> = self
            .event_tables
            .iter()
            .chain(self.config.table_days.keys())
            .collect();
        tables.sort();
        tables.dedup();
        for table in tables {
            let Some(days) = self.event_days(table) else {
                continue;
            };
            let before = now - chrono::Duration::days(i64::from(days));
            let backend = self.router.resolve_event(table);
            let result = self
                .run_batches(|rows| backend.prune_before(table, before, rows))
                .await;
            match result {
                Ok(n) => {
                    report.events_deleted.insert(table.clone(), n);
                }
                Err(e) => {
                    report.failures += 1;
                    tracing::warn!(table = %table, error = %e, "spectra retention: event prune failed");
                }
            }
        }

        tracing::info!(
            metrics_deleted = report.metrics_deleted,
            event_tables = report.events_deleted.len(),
            failures = report.failures,
            "spectra retention: prune run finished"
        );
        report
    }

    async fn run_batches<F, Fut>(&self, mut call: F) -> spectra_core::Result<u64>
    where
        F: FnMut(u32) -> Fut,
        Fut: std::future::Future<Output = spectra_core::Result<PruneOutcome>>,
    {
        let mut total = 0;
        for _ in 0..self.config.max_batches {
            let outcome = call(self.config.batch_rows).await?;
            total += outcome.deleted;
            if !outcome.more {
                break;
            }
        }
        Ok(total)
    }

    /// Spawns the scheduled loop on the current tokio runtime. The first run happens after
    /// one `interval`. Dropping the returned handle stops the loop.
    #[must_use]
    pub fn spawn(self) -> RetentionHandle {
        let interval = self.config.interval;
        let task = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            ticker.tick().await;
            loop {
                ticker.tick().await;
                self.prune_once(Utc::now()).await;
            }
        });
        RetentionHandle { task }
    }
}

/// Stops the scheduled prune loop when dropped.
pub struct RetentionHandle {
    task: JoinHandle<()>,
}

impl Drop for RetentionHandle {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use spectra_backend_mem::{MemEventsBackend, MemMetricsBackend};
    use spectra_core::{EventStorageBackend, EventsQueryFilter, MetricsStorageBackend};

    fn setup(
        config: RetentionConfig,
    ) -> (
        RetentionPruner,
        Arc<MemEventsBackend>,
        Arc<MemMetricsBackend>,
    ) {
        let events = Arc::new(MemEventsBackend::new());
        let metrics = Arc::new(MemMetricsBackend::new());
        let router = Arc::new(SpectraRouter::with_defaults(
            metrics.clone() as SharedMetricsBackend,
            events.clone() as spectra_core::SharedEventBackend,
        ));
        let pruner = RetentionPruner::new(
            config,
            router,
            vec![metrics.clone() as SharedMetricsBackend],
            vec!["audit".into(), "debug".into()],
        );
        (pruner, events, metrics)
    }

    async fn count(events: &MemEventsBackend, table: &str) -> usize {
        events
            .query_rows(EventsQueryFilter {
                table: table.into(),
                ..Default::default()
            })
            .await
            .expect("query")
            .len()
    }

    #[tokio::test]
    async fn prunes_per_table_and_leaves_unconfigured_tables() {
        let mut config = RetentionConfig::default();
        config.table_days.insert("debug".into(), 7);
        let (pruner, events, _) = setup(config);
        let now = Utc::now();
        let old = now - chrono::Duration::days(30);
        for table in ["audit", "debug"] {
            events
                .append_row(table, &json!({}), old, None)
                .await
                .expect("w");
            events
                .append_row(table, &json!({}), now, None)
                .await
                .expect("w");
        }
        let report = pruner.prune_once(now).await;
        assert_eq!(report.events_deleted.get("debug"), Some(&1));
        assert!(!report.events_deleted.contains_key("audit"));
        assert_eq!(count(&events, "debug").await, 1);
        assert_eq!(count(&events, "audit").await, 2);
    }

    #[tokio::test]
    async fn one_run_is_bounded_by_max_batches() {
        let config = RetentionConfig {
            batch_rows: 2,
            max_batches: 2,
            default_event_days: Some(1),
            ..Default::default()
        };
        let (pruner, events, _) = setup(config);
        let now = Utc::now();
        let old = now - chrono::Duration::days(5);
        for _ in 0..7 {
            events
                .append_row("audit", &json!({}), old, None)
                .await
                .expect("w");
        }
        assert_eq!(pruner.prune_once(now).await.events_deleted["audit"], 4);
        assert_eq!(pruner.prune_once(now).await.events_deleted["audit"], 3);
        assert_eq!(count(&events, "audit").await, 0);
    }

    #[tokio::test]
    async fn prunes_metrics_and_noops_without_config() {
        let (idle, _, metrics) = setup(RetentionConfig::default());
        let now = Utc::now();
        let old = now - chrono::Duration::days(40);
        metrics
            .record_counter("hits", &json!({}), 1, old)
            .await
            .expect("w");
        assert_eq!(idle.prune_once(now).await, PruneReport::default());

        let config = RetentionConfig {
            metrics_days: Some(30),
            ..Default::default()
        };
        let (pruner, _, metrics) = setup(config);
        metrics
            .record_counter("hits", &json!({}), 1, old)
            .await
            .expect("w");
        metrics
            .record_counter("hits", &json!({}), 1, now)
            .await
            .expect("w");
        assert_eq!(pruner.prune_once(now).await.metrics_deleted, 1);
    }

    #[tokio::test(start_paused = true)]
    async fn spawned_loop_prunes_on_interval_and_stops_on_drop() {
        let config = RetentionConfig {
            interval: Duration::from_secs(60),
            default_event_days: Some(1),
            ..Default::default()
        };
        let (pruner, events, _) = setup(config);
        let old = Utc::now() - chrono::Duration::days(5);
        events
            .append_row("audit", &json!({}), old, None)
            .await
            .expect("w");
        let handle = pruner.spawn();
        tokio::time::sleep(Duration::from_secs(61)).await;
        assert_eq!(count(&events, "audit").await, 0);
        drop(handle);
        events
            .append_row("audit", &json!({}), old, None)
            .await
            .expect("w");
        tokio::time::sleep(Duration::from_secs(120)).await;
        assert_eq!(count(&events, "audit").await, 1);
    }
}
