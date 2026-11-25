//! Photon consumer-group subscription that feeds a [`BatchedStorageConsumer`].

use std::collections::HashMap;
use std::ops::RangeInclusive;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use photon::Photon;
use tokio::task::JoinHandle;

use crate::consumer::{BatchedStorageConsumer, Cursor};
use crate::topic::{SpectraEmit, SHARD_COUNT, TOPIC_NAME};

/// Shards `range` out of [`SHARD_COUNT`], for splitting the topic between processes.
///
/// Give each process in the group a disjoint range (for example `0..=7` and `8..=15`).
/// Out-of-range ids are dropped.
pub fn shard_range(range: RangeInclusive<u32>) -> Vec<u32> {
    range.filter(|id| *id < SHARD_COUNT).collect()
}

/// Every shard of the topic, for a single-process group.
pub fn all_shards() -> Vec<u32> {
    (0..SHARD_COUNT).collect()
}

/// Where a [`GroupSubscription`] saves its position in Photon.
#[derive(Debug, Clone)]
pub struct DurableConfig {
    /// Checkpoint name; one name per consumer group member. Restarting with the same name
    /// resumes after the last checkpoint.
    pub subscription: String,
    /// How often written positions are saved. Default `1s`.
    pub checkpoint_interval: Duration,
}

impl DurableConfig {
    /// Checkpoints under `subscription` every second.
    pub fn new(subscription: impl Into<String>) -> Self {
        Self {
            subscription: subscription.into(),
            checkpoint_interval: Duration::from_secs(1),
        }
    }
}

struct Checkpointer {
    photon: Photon,
    subscription: String,
    consumer: BatchedStorageConsumer,
    saved: parking_lot::Mutex<HashMap<String, i64>>,
}

impl Checkpointer {
    /// Save every written position that moved since the last save.
    async fn save(&self) -> photon::Result<()> {
        let written = self.consumer.written_cursors();
        for (shard_key, seq) in written {
            if self.saved.lock().get(&shard_key) == Some(&seq) {
                continue;
            }
            self.photon
                .set_checkpoint(&self.subscription, TOPIC_NAME, Some(&shard_key), seq)
                .await?;
            self.saved.lock().insert(shard_key, seq);
        }
        Ok(())
    }
}

/// Running subscription over the assigned shards. Dropping it stops the subscription.
pub struct GroupSubscription {
    task: JoinHandle<()>,
    checkpoint_task: Option<JoinHandle<()>>,
    checkpointer: Option<Arc<Checkpointer>>,
    decode_errors: Arc<AtomicU64>,
}

impl GroupSubscription {
    /// Subscribe to `shards` of the Spectra topic and ingest each event into `consumer`.
    ///
    /// Events that do not decode are logged and counted ([`Self::decode_errors`]); the stream
    /// keeps going. The stream ends when Photon closes it or [`Self::stop`] is called. No
    /// position is saved, so where a restart picks up depends on the Photon storage port;
    /// use [`Self::start_durable`] to resume where the last run stopped.
    pub fn start(photon: &Photon, shards: &[u32], consumer: BatchedStorageConsumer) -> Self {
        let stream = photon.subscribe_consumer_group(TOPIC_NAME, shards, HashMap::new());
        Self::spawn(stream, consumer, None)
    }

    /// Like [`Self::start`], but resumes after the position saved under
    /// `durable.subscription` and keeps saving it as batches are written.
    ///
    /// A shard with no saved position replays from the start of what the Photon port still
    /// holds. A position is saved only after the batch holding its event has been written
    /// (or failed and been counted), so a crash re-delivers at most the unsaved tail:
    /// delivery is at-least-once across restarts. Use one durable subscription per
    /// [`BatchedStorageConsumer`].
    ///
    /// Call [`Self::checkpoint`] after [`BatchedStorageConsumer::flush`] before shutting down,
    /// or the last interval's progress is replayed on the next start.
    ///
    /// # Errors
    ///
    /// Returns the Photon error when a saved position cannot be read.
    pub async fn start_durable(
        photon: &Photon,
        shards: &[u32],
        consumer: BatchedStorageConsumer,
        durable: DurableConfig,
    ) -> photon::Result<Self> {
        let mut after = HashMap::new();
        let mut saved = HashMap::new();
        for &shard in shards {
            let key = shard_key(shard);
            let seq = photon
                .get_checkpoint_seq(&durable.subscription, TOPIC_NAME, Some(&key))
                .await?
                .unwrap_or(0);
            saved.insert(key, seq);
            after.insert(shard, Some(seq));
        }
        let stream = photon.subscribe_consumer_group(TOPIC_NAME, shards, after);
        let checkpointer = Arc::new(Checkpointer {
            photon: photon.clone(),
            subscription: durable.subscription,
            consumer: consumer.clone(),
            saved: parking_lot::Mutex::new(saved),
        });
        let mut this = Self::spawn(stream, consumer, Some(&checkpointer));
        let ticker = Arc::clone(&checkpointer);
        let interval = durable.checkpoint_interval;
        this.checkpoint_task = Some(tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;
                if let Err(err) = ticker.save().await {
                    tracing::warn!(error = %err, "spectra photon checkpoint failed");
                }
            }
        }));
        this.checkpointer = Some(checkpointer);
        Ok(this)
    }

    fn spawn(
        mut stream: std::pin::Pin<
            Box<dyn futures::Stream<Item = photon::Result<photon::Event>> + Send>,
        >,
        consumer: BatchedStorageConsumer,
        durable: Option<&Arc<Checkpointer>>,
    ) -> Self {
        let track = durable.is_some();
        let decode_errors = Arc::new(AtomicU64::new(0));
        let errors = Arc::clone(&decode_errors);
        let task = tokio::spawn(async move {
            // Replay and live delivery can overlap on the in-memory port; skip repeats.
            let mut last_seq: HashMap<String, i64> = HashMap::new();
            while let Some(item) = stream.next().await {
                let event = match item {
                    Ok(event) => event,
                    Err(err) => {
                        tracing::warn!(error = %err, "photon spectra stream error");
                        continue;
                    }
                };
                let cursor = match (track, event.topic_key.clone()) {
                    (true, Some(shard_key)) => {
                        let seen = last_seq.entry(shard_key.clone()).or_insert(0);
                        if event.seq <= *seen {
                            continue;
                        }
                        *seen = event.seq;
                        Some(Cursor {
                            shard_key,
                            seq: event.seq,
                        })
                    }
                    _ => None,
                };
                match serde_json::from_value::<SpectraEmit>(event.payload_json) {
                    Ok(emit) => {
                        let sent = match cursor {
                            Some(cursor) => consumer.ingest_at(emit.envelope, cursor).await,
                            None => consumer.ingest(emit.envelope).await,
                        };
                        if sent.is_err() {
                            return;
                        }
                    }
                    Err(err) => {
                        errors.fetch_add(1, Ordering::Relaxed);
                        tracing::warn!(error = %err, "undecodable spectra emit; skipping");
                    }
                }
            }
        });
        Self {
            task,
            checkpoint_task: None,
            checkpointer: None,
            decode_errors,
        }
    }

    /// Save the written positions now. A no-op on a subscription from [`Self::start`].
    ///
    /// Positions cover batches the writer has finished; call
    /// [`BatchedStorageConsumer::flush`] first to include everything ingested so far.
    ///
    /// # Errors
    ///
    /// Returns the Photon error when a checkpoint cannot be saved.
    pub async fn checkpoint(&self) -> photon::Result<()> {
        match &self.checkpointer {
            Some(checkpointer) => checkpointer.save().await,
            None => Ok(()),
        }
    }

    /// Events skipped because they did not decode.
    pub fn decode_errors(&self) -> u64 {
        self.decode_errors.load(Ordering::Relaxed)
    }

    /// Stop reading new events. Queued envelopes still need [`BatchedStorageConsumer::flush`].
    pub fn stop(&self) {
        self.task.abort();
        if let Some(task) = &self.checkpoint_task {
            task.abort();
        }
    }
}

impl Drop for GroupSubscription {
    fn drop(&mut self) {
        self.stop();
    }
}

fn shard_key(shard: u32) -> String {
    photon::shard_router::shard_storage_key(shard)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shard_range_clamps_to_topic_shards() {
        assert_eq!(shard_range(14..=20), vec![14, 15]);
        assert_eq!(all_shards().len(), SHARD_COUNT as usize);
    }
}
