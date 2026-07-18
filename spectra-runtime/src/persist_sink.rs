//! Generic sink that persists emits to [`SpectraRouter`] storage backends.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use chrono::{DateTime, Utc};
use serde_json::{Map, Value};
use spectra_core::{
    current_emit_ts, record_persist_queue_drop, record_storage_batch_write_events,
    record_storage_batch_write_metrics, record_storage_write_events, record_storage_write_metrics,
    EventWriteRow, MetricWriteRow, SpectraRouter, SpectraSink,
};
use tokio::sync::{mpsc, oneshot};

use crate::persist_config::{PersistConfig, PersistOverflow};

enum PersistJob {
    Counter {
        name: String,
        labels: Value,
        delta: i64,
        ts: DateTime<Utc>,
    },
    Gauge {
        name: String,
        labels: Value,
        value: f64,
        ts: DateTime<Utc>,
    },
    Event {
        table: String,
        fields: Value,
        ts: DateTime<Utc>,
    },
    /// Barrier: completes after all prior jobs in the queue have been processed.
    Flush(oneshot::Sender<()>),
}

/// Handle to wait until the persist queue has drained prior emits.
#[derive(Clone)]
pub struct PersistHandle {
    tx: mpsc::Sender<PersistJob>,
}

impl PersistHandle {
    /// Wait until every job enqueued before this call has been flushed to storage.
    pub async fn flush(&self) -> spectra_core::Result<()> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.tx
            .send(PersistJob::Flush(ack_tx))
            .await
            .map_err(|_| {
                spectra_core::Error::Internal("persist queue closed during flush".into())
            })?;
        ack_rx
            .await
            .map_err(|_| spectra_core::Error::Internal("persist flush ack dropped".into()))
    }
}

/// [`SpectraSink`] that queues telemetry for asynchronous storage through a [`SpectraRouter`].
///
/// Queue capacity and batching are configured with [`PersistConfig`] (via
/// [`crate::SpectraBuilder::persist`]), not environment variables.
///
/// Default overflow policy is [`PersistOverflow::Drop`] (non-blocking; drops counted via
/// `persist_queue_drops`). Use [`PersistOverflow::Block`] for backpressure.
/// Call [`PersistHandle::flush`] (or [`crate::Spectra::flush_persist`]) to wait for durability.
pub struct StoragePersistSink {
    inner: Option<Arc<dyn SpectraSink>>,
    tx: mpsc::Sender<PersistJob>,
    handle: PersistHandle,
    overflow: PersistOverflow,
}

impl StoragePersistSink {
    /// Persist-only sink with default [`PersistConfig`].
    pub fn new(router: Arc<SpectraRouter>) -> Self {
        Self::with_config(router, None, PersistConfig::default())
    }

    /// Persist-only sink with explicit config.
    pub fn new_with_config(router: Arc<SpectraRouter>, config: PersistConfig) -> Self {
        Self::with_config(router, None, config)
    }

    /// Invoke `inner` on the hot path, then enqueue the same emit for async storage persist.
    pub fn with_inner(router: Arc<SpectraRouter>, inner: Option<Arc<dyn SpectraSink>>) -> Self {
        Self::with_config(router, inner, PersistConfig::default())
    }

    /// Like [`Self::with_inner`] with explicit [`PersistConfig`].
    pub fn with_config(
        router: Arc<SpectraRouter>,
        inner: Option<Arc<dyn SpectraSink>>,
        config: PersistConfig,
    ) -> Self {
        let config = config.normalized();
        let overflow = config.overflow;
        let (tx, mut rx) = mpsc::channel(config.queue_max);
        let handle = PersistHandle { tx: tx.clone() };
        let router_worker = Arc::clone(&router);
        let batch_max = config.batch_max;
        let batch_wait = config.batch_wait;
        let batch_enabled = config.batch_enabled;

        tokio::spawn(async move {
            while let Some(first) = rx.recv().await {
                if let PersistJob::Flush(ack) = first {
                    let _ = ack.send(());
                    continue;
                }

                let mut batch = vec![first];
                let mut pending_flush: Option<oneshot::Sender<()>> = None;

                while batch.len() < batch_max {
                    match rx.try_recv() {
                        Ok(PersistJob::Flush(ack)) => {
                            pending_flush = Some(ack);
                            break;
                        }
                        Ok(job) => batch.push(job),
                        Err(mpsc::error::TryRecvError::Empty) => {
                            if batch.len() == 1 && batch_enabled {
                                tokio::time::sleep(batch_wait).await;
                                match rx.try_recv() {
                                    Ok(PersistJob::Flush(ack)) => {
                                        pending_flush = Some(ack);
                                    }
                                    Ok(job) => {
                                        batch.push(job);
                                        continue;
                                    }
                                    Err(_) => {}
                                }
                            }
                            break;
                        }
                        Err(mpsc::error::TryRecvError::Disconnected) => break,
                    }
                }

                if batch_enabled {
                    if let Err(e) = flush_batch(&router_worker, batch).await {
                        log::warn!("[spectra:persist] batch flush: {e}");
                    }
                } else {
                    for job in batch {
                        if let Err(e) = run_job(&router_worker, job).await {
                            log::warn!("[spectra:persist] {e}");
                        }
                    }
                }

                if let Some(ack) = pending_flush {
                    let _ = ack.send(());
                }
            }
        });

        Self {
            inner,
            tx,
            handle,
            overflow,
        }
    }

    /// Handle for [`PersistHandle::flush`].
    pub fn handle(&self) -> PersistHandle {
        self.handle.clone()
    }
}

impl SpectraSink for StoragePersistSink {
    fn record_counter(&self, name: &str, labels: &[(&str, &str)], delta: i64) {
        if let Some(inner) = &self.inner {
            inner.record_counter(name, labels, delta);
        }
        enqueue(
            &self.tx,
            PersistJob::Counter {
                name: name.to_string(),
                labels: labels_to_value(labels),
                delta,
                ts: emit_ts(),
            },
            self.overflow,
        );
    }

    fn record_gauge(&self, name: &str, labels: &[(&str, &str)], value: f64) {
        if let Some(inner) = &self.inner {
            inner.record_gauge(name, labels, value);
        }
        enqueue(
            &self.tx,
            PersistJob::Gauge {
                name: name.to_string(),
                labels: labels_to_value(labels),
                value,
                ts: emit_ts(),
            },
            self.overflow,
        );
    }

    fn log_event(&self, table: &str, fields: &Value) {
        if let Some(inner) = &self.inner {
            inner.log_event(table, fields);
        }
        enqueue(
            &self.tx,
            PersistJob::Event {
                table: table.to_string(),
                fields: fields.clone(),
                ts: emit_ts(),
            },
            self.overflow,
        );
    }
}

fn emit_ts() -> DateTime<Utc> {
    current_emit_ts()
}

fn labels_to_value(labels: &[(&str, &str)]) -> Value {
    let mut map = Map::new();
    for (k, v) in labels {
        map.insert((*k).to_string(), Value::String((*v).to_string()));
    }
    Value::Object(map)
}

fn enqueue(tx: &mpsc::Sender<PersistJob>, job: PersistJob, overflow: PersistOverflow) {
    match overflow {
        PersistOverflow::Drop => {
            if tx.try_send(job).is_err() {
                record_persist_queue_drop();
                log::warn!("[spectra:persist] queue full; dropping job");
            }
        }
        PersistOverflow::Block => {
            let send_result = if tokio::runtime::Handle::try_current().is_ok() {
                tokio::task::block_in_place(|| {
                    let handle = tokio::runtime::Handle::current();
                    handle.block_on(tx.send(job))
                })
            } else {
                tx.blocking_send(job)
            };
            if send_result.is_err() {
                record_persist_queue_drop();
                log::warn!("[spectra:persist] queue closed; dropping job");
            }
        }
    }
}

async fn run_job(router: &SpectraRouter, job: PersistJob) -> spectra_core::Result<()> {
    match job {
        PersistJob::Flush(_) => Ok(()),
        PersistJob::Counter {
            name,
            labels,
            delta,
            ts,
        } => {
            let started = Instant::now();
            let backend = router.resolve_metrics(&name);
            let result = backend.record_counter(&name, &labels, delta, ts).await;
            if result.is_ok() {
                record_storage_write_metrics(started.elapsed());
            }
            result
        }
        PersistJob::Gauge {
            name,
            labels,
            value,
            ts,
        } => {
            let started = Instant::now();
            let backend = router.resolve_metrics(&name);
            let result = backend.record_gauge(&name, &labels, value, ts).await;
            if result.is_ok() {
                record_storage_write_metrics(started.elapsed());
            }
            result
        }
        PersistJob::Event { table, fields, ts } => {
            let started = Instant::now();
            let backend = router.resolve_event(&table);
            let result = backend.append_row(&table, &fields, ts, None).await;
            if result.is_ok() {
                record_storage_write_events(started.elapsed());
            }
            result
        }
    }
}

async fn flush_batch(router: &SpectraRouter, batch: Vec<PersistJob>) -> spectra_core::Result<()> {
    fn arc_key<T: ?Sized>(arc: &Arc<T>) -> usize {
        Arc::as_ptr(arc) as *const () as usize
    }

    let mut metrics_buckets: HashMap<usize, (spectra_core::SharedMetricsBackend, Vec<MetricWriteRow>)> =
        HashMap::new();
    let mut event_buckets: HashMap<usize, (spectra_core::SharedEventBackend, Vec<EventWriteRow>)> =
        HashMap::new();

    for job in batch {
        match job {
            PersistJob::Flush(_) => {}
            PersistJob::Counter {
                name,
                labels,
                delta,
                ts,
            } => {
                let backend = router.resolve_metrics(&name);
                let key = arc_key(&backend);
                metrics_buckets
                    .entry(key)
                    .or_insert_with(|| (Arc::clone(&backend), Vec::new()))
                    .1
                    .push(MetricWriteRow {
                        name,
                        kind: "counter",
                        value: Value::from(delta),
                        labels,
                        ts,
                        correlation_id: None,
                    });
            }
            PersistJob::Gauge {
                name,
                labels,
                value,
                ts,
            } => {
                let backend = router.resolve_metrics(&name);
                let key = arc_key(&backend);
                metrics_buckets
                    .entry(key)
                    .or_insert_with(|| (Arc::clone(&backend), Vec::new()))
                    .1
                    .push(MetricWriteRow {
                        name,
                        kind: "gauge",
                        value: serde_json::json!(value),
                        labels,
                        ts,
                        correlation_id: None,
                    });
            }
            PersistJob::Event { table, fields, ts } => {
                let backend = router.resolve_event(&table);
                let key = arc_key(&backend);
                event_buckets
                    .entry(key)
                    .or_insert_with(|| (Arc::clone(&backend), Vec::new()))
                    .1
                    .push(EventWriteRow {
                        table,
                        fields,
                        ts,
                        correlation_id: None,
                    });
            }
        }
    }

    for (_, (backend, rows)) in metrics_buckets {
        if !rows.is_empty() {
            let started = Instant::now();
            let row_count = rows.len() as u64;
            backend.record_metrics_batch(&rows).await?;
            record_storage_batch_write_metrics(started.elapsed(), row_count);
        }
    }
    for (_, (backend, rows)) in event_buckets {
        if !rows.is_empty() {
            let started = Instant::now();
            let row_count = rows.len() as u64;
            backend.append_rows_batch(&rows).await?;
            record_storage_batch_write_events(started.elapsed(), row_count);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use spectra_backend_mem::{MemEventsBackend, MemMetricsBackend};
    use spectra_core::{
        try_record_counter_now, NoOpSink, SharedEventBackend, SharedMetricsBackend, SpectraConfig,
    };

    static PERSIST_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    #[tokio::test]
    async fn batch_flush_persists_multiple_counters() {
        let _g = PERSIST_TEST_LOCK.lock().await;
        spectra_core::install_config(SpectraConfig {
            enabled: false,
            ..Default::default()
        });

        let metrics: SharedMetricsBackend = Arc::new(MemMetricsBackend::new());
        let events: SharedEventBackend = Arc::new(MemEventsBackend::new());
        let router = Arc::new(SpectraRouter::with_defaults(
            Arc::clone(&metrics),
            Arc::clone(&events),
        ));
        let sink = StoragePersistSink::new_with_config(
            Arc::clone(&router),
            PersistConfig {
                batch_max: 8,
                batch_enabled: true,
                ..PersistConfig::default()
            },
        );
        let handle = sink.handle();
        spectra_core::set_sink(Arc::new(sink));

        for i in 0..4 {
            try_record_counter_now(&format!("batch_counter_{i}"), &[], 1);
        }
        handle.flush().await.expect("flush");

        for i in 0..4 {
            let points = router
                .query_metrics(spectra_core::MetricsQueryRange {
                    metric_name: format!("batch_counter_{i}"),
                    start: Utc::now() - chrono::Duration::seconds(5),
                    end: Utc::now() + chrono::Duration::seconds(1),
                    label_matchers: vec![],
                })
                .await
                .expect("query");
            assert_eq!(points.len(), 1, "counter {i}");
        }

        spectra_core::set_sink(Arc::new(NoOpSink));
    }
}
