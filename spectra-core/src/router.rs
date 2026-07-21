//! Resolves event tables and metric names to storage backends for reads and writes.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use parking_lot::RwLock;

use crate::error::Result;
use crate::query::EventAggregateResult;
use crate::storage::{
    EventsAggregateFilter, EventsQueryFilter, MetricsQueryRange, NoOpEventBackend,
    NoOpMetricsBackend, SharedEventBackend, SharedMetricsBackend,
};

static GLOBAL_ROUTER: OnceLock<Arc<SpectraRouter>> = OnceLock::new();

/// Resolves event tables and metric names to storage backends.
///
/// A runtime installs default metrics and events backends, then registers schema-specific
/// routes. Queries use a named route when present and otherwise fall back to the corresponding
/// default backend. Most applications access this through `Spectra::router()`.
///
/// # Examples
///
/// ```no_run
/// use chrono::{Duration, Utc};
/// use spectra_core::{MetricsQueryRange, SpectraRouter};
///
/// # async fn example() -> spectra_core::Result<()> {
/// let router = SpectraRouter::new();
/// let now = Utc::now();
/// let points = router.query_metrics(MetricsQueryRange {
///     metric_name: "cache_hits".into(),
///     start: now - Duration::minutes(5),
///     end: now,
///     label_matchers: vec![],
/// }).await?;
///
/// // A new router uses no-op defaults, so no rows are returned.
/// assert!(points.is_empty());
/// # Ok(())
/// # }
/// ```
pub struct SpectraRouter {
    events: RwLock<HashMap<String, SharedEventBackend>>,
    metrics: RwLock<HashMap<String, SharedMetricsBackend>>,
    default_events: SharedEventBackend,
    default_metrics: SharedMetricsBackend,
}

impl Default for SpectraRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl SpectraRouter {
    /// Creates a router with no-op default backends.
    pub fn new() -> Self {
        Self {
            events: RwLock::new(HashMap::new()),
            metrics: RwLock::new(HashMap::new()),
            default_events: Arc::new(NoOpEventBackend),
            default_metrics: Arc::new(NoOpMetricsBackend),
        }
    }

    /// Create a router with default backends for unregistered schema names.
    pub fn with_defaults(
        default_metrics: SharedMetricsBackend,
        default_events: SharedEventBackend,
    ) -> Self {
        Self {
            events: RwLock::new(HashMap::new()),
            metrics: RwLock::new(HashMap::new()),
            default_events,
            default_metrics,
        }
    }

    /// Registers a storage backend for an event table.
    pub fn register_event_backend(&self, table: impl Into<String>, backend: SharedEventBackend) {
        self.events.write().insert(table.into(), backend);
    }

    /// Registers a storage backend for a metric family.
    pub fn register_metrics_backend(&self, name: impl Into<String>, backend: SharedMetricsBackend) {
        self.metrics.write().insert(name.into(), backend);
    }

    /// Resolves the backend for an event table, falling back to the default.
    pub fn resolve_event(&self, table: &str) -> SharedEventBackend {
        self.events
            .read()
            .get(table)
            .cloned()
            .unwrap_or_else(|| Arc::clone(&self.default_events))
    }

    /// Resolves the backend for a metric family, falling back to the default.
    pub fn resolve_metrics(&self, name: &str) -> SharedMetricsBackend {
        self.metrics
            .read()
            .get(name)
            .cloned()
            .unwrap_or_else(|| Arc::clone(&self.default_metrics))
    }

    /// Queries event rows through the resolved backend.
    pub async fn query_events(
        &self,
        filter: EventsQueryFilter,
    ) -> Result<Vec<crate::storage::EventRow>> {
        let backend = self.resolve_event(&filter.table);
        backend.query_rows(filter).await
    }

    /// Queries metric points through the resolved backend.
    pub async fn query_metrics(
        &self,
        query: MetricsQueryRange,
    ) -> Result<Vec<crate::storage::MetricPoint>> {
        let backend = self.resolve_metrics(&query.metric_name);
        backend.query_range(query).await
    }

    /// Queries aggregated chart data through the resolved backend.
    pub async fn query_event_aggregate(
        &self,
        filter: EventsAggregateFilter,
    ) -> Result<EventAggregateResult> {
        let table = filter.table.clone();
        let backend = self.resolve_event(&table);
        backend.query_aggregate(filter).await
    }

    /// Installs a router as the process-global instance (call once).
    pub fn set_global(router: Arc<Self>) {
        let _ = GLOBAL_ROUTER.set(router);
    }

    /// Returns the process-global router (panics if not installed).
    ///
    /// Prefer [`Self::try_global`] in library code; use this in hosts/examples that have
    /// already called [`Self::set_global`].
    pub fn global() -> Arc<Self> {
        // Process invariant: hosts must install before calling.
        #[allow(clippy::expect_used)]
        GLOBAL_ROUTER
            .get()
            .cloned()
            .expect("SpectraRouter::set_global was not called")
    }

    /// Returns the process-global router if installed.
    pub fn try_global() -> Option<Arc<Self>> {
        GLOBAL_ROUTER.get().cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{EventStorageBackend, EventsQueryFilter};
    use async_trait::async_trait;
    use chrono::Utc;
    use serde_json::json;
    use std::sync::atomic::{AtomicU32, Ordering};

    struct CountingEventBackend {
        appends: AtomicU32,
    }

    #[async_trait]
    impl EventStorageBackend for CountingEventBackend {
        fn engine_type(&self) -> crate::storage::StorageEngineType {
            crate::storage::StorageEngineType::NoOp
        }

        async fn append_row(
            &self,
            _: &str,
            _: &serde_json::Value,
            _: chrono::DateTime<Utc>,
            _: Option<&str>,
        ) -> crate::error::Result<()> {
            self.appends.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[test]
    fn router_noop_default() {
        let router = SpectraRouter::new();
        let rt = tokio::runtime::Runtime::new().expect("runtime");
        let rows = rt
            .block_on(router.query_events(EventsQueryFilter {
                table: "missing".into(),
                ..Default::default()
            }))
            .expect("query");
        assert!(rows.is_empty());
    }

    #[test]
    fn router_resolve_event_backend() {
        let router = SpectraRouter::new();
        let counting = Arc::new(CountingEventBackend {
            appends: AtomicU32::new(0),
        });
        let backend: SharedEventBackend = Arc::clone(&counting) as SharedEventBackend;
        router.register_event_backend("t1", backend);
        let resolved = router.resolve_event("t1");
        let rt = tokio::runtime::Runtime::new().expect("runtime");
        rt.block_on(async {
            resolved
                .append_row("t1", &json!({}), Utc::now(), None)
                .await
                .expect("append");
        });
        assert_eq!(counting.appends.load(Ordering::SeqCst), 1);
    }
}
