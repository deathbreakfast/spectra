use std::sync::Arc;

use anyhow::Result;
use spectra::spectra_core::{EventStorageBackend, MetricsStorageBackend};
use spectra::{MemEventsBackend, MemMetricsBackend, SqliteEventsBackend, SqliteMetricsBackend};

use crate::fixtures::TempStore;
use crate::matrix::StorageAdapter;

#[cfg(all(feature = "clickhouse", not(feature = "tensorbase")))]
use crate::shard::clickhouse_url_sharded;
#[cfg(all(feature = "tensorbase", not(feature = "clickhouse")))]
use crate::shard::tensorbase_url_sharded;
#[cfg(all(feature = "clickhouse", feature = "tensorbase"))]
use crate::shard::{clickhouse_url_sharded, tensorbase_url_sharded};

/// Metrics and events backends kept alive for the duration of a matrix run.
pub struct BackendPair {
    /// Shared metrics storage backend.
    pub metrics: Arc<dyn MetricsStorageBackend>,
    /// Shared events storage backend.
    pub events: Arc<dyn EventStorageBackend>,
    /// Keeps sqlite temp files alive when storage is [`StorageAdapter::Sqlite`].
    pub _sqlite_store: Option<TempStore>,
}

/// Construct storage backends for one matrix storage adapter.
pub async fn build_backends(storage: StorageAdapter, slug: &str) -> Result<BackendPair> {
    match storage {
        StorageAdapter::Mem => Ok(BackendPair {
            metrics: Arc::new(MemMetricsBackend::new()),
            events: Arc::new(MemEventsBackend::new()),
            _sqlite_store: None,
        }),
        StorageAdapter::Sqlite => {
            let store = TempStore::new(slug)?;
            let metrics = SqliteMetricsBackend::new(&store.metrics_path)
                .map_err(|e| anyhow::anyhow!("sqlite metrics: {e}"))?;
            let events = SqliteEventsBackend::new(&store.events_path)
                .map_err(|e| anyhow::anyhow!("sqlite events: {e}"))?;
            Ok(BackendPair {
                metrics: Arc::new(metrics),
                events: Arc::new(events),
                _sqlite_store: Some(store),
            })
        }
        #[cfg(feature = "tensorbase")]
        StorageAdapter::TensorBase => {
            use spectra::{TensorBaseEventsBackend, TensorBaseMetricsBackend};
            let url = tensorbase_url_sharded()?;
            let metrics = TensorBaseMetricsBackend::connect(&url)
                .await
                .map_err(|e| anyhow::anyhow!("tensorbase metrics: {e}"))?;
            let events = TensorBaseEventsBackend::connect(&url)
                .await
                .map_err(|e| anyhow::anyhow!("tensorbase events: {e}"))?;
            Ok(BackendPair {
                metrics: Arc::new(metrics),
                events: Arc::new(events),
                _sqlite_store: None,
            })
        }
        #[cfg(not(feature = "tensorbase"))]
        StorageAdapter::TensorBase => {
            anyhow::bail!("tensorbase feature not enabled on spectra-testkit")
        }
        #[cfg(feature = "clickhouse")]
        StorageAdapter::ClickHouse => {
            use spectra::{ClickHouseEventsBackend, ClickHouseMetricsBackend};
            let url = clickhouse_url_sharded()?;
            let metrics = ClickHouseMetricsBackend::connect(&url)
                .await
                .map_err(|e| anyhow::anyhow!("clickhouse metrics: {e}"))?;
            let events = ClickHouseEventsBackend::connect(&url)
                .await
                .map_err(|e| anyhow::anyhow!("clickhouse events: {e}"))?;
            Ok(BackendPair {
                metrics: Arc::new(metrics),
                events: Arc::new(events),
                _sqlite_store: None,
            })
        }
        #[cfg(not(feature = "clickhouse"))]
        StorageAdapter::ClickHouse => {
            anyhow::bail!("clickhouse feature not enabled on spectra-testkit")
        }
    }
}
