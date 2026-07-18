//! ClickHouse metrics storage adapter.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use spectra_backend_remote_common::RemoteMetricsBackend;
use spectra_core::{
    MetricPoint, MetricWriteRow, MetricsQueryRange, MetricsStorageBackend, Result,
    StorageEngineType,
};

/// Remote ClickHouse metrics storage.
///
/// [`connect`](Self::connect) opens the ClickHouse client and creates the Spectra metrics table
/// when needed. A running Spectra instance requires both this backend and
/// `ClickHouseEventsBackend`; remote storage does not use `.embedded()`.
///
/// # Examples
///
/// Facade wiring through `Spectra::builder()` (requires the `spectra` crate with the
/// `clickhouse` feature):
///
/// ```ignore
/// use std::sync::Arc;
/// use spectra::{ClickHouseEventsBackend, ClickHouseMetricsBackend, Spectra};
///
/// # async fn start() -> spectra::Result<()> {
/// let url = "http://127.0.0.1:8123";
/// let spectra = Spectra::builder()
///     .metrics_backend(Arc::new(ClickHouseMetricsBackend::connect(url).await?))
///     .events_backend(Arc::new(ClickHouseEventsBackend::connect(url).await?))
///     .build()?;
/// # let _ = spectra;
/// # Ok(())
/// # }
/// ```
pub struct ClickHouseMetricsBackend(RemoteMetricsBackend);

impl ClickHouseMetricsBackend {
    /// Connect to ClickHouse over HTTP or native protocol and ensure metrics tables exist.
    ///
    /// `url` is typically `http://host:8123`. The call is async and executes DDL, so the
    /// ClickHouse credentials need table-creation permission.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # async fn example() -> spectra_core::Result<()> {
    /// use spectra_backend_clickhouse::ClickHouseMetricsBackend;
    ///
    /// let backend = ClickHouseMetricsBackend::connect("http://127.0.0.1:8123").await?;
    /// # let _ = backend;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn connect(url: &str) -> Result<Self> {
        Ok(Self(
            RemoteMetricsBackend::connect(
                url,
                StorageEngineType::ClickHouse,
                &crate::ddl::metrics_ddl(),
            )
            .await?,
        ))
    }

    /// In-memory stub that preserves the ClickHouse engine type for storage-contract tests.
    ///
    /// # Examples
    ///
    /// ```
    /// use spectra_backend_clickhouse::ClickHouseMetricsBackend;
    /// use spectra_core::{MetricsStorageBackend, StorageEngineType};
    ///
    /// let backend = ClickHouseMetricsBackend::in_memory_stub();
    /// assert_eq!(backend.engine_type(), StorageEngineType::ClickHouse);
    /// ```
    pub fn in_memory_stub() -> Self {
        Self(RemoteMetricsBackend::in_memory_for_test(
            StorageEngineType::ClickHouse,
        ))
    }

    /// In-memory stub for unit tests in this crate.
    #[cfg(test)]
    pub fn in_memory_for_test() -> Self {
        Self::in_memory_stub()
    }
}

#[async_trait]
impl MetricsStorageBackend for ClickHouseMetricsBackend {
    fn engine_type(&self) -> StorageEngineType {
        self.0.engine_type()
    }

    async fn record_counter(
        &self,
        name: &str,
        labels: &Value,
        delta: i64,
        ts: DateTime<Utc>,
    ) -> Result<()> {
        self.0.record_counter(name, labels, delta, ts).await
    }

    async fn record_gauge(
        &self,
        name: &str,
        labels: &Value,
        value: f64,
        ts: DateTime<Utc>,
    ) -> Result<()> {
        self.0.record_gauge(name, labels, value, ts).await
    }

    async fn record_metrics_batch(&self, rows: &[MetricWriteRow]) -> Result<()> {
        self.0.record_metrics_batch(rows).await
    }

    async fn query_range(&self, query: MetricsQueryRange) -> Result<Vec<MetricPoint>> {
        self.0.query_range(query).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use serde_json::json;

    #[tokio::test]
    async fn clickhouse_metrics_roundtrip_in_memory() {
        let backend = ClickHouseMetricsBackend::in_memory_for_test();
        let ts = Utc::now();
        backend
            .record_counter("hits", &json!({}), 2, ts)
            .await
            .expect("write");
        let points = backend
            .query_range(MetricsQueryRange {
                metric_name: "hits".into(),
                start: ts - Duration::seconds(1),
                end: ts + Duration::seconds(1),
                label_matchers: vec![],
            })
            .await
            .expect("query");
        assert_eq!(points.len(), 1);
    }

    #[tokio::test]
    #[ignore = "requires SPECTRA_CLICKHOUSE_URL"]
    async fn clickhouse_metrics_integration() {
        let url = std::env::var("SPECTRA_CLICKHOUSE_URL").expect("SPECTRA_CLICKHOUSE_URL");
        let backend = ClickHouseMetricsBackend::connect(&url)
            .await
            .expect("connect");
        let ts = Utc::now();
        backend
            .record_counter("integration_hits", &json!({}), 1, ts)
            .await
            .expect("write");
        let points = backend
            .query_range(MetricsQueryRange {
                metric_name: "integration_hits".into(),
                start: ts - Duration::seconds(5),
                end: ts + Duration::seconds(5),
                label_matchers: vec![],
            })
            .await
            .expect("query");
        assert!(!points.is_empty());
    }
}
