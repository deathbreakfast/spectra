//! Install Spectra for one matrix row.

mod backends;
mod telemetry;

use std::sync::Arc;

use anyhow::Result;
use spectra::{PersistConfig, RecordingSink, Spectra, SpectraSink};

use crate::fixtures::{assert_embedded_topology, validate_matrix_env};
use crate::matrix::{MatrixSpec, Topology, TransportAdapter};

pub use backends::BackendPair;

pub(crate) static MATRIX_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Holds an installed Spectra runtime for scenario execution.
pub struct InstalledSpectra {
    /// Installed facade runtime (router + global sink).
    pub spectra: Spectra,
    /// Matrix row this installation corresponds to.
    pub matrix: MatrixSpec,
    /// Present when transport is [`TransportAdapter::Recording`].
    pub transport: Option<Arc<RecordingSink>>,
    metrics: Arc<dyn spectra::spectra_core::MetricsStorageBackend>,
    events: Arc<dyn spectra::spectra_core::EventStorageBackend>,
    _backends: BackendPair,
    _telemetry: telemetry::TelemetryState,
}

/// Prepares one matrix row installation (backends, transport, telemetry).
pub struct BootstrapSession {
    matrix: MatrixSpec,
    slug_suffix: Option<String>,
    persist_config: Option<PersistConfig>,
}

impl BootstrapSession {
    /// Capture the matrix row to install (does not touch global state yet).
    pub const fn new(matrix: MatrixSpec) -> Self {
        Self {
            matrix,
            slug_suffix: None,
            persist_config: None,
        }
    }

    /// Append a suffix to the storage slug (e.g. multibench client index).
    pub fn with_slug_suffix(mut self, suffix: impl Into<String>) -> Self {
        self.slug_suffix = Some(suffix.into());
        self
    }

    /// Override L2 [`PersistConfig`] (batch size, queue depth, etc.).
    pub fn with_persist_config(mut self, config: PersistConfig) -> Self {
        self.persist_config = Some(config);
        self
    }

    /// View the matrix row configured for this session.
    pub const fn matrix(&self) -> &MatrixSpec {
        &self.matrix
    }

    /// Install backends, transport, and global sink under a process-wide test lock.
    pub async fn install_async(self) -> Result<InstalledSpectra> {
        assert_embedded_topology(self.matrix.storage, self.matrix.topology)?;
        validate_matrix_env(self.matrix.storage)?;

        let slug = self.bench_slug();
        let backends = backends::build_backends(self.matrix.storage, &slug).await?;

        let mut builder = Spectra::builder()
            .metrics_backend(Arc::clone(&backends.metrics))
            .events_backend(Arc::clone(&backends.events));

        if let Some(persist_config) = self.persist_config {
            builder = builder.persist(persist_config);
        }

        if self.matrix.topology == Topology::Embedded {
            builder = builder.embedded();
        }

        if !self.matrix.persist_enabled {
            builder = builder.persist_disabled();
        }

        let transport = match self.matrix.transport {
            TransportAdapter::Direct => None,
            TransportAdapter::Recording => {
                let sink = Arc::new(RecordingSink::new());
                builder = builder.sink(Arc::clone(&sink) as Arc<dyn SpectraSink>);
                Some(sink)
            }
        };

        let (builder, telemetry_state) =
            telemetry::apply_telemetry(builder, self.matrix.telemetry, self.matrix.transport, &slug)?;

        let spectra = builder
            .build()
            .map_err(|e| anyhow::anyhow!("SpectraBuilder: {e}"))?;

        Ok(InstalledSpectra {
            spectra,
            matrix: self.matrix,
            transport,
            metrics: Arc::clone(&backends.metrics),
            events: Arc::clone(&backends.events),
            _backends: backends,
            _telemetry: telemetry_state,
        })
    }

    fn bench_slug(&self) -> String {
        match &self.slug_suffix {
            Some(suffix) => format!("{}-{}", self.matrix.slug(), suffix),
            None => self.matrix.slug(),
        }
    }
}

impl InstalledSpectra {
    /// Metrics storage backend for adapter-direct bench workloads.
    pub fn metrics_backend(&self) -> Arc<dyn spectra::spectra_core::MetricsStorageBackend> {
        Arc::clone(&self.metrics)
    }

    /// Events storage backend for adapter-direct bench workloads.
    pub fn events_backend(&self) -> Arc<dyn spectra::spectra_core::EventStorageBackend> {
        Arc::clone(&self.events)
    }
}
