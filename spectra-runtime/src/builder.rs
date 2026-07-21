//! Build and install a Spectra runtime with injected storage backends.

use std::sync::Arc;

use spectra_core::{
    install_config, set_sink, LoggingKind, SchemaRegistry, SharedEventBackend,
    SharedMetricsBackend, SpectraConfig, SpectraRouter, SpectraSink,
};

use crate::persist_config::PersistConfig;
use crate::persist_sink::{PersistHandle, StoragePersistSink};

#[cfg(feature = "telemetry-console")]
use crate::async_writer::OffThreadSpectraSink;
#[cfg(feature = "telemetry-console")]
use spectra_core::NdjsonFileSink;
#[cfg(feature = "telemetry-console")]
use std::path::Path;

/// Handle to an installed process-wide Spectra runtime.
///
/// Keep this value to access the configured [`SpectraRouter`]. Building installs the emit
/// configuration, router, and sink globally; applications should build once during startup.
///
/// # Examples
///
/// ```no_run
/// use std::sync::Arc;
/// use spectra_backend_mem::{MemEventsBackend, MemMetricsBackend};
/// use spectra_runtime::Spectra;
///
/// # fn example() -> spectra_core::Result<()> {
/// let spectra = Spectra::builder()
///     .metrics_backend(Arc::new(MemMetricsBackend::new()))
///     .events_backend(Arc::new(MemEventsBackend::new()))
///     .embedded()
///     .build()?;
///
/// let router = spectra.router();
/// # let _ = router;
/// # Ok(())
/// # }
/// ```
pub struct Spectra {
    router: Arc<SpectraRouter>,
    persist: Option<PersistHandle>,
    /// Topology marker from [`SpectraBuilder::embedded`] (does not select backends).
    embedded: bool,
}

impl Spectra {
    /// Returns the installed global router (metrics/events query and backend resolution).
    pub fn router(&self) -> Arc<SpectraRouter> {
        Arc::clone(&self.router)
    }

    /// Whether this runtime was marked as in-process embedded topology.
    ///
    /// Set by [`SpectraBuilder::embedded`]. This is a topology marker only — it does not
    /// select, validate, or change storage backends.
    pub fn is_embedded(&self) -> bool {
        self.embedded
    }

    /// Wait until every persist job enqueued before this call has been written to storage.
    ///
    /// Use after `try_record_*_now` / generated helpers when a script must exit only after
    /// durable writes complete. No-op when persist was disabled at build time.
    pub async fn flush_persist(&self) -> spectra_core::Result<()> {
        match &self.persist {
            Some(handle) => handle.flush().await,
            None => Ok(()),
        }
    }

    /// Start building a process-scoped Spectra runtime with explicit storage injection.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use std::sync::Arc;
    /// # fn demo() -> spectra_core::Result<()> {
    /// use spectra_backend_mem::{MemEventsBackend, MemMetricsBackend};
    /// use spectra_runtime::Spectra;
    ///
    /// let _spectra = Spectra::builder()
    ///     .metrics_backend(Arc::new(MemMetricsBackend::new()))
    ///     .events_backend(Arc::new(MemEventsBackend::new()))
    ///     .embedded()
    ///     .build()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn builder() -> SpectraBuilder {
        SpectraBuilder::new()
    }
}

/// Configures and installs [`Spectra`] with explicit storage adapter injection.
///
/// Both metrics and events backends are required. Storage persistence is enabled by default;
/// an optional [`SpectraSink`] can receive the same emits before they are queued for persistence.
///
/// | Mode | Calls |
/// |------|-------|
/// | Direct persist | backends + `.build()` |
/// | Dual-path | `.sink(transport).build()` |
/// | **Publisher** (distributed) | `.sink(transport).persist_disabled().build()` |
///
/// Publisher processes publish through the sink; consumers subscribe on the host bus and write
/// storage. See the `spectra` crate **Getting started → Mode 2**.
///
/// # Examples
///
/// ```no_run
/// use std::sync::Arc;
/// use spectra_backend_mem::{MemEventsBackend, MemMetricsBackend};
/// use spectra_core::{RecordingSink, SpectraSink};
/// use spectra_runtime::Spectra;
///
/// # fn example() -> spectra_core::Result<()> {
/// let transport = Arc::new(RecordingSink::new());
/// let spectra = Spectra::builder()
///     .metrics_backend(Arc::new(MemMetricsBackend::new()))
///     .events_backend(Arc::new(MemEventsBackend::new()))
///     .sink(Arc::clone(&transport) as Arc<dyn SpectraSink>)
///     .embedded()
///     .build()?;
/// # let _ = spectra;
/// # Ok(())
/// # }
/// ```
pub struct SpectraBuilder {
    metrics: Option<SharedMetricsBackend>,
    events: Option<SharedEventBackend>,
    config: Option<SpectraConfig>,
    transport_sink: Option<Arc<dyn SpectraSink>>,
    embedded: bool,
    persist: bool,
    persist_config: PersistConfig,
}

impl Default for SpectraBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl SpectraBuilder {
    /// New builder with async storage persist enabled and no backends configured yet.
    pub fn new() -> Self {
        Self {
            metrics: None,
            events: None,
            config: None,
            transport_sink: None,
            embedded: false,
            persist: true,
            persist_config: PersistConfig::default(),
        }
    }

    /// Register the metrics storage backend (required before [`Self::build`]).
    pub fn metrics_backend(mut self, backend: SharedMetricsBackend) -> Self {
        self.metrics = Some(backend);
        self
    }

    /// Register the events storage backend (required before [`Self::build`]).
    pub fn events_backend(mut self, backend: SharedEventBackend) -> Self {
        self.events = Some(backend);
        self
    }

    /// Override emit policy and feature flags (defaults to [`SpectraConfig::from_env`]).
    pub fn config(mut self, config: SpectraConfig) -> Self {
        self.config = Some(config);
        self
    }

    /// Configure the L2 persist queue and batch settings (ignored when persist is disabled).
    ///
    /// Defaults: `queue_max=8192`, `batch_max=32`, `batch_wait=5ms`, `batch_enabled=true`.
    /// Raise `batch_max` for high-throughput DW ingest.
    pub fn persist(mut self, config: PersistConfig) -> Self {
        self.persist_config = config;
        self
    }

    /// Optional transport or telemetry sink (invoked before async storage persist when enabled).
    ///
    /// Use this for the **publisher** side of distributed ingest: your [`SpectraSink`]
    /// publishes emits onto a bus (for example Photon). Combine with
    /// [`Self::persist_disabled`] when the publisher must not write storage.
    ///
    /// Without `persist_disabled`, the runtime installs a persist wrapper that calls your
    /// sink first, then queues async storage writes (dual-path).
    ///
    /// Implement [`SpectraSink`](spectra_core::SpectraSink) in your application or use
    /// [`RecordingSink`](spectra_core::RecordingSink) in tests. See the `spectra` crate
    /// **Getting started → Mode 2**.
    pub fn sink(mut self, sink: Arc<dyn SpectraSink>) -> Self {
        self.transport_sink = Some(sink);
        self
    }

    /// Mark in-process embedded topology (in-host storage backends).
    ///
    /// This sets a topology flag retained on the returned [`Spectra`] handle
    /// ([`Spectra::is_embedded`]). It does **not** select or validate backends — you still
    /// inject storage via [`Self::metrics_backend`] / [`Self::events_backend`].
    pub fn embedded(mut self) -> Self {
        self.embedded = true;
        self
    }

    /// Disable async storage persist (only the transport sink receives emits).
    ///
    /// **Publisher / distributed mode.** Pair with [`Self::sink`]: writers publish through the
    /// sink; separate consumer processes subscribe and persist. Requires a sink — building
    /// with persist disabled and no sink returns an error.
    pub fn persist_disabled(mut self) -> Self {
        self.persist = false;
        self
    }

    /// Attach off-thread NDJSON + optional console mirror telemetry (`telemetry-console` feature).
    ///
    /// Writes `{dir}/metrics.ndjson` and `{dir}/events.ndjson`.
    #[cfg(feature = "telemetry-console")]
    pub fn telemetry_ndjson(mut self, dir: impl AsRef<Path>) -> spectra_core::Result<Self> {
        let dir = dir.as_ref();
        let ndjson = NdjsonFileSink::new(dir.join("metrics.ndjson"), dir.join("events.ndjson"))?;
        let sink = Arc::new(OffThreadSpectraSink::new(ndjson));
        self.transport_sink = Some(sink);
        Ok(self)
    }

    /// Install global config, router, and sink; returns a handle to the running runtime.
    pub fn build(self) -> spectra_core::Result<Spectra> {
        let metrics = self
            .metrics
            .ok_or_else(|| spectra_core::Error::config("metrics_backend is required"))?;
        let events = self
            .events
            .ok_or_else(|| spectra_core::Error::config("events_backend is required"))?;

        let router = build_router(metrics, events);
        let router = Arc::new(router);
        SpectraRouter::set_global(Arc::clone(&router));

        let config = self.config.unwrap_or_else(SpectraConfig::from_env);
        install_config(config);

        let persist_config = self.persist_config;
        let (installed, persist_handle): (Arc<dyn SpectraSink>, Option<PersistHandle>) =
            match (self.persist, self.transport_sink) {
                (true, Some(inner)) => {
                    let sink = StoragePersistSink::with_config(
                        Arc::clone(&router),
                        Some(inner),
                        persist_config,
                    );
                    let handle = sink.handle();
                    (Arc::new(sink), Some(handle))
                }
                (true, None) => {
                    let sink =
                        StoragePersistSink::new_with_config(Arc::clone(&router), persist_config);
                    let handle = sink.handle();
                    (Arc::new(sink), Some(handle))
                }
                (false, Some(inner)) => (inner, None),
                (false, None) => {
                    return Err(spectra_core::Error::config(
                        "SpectraBuilder: persist is disabled but no sink was configured; \
                     call .sink(...) or enable persist",
                    ));
                }
            };
        set_sink(installed);

        Ok(Spectra {
            router,
            persist: persist_handle,
            embedded: self.embedded,
        })
    }
}

fn build_router(metrics: SharedMetricsBackend, events: SharedEventBackend) -> SpectraRouter {
    let router = SpectraRouter::with_defaults(Arc::clone(&metrics), Arc::clone(&events));
    for name in SchemaRegistry::global().list_schemas() {
        let Some(meta) = SchemaRegistry::global().get_schema(name) else {
            continue;
        };
        match meta.logging_kind {
            LoggingKind::Event => {
                router.register_event_backend(name, Arc::clone(&events));
            }
            LoggingKind::Metric => {
                router.register_metrics_backend(name, Arc::clone(&metrics));
            }
        }
    }
    router
}

#[cfg(test)]
mod tests {
    use super::*;
    use spectra_backend_mem::{MemEventsBackend, MemMetricsBackend};
    use spectra_core::{try_record_counter_now, NoOpSink, RecordingSink, SpectraConfig};

    static RUNTIME_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    fn mem_backends() -> (SharedMetricsBackend, SharedEventBackend) {
        (
            Arc::new(MemMetricsBackend::new()),
            Arc::new(MemEventsBackend::new()),
        )
    }

    async fn with_isolated_runtime<F, Fut>(f: F)
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        let _g = RUNTIME_TEST_LOCK.lock().await;
        spectra_core::install_config(SpectraConfig {
            enabled: false,
            ..Default::default()
        });
        f().await;
        spectra_core::set_sink(Arc::new(NoOpSink));
    }

    #[tokio::test]
    async fn embedded_flag_retained_on_handle() {
        with_isolated_runtime(|| async {
            let (metrics, events) = mem_backends();
            let spectra = Spectra::builder()
                .metrics_backend(metrics)
                .events_backend(events)
                .embedded()
                .build()
                .expect("build");
            assert!(spectra.is_embedded());
        })
        .await;
    }

    #[tokio::test]
    async fn builder_installs_persist_sink() {
        with_isolated_runtime(|| async {
            let (metrics, events) = mem_backends();

            let spectra = Spectra::builder()
                .metrics_backend(Arc::clone(&metrics))
                .events_backend(Arc::clone(&events))
                .embedded()
                .build()
                .expect("build");

            try_record_counter_now("test_counter", &[], 1);
            spectra.flush_persist().await.expect("flush");

            let points = spectra
                .router()
                .query_metrics(spectra_core::MetricsQueryRange {
                    metric_name: "test_counter".into(),
                    start: chrono::Utc::now() - chrono::Duration::seconds(5),
                    end: chrono::Utc::now() + chrono::Duration::seconds(1),
                    label_matchers: vec![],
                })
                .await
                .expect("query");
            assert_eq!(points.len(), 1);
        })
        .await;
    }

    #[tokio::test]
    async fn transport_and_persist_both_receive_emits() {
        with_isolated_runtime(|| async {
            let (metrics, events) = mem_backends();
            let transport = Arc::new(RecordingSink::new());

            let spectra = Spectra::builder()
                .metrics_backend(Arc::clone(&metrics))
                .events_backend(Arc::clone(&events))
                .sink(Arc::clone(&transport) as Arc<dyn SpectraSink>)
                .embedded()
                .build()
                .expect("build");

            try_record_counter_now("dual_path_counter", &[], 1);
            spectra.flush_persist().await.expect("flush");

            assert_eq!(transport.counters().len(), 1);
            let points = spectra
                .router()
                .query_metrics(spectra_core::MetricsQueryRange {
                    metric_name: "dual_path_counter".into(),
                    start: chrono::Utc::now() - chrono::Duration::seconds(5),
                    end: chrono::Utc::now() + chrono::Duration::seconds(1),
                    label_matchers: vec![],
                })
                .await
                .expect("query");
            assert_eq!(points.len(), 1);
        })
        .await;
    }

    #[tokio::test]
    async fn transport_only_skips_storage() {
        with_isolated_runtime(|| async {
            let (metrics, events) = mem_backends();
            let transport = Arc::new(RecordingSink::new());

            let spectra = Spectra::builder()
                .metrics_backend(Arc::clone(&metrics))
                .events_backend(Arc::clone(&events))
                .sink(Arc::clone(&transport) as Arc<dyn SpectraSink>)
                .persist_disabled()
                .build()
                .expect("build");

            try_record_counter_now("transport_only_counter", &[], 1);
            spectra.flush_persist().await.expect("flush noop");
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;

            assert_eq!(transport.counters().len(), 1);
            let points = spectra
                .router()
                .query_metrics(spectra_core::MetricsQueryRange {
                    metric_name: "transport_only_counter".into(),
                    start: chrono::Utc::now() - chrono::Duration::seconds(5),
                    end: chrono::Utc::now() + chrono::Duration::seconds(1),
                    label_matchers: vec![],
                })
                .await
                .expect("query");
            assert!(points.is_empty());
        })
        .await;
    }

    #[tokio::test]
    async fn persist_disabled_without_sink_errors() {
        let _g = RUNTIME_TEST_LOCK.lock().await;
        let (metrics, events) = mem_backends();
        let result = Spectra::builder()
            .metrics_backend(metrics)
            .events_backend(events)
            .persist_disabled()
            .build();
        assert!(result.is_err());
        assert!(result.err().expect("err").to_string().contains("no sink"));
    }

    #[tokio::test]
    async fn persist_config_batch_and_flush() {
        with_isolated_runtime(|| async {
            let (metrics, events) = mem_backends();

            let spectra = Spectra::builder()
                .metrics_backend(Arc::clone(&metrics))
                .events_backend(Arc::clone(&events))
                .persist(PersistConfig {
                    batch_max: 16,
                    batch_enabled: true,
                    ..PersistConfig::default()
                })
                .embedded()
                .build()
                .expect("build");

            for i in 0..8 {
                try_record_counter_now(&format!("cfg_batch_{i}"), &[], 1);
            }
            spectra.flush_persist().await.expect("flush");

            for i in 0..8 {
                let points = spectra
                    .router()
                    .query_metrics(spectra_core::MetricsQueryRange {
                        metric_name: format!("cfg_batch_{i}"),
                        start: chrono::Utc::now() - chrono::Duration::seconds(5),
                        end: chrono::Utc::now() + chrono::Duration::seconds(1),
                        label_matchers: vec![],
                    })
                    .await
                    .expect("query");
                assert_eq!(points.len(), 1, "counter {i}");
            }
        })
        .await;
    }
}
