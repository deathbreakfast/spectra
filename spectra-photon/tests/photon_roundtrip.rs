//! Publisher sink → Photon group topic → batched consumer → storage, all in process.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use serde_json::{json, Value};
use spectra_backend_mem::{MemEventsBackend, MemMetricsBackend};
use spectra_core::{
    Error, EventStorageBackend, EventWriteRow, MetricWriteRow, MetricsStorageBackend,
    SharedEventBackend, SharedMetricsBackend, SpectraRouter, SpectraSink, StorageEngineType,
};
use spectra_photon::{
    all_shards, shard_range, BatchConfig, BatchedStorageConsumer, DurableConfig, GroupSubscription,
    PhotonPublishSink, SpectraEnvelope, SHARD_COUNT, TOPIC_NAME,
};

const DEV_KEY: &str = "cGhvdG9uLWRldi10cmFuc3BvcnQta2V5LTMyYnl0ZXM=";

fn photon() -> photon::Photon {
    std::env::set_var("PHOTON_TRANSPORT_KEY", DEV_KEY);
    photon::Photon::builder().auto_registry().build().unwrap()
}

fn ts() -> DateTime<Utc> {
    Utc::now()
}

fn counter(name: &str, n: i64) -> SpectraEnvelope {
    SpectraEnvelope::Counter {
        name: name.into(),
        labels: json!({}),
        delta: n,
        ts: ts(),
    }
}

/// Backend that records the size of every batch call and can fail one metric name.
#[derive(Default)]
struct RecordingBackend {
    metric_batches: Mutex<Vec<usize>>,
    event_batches: Mutex<Vec<usize>>,
    fail_metric: Option<&'static str>,
}

#[async_trait]
impl MetricsStorageBackend for RecordingBackend {
    fn engine_type(&self) -> StorageEngineType {
        StorageEngineType::Mem
    }
    async fn record_counter(
        &self,
        _: &str,
        _: &Value,
        _: i64,
        _: DateTime<Utc>,
    ) -> spectra_core::Result<()> {
        Ok(())
    }
    async fn record_gauge(
        &self,
        _: &str,
        _: &Value,
        _: f64,
        _: DateTime<Utc>,
    ) -> spectra_core::Result<()> {
        Ok(())
    }
    async fn record_metrics_batch(&self, rows: &[MetricWriteRow]) -> spectra_core::Result<()> {
        if let Some(bad) = self.fail_metric {
            if rows.iter().any(|r| r.name == bad) {
                return Err(Error::Config("injected failure".into()));
            }
        }
        self.metric_batches.lock().push(rows.len());
        Ok(())
    }
}

#[async_trait]
impl EventStorageBackend for RecordingBackend {
    fn engine_type(&self) -> StorageEngineType {
        StorageEngineType::Mem
    }
    async fn append_row(
        &self,
        _: &str,
        _: &Value,
        _: DateTime<Utc>,
        _: Option<&str>,
    ) -> spectra_core::Result<()> {
        Ok(())
    }
    async fn append_rows_batch(&self, rows: &[EventWriteRow]) -> spectra_core::Result<()> {
        self.event_batches.lock().push(rows.len());
        Ok(())
    }
}

fn recording_router(backend: &Arc<RecordingBackend>) -> Arc<SpectraRouter> {
    let metrics: SharedMetricsBackend = Arc::clone(backend) as SharedMetricsBackend;
    let events: SharedEventBackend = Arc::clone(backend) as SharedEventBackend;
    Arc::new(SpectraRouter::with_defaults(metrics, events))
}

async fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while !done() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for {what}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// Start a subscription that is safe to publish ahead of.
///
/// Photon has no "subscription is live" signal, and a plain `start` joins at the live tail, so
/// emits published before its streams are polled are lost. A durable subscription with no saved
/// position replays from the start of the log instead, so tests need no startup delay.
async fn subscribe(
    photon: &photon::Photon,
    shards: &[u32],
    consumer: &BatchedStorageConsumer,
    name: &str,
) -> GroupSubscription {
    GroupSubscription::start_durable(photon, shards, consumer.clone(), DurableConfig::new(name))
        .await
        .unwrap()
}

/// Window for asserting that nothing arrives. Absence cannot be observed, so this is the one
/// place a test waits on the clock.
async fn quiet_window() {
    tokio::time::sleep(Duration::from_millis(300)).await;
}

/// Events covered by the checkpoints saved under `name`: each shard's checkpoint is the count
/// of emits it has carried.
async fn checkpointed(photon: &photon::Photon, name: &str) -> i64 {
    let mut total = 0;
    for shard in 0..SHARD_COUNT {
        let key = photon::shard_router::shard_storage_key(shard);
        total += photon
            .get_checkpoint_seq(name, TOPIC_NAME, Some(&key))
            .await
            .unwrap()
            .unwrap_or(0);
    }
    total
}

#[tokio::test(flavor = "multi_thread")]
async fn publish_sink_to_consumer_lands_in_mem_backends() {
    let photon = photon();
    let metrics = Arc::new(MemMetricsBackend::new());
    let events = Arc::new(MemEventsBackend::new());
    let router = Arc::new(SpectraRouter::with_defaults(
        Arc::clone(&metrics) as SharedMetricsBackend,
        Arc::clone(&events) as SharedEventBackend,
    ));
    let consumer = BatchedStorageConsumer::new(router, BatchConfig::default());
    let _sub = subscribe(&photon, &all_shards(), &consumer, "roundtrip").await;

    let sink = PhotonPublishSink::new(photon.clone());
    for i in 0..20 {
        sink.record_counter(
            "hits",
            &[("route", if i % 2 == 0 { "/a" } else { "/b" })],
            1,
        );
    }
    for i in 0..5 {
        sink.record_gauge("depth", &[], f64::from(i));
    }
    for i in 0..7 {
        sink.log_event("audit", &json!({"n": i}));
    }

    wait_for("32 rows written", || consumer.stats().rows_written >= 32).await;
    consumer.flush().await.unwrap();

    assert_eq!(sink.stats().dropped, 0);
    assert_eq!(sink.stats().published, 32);
    assert_eq!(metrics.metric_stats("hits").await.unwrap().rows, 20);
    assert_eq!(metrics.metric_stats("depth").await.unwrap().rows, 5);
    assert_eq!(events.event_stats("audit").await.unwrap().rows, 7);
    assert_eq!(consumer.stats().batches_failed, 0);
}

#[tokio::test]
async fn size_threshold_splits_batches() {
    let backend = Arc::new(RecordingBackend::default());
    let consumer = BatchedStorageConsumer::new(
        recording_router(&backend),
        BatchConfig {
            batch_max: 4,
            batch_wait: Duration::from_secs(60),
            queue_max: 64,
        },
    );
    for i in 0..10 {
        consumer.ingest(counter("m", i)).await.unwrap();
    }
    consumer.flush().await.unwrap();
    assert_eq!(*backend.metric_batches.lock(), vec![4, 4, 2]);
    assert_eq!(consumer.stats().rows_written, 10);
}

#[tokio::test]
async fn time_threshold_flushes_partial_batch() {
    let backend = Arc::new(RecordingBackend::default());
    let consumer = BatchedStorageConsumer::new(
        recording_router(&backend),
        BatchConfig {
            batch_max: 1000,
            batch_wait: Duration::from_millis(30),
            queue_max: 64,
        },
    );
    for i in 0..3 {
        consumer.ingest(counter("m", i)).await.unwrap();
    }
    // No explicit flush: the timer alone must write the partial batch.
    wait_for("timed flush", || consumer.stats().rows_written == 3).await;
    assert_eq!(*backend.metric_batches.lock(), vec![3]);
}

#[tokio::test]
async fn failed_batch_is_counted_and_later_batches_still_write() {
    let backend = Arc::new(RecordingBackend {
        fail_metric: Some("bad"),
        ..RecordingBackend::default()
    });
    let consumer = BatchedStorageConsumer::new(
        recording_router(&backend),
        BatchConfig {
            batch_max: 2,
            batch_wait: Duration::from_secs(60),
            queue_max: 64,
        },
    );
    consumer.ingest(counter("bad", 1)).await.unwrap();
    consumer.ingest(counter("bad", 1)).await.unwrap();
    consumer.ingest(counter("good", 1)).await.unwrap();
    consumer.ingest(counter("good", 1)).await.unwrap();
    consumer.flush().await.unwrap();

    let stats = consumer.stats();
    assert_eq!((stats.batches_failed, stats.rows_failed), (1, 2));
    assert_eq!((stats.batches_written, stats.rows_written), (1, 2));
}

#[tokio::test]
async fn shutdown_drains_queue_and_then_refuses_input() {
    let backend = Arc::new(RecordingBackend::default());
    let consumer = BatchedStorageConsumer::new(
        recording_router(&backend),
        BatchConfig {
            batch_max: 100,
            batch_wait: Duration::from_secs(60),
            queue_max: 64,
        },
    );
    for i in 0..5 {
        consumer.ingest(counter("m", i)).await.unwrap();
    }
    consumer.shutdown().await;
    assert_eq!(consumer.stats().rows_written, 5);
    assert!(consumer.ingest(counter("m", 1)).await.is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn two_consumers_with_disjoint_shards_split_the_stream_without_duplicates() {
    let photon = photon();
    let a_backend = Arc::new(RecordingBackend::default());
    let b_backend = Arc::new(RecordingBackend::default());
    let config = BatchConfig {
        batch_max: 8,
        batch_wait: Duration::from_millis(20),
        queue_max: 256,
    };
    let a = BatchedStorageConsumer::new(recording_router(&a_backend), config.clone());
    let b = BatchedStorageConsumer::new(recording_router(&b_backend), config);
    let _sub_a = subscribe(&photon, &shard_range(0..=7), &a, "group-a").await;
    let _sub_b = subscribe(&photon, &shard_range(8..=15), &b, "group-b").await;

    let sink = PhotonPublishSink::new(photon.clone());
    let total = 200u64;
    for i in 0..total {
        sink.record_counter(&format!("series_{i}"), &[], 1);
    }

    wait_for("all rows written", || {
        a.stats().rows_written + b.stats().rows_written >= total
    })
    .await;
    a.flush().await.unwrap();
    b.flush().await.unwrap();

    let (wa, wb) = (a.stats().rows_written, b.stats().rows_written);
    assert_eq!(wa + wb, total, "every emit written exactly once");
    assert!(wa > 0 && wb > 0, "both consumers took part: {wa} / {wb}");
}

#[tokio::test(flavor = "multi_thread")]
async fn in_memory_subscription_starts_at_the_live_tail() {
    let photon = photon();
    let sink = PhotonPublishSink::new(photon.clone());
    for _ in 0..5 {
        sink.record_counter("early", &[], 1);
    }
    wait_for("publish", || sink.stats().published == 5).await;

    let backend = Arc::new(RecordingBackend::default());
    let consumer = BatchedStorageConsumer::new(recording_router(&backend), BatchConfig::default());
    let _sub = GroupSubscription::start(&photon, &all_shards(), consumer.clone());
    quiet_window().await;
    consumer.flush().await.unwrap();
    assert_eq!(
        consumer.stats().rows_written,
        0,
        "earlier emits are not replayed"
    );
}

fn slow_config() -> BatchConfig {
    BatchConfig {
        batch_max: 100,
        batch_wait: Duration::from_secs(60),
        queue_max: 256,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn durable_subscription_resumes_after_its_checkpoint() {
    let photon = photon();
    let sink = PhotonPublishSink::new(photon.clone());

    let first_backend = Arc::new(RecordingBackend::default());
    let first = BatchedStorageConsumer::new(recording_router(&first_backend), slow_config());
    let sub = subscribe(&photon, &all_shards(), &first, "resume").await;
    for i in 0..10 {
        sink.record_counter(&format!("before_{i}"), &[], 1);
    }
    wait_for("first run ingest", || sink.stats().published == 10).await;
    // Ingest is async after publish; flush until every emit has been written.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while first.stats().rows_written < 10 {
        assert!(tokio::time::Instant::now() < deadline, "first run stalled");
        first.flush().await.unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    sub.checkpoint().await.unwrap();
    assert_eq!(checkpointed(&photon, "resume").await, 10);
    drop(sub);

    for i in 0..5 {
        sink.record_counter(&format!("while_down_{i}"), &[], 1);
    }
    wait_for("down-time publish", || sink.stats().published == 15).await;

    let second_backend = Arc::new(RecordingBackend::default());
    let second = BatchedStorageConsumer::new(recording_router(&second_backend), slow_config());
    let _sub = subscribe(&photon, &all_shards(), &second, "resume").await;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while second.stats().rows_written < 5 {
        assert!(tokio::time::Instant::now() < deadline, "restart stalled");
        second.flush().await.unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    quiet_window().await;
    second.flush().await.unwrap();
    assert_eq!(
        second.stats().rows_written,
        5,
        "only the emits after the checkpoint are delivered"
    );

    let fresh_backend = Arc::new(RecordingBackend::default());
    let fresh = BatchedStorageConsumer::new(recording_router(&fresh_backend), slow_config());
    let _fresh_sub = subscribe(&photon, &all_shards(), &fresh, "other-name").await;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while fresh.stats().rows_written < 15 {
        assert!(tokio::time::Instant::now() < deadline, "fresh name stalled");
        fresh.flush().await.unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(fresh.stats().rows_written, 15, "a new name replays the log");
}

#[tokio::test(flavor = "multi_thread")]
async fn checkpoint_waits_for_the_batch_to_be_written() {
    let photon = photon();
    let sink = PhotonPublishSink::new(photon.clone());
    let backend = Arc::new(RecordingBackend::default());
    let consumer = BatchedStorageConsumer::new(recording_router(&backend), slow_config());
    let sub = subscribe(&photon, &all_shards(), &consumer, "gated").await;
    for i in 0..6 {
        sink.record_counter(&format!("gated_{i}"), &[], 1);
    }
    wait_for("publish", || sink.stats().published == 6).await;
    quiet_window().await;

    // Ingested but still queued in an unwritten batch (60s batch_wait): nothing to save yet.
    sub.checkpoint().await.unwrap();
    assert_eq!(checkpointed(&photon, "gated").await, 0);

    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while consumer.stats().rows_written < 6 {
        assert!(tokio::time::Instant::now() < deadline, "writer stalled");
        consumer.flush().await.unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    sub.checkpoint().await.unwrap();
    assert_eq!(checkpointed(&photon, "gated").await, 6);
}
