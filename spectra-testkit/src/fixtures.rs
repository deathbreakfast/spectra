use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use tempfile::TempDir;

use crate::matrix::StorageAdapter;

/// Holds temp directories alive for sqlite matrix rows.
pub struct TempStore {
    _dir: TempDir,
    /// Path to the sqlite metrics database file.
    pub metrics_path: PathBuf,
    /// Path to the sqlite events database file.
    pub events_path: PathBuf,
}

impl TempStore {
    /// Create a temp directory with `metrics.db` and `events.db` paths under `prefix`.
    pub fn new(prefix: &str) -> Result<Self> {
        let dir = tempfile::tempdir().context("tempdir for sqlite backends")?;
        let base = dir.path().join(prefix);
        std::fs::create_dir_all(&base)?;
        Ok(Self {
            metrics_path: base.join("metrics.db"),
            events_path: base.join("events.db"),
            _dir: dir,
        })
    }
}

/// Read `SPECTRA_TENSORBASE_URL` (required for tensorbase matrix rows).
///
/// Multi-DW: uses `SPECTRA_TENSORBASE_URL_{shard}` when `SPECTRA_BENCH_DW_N>1`.
pub fn tensorbase_url() -> Result<String> {
    crate::shard::tensorbase_url_sharded()
}

/// Read `SPECTRA_CLICKHOUSE_URL` (required for clickhouse matrix rows).
///
/// Multi-DW: uses `SPECTRA_CLICKHOUSE_URL_{shard}` when `SPECTRA_BENCH_DW_N>1`.
pub fn clickhouse_url() -> Result<String> {
    crate::shard::clickhouse_url_sharded()
}

/// Return true when the env vars for a remote storage adapter are present.
pub fn remote_env_ready(storage: StorageAdapter) -> bool {
    match storage {
        StorageAdapter::TensorBase => crate::shard::tensorbase_url_sharded().is_ok(),
        StorageAdapter::ClickHouse => crate::shard::clickhouse_url_sharded().is_ok(),
        StorageAdapter::Mem | StorageAdapter::Sqlite => true,
    }
}

/// Fail fast when a matrix row requires remote URLs that are not set.
pub fn validate_matrix_env(storage: StorageAdapter) -> Result<()> {
    match storage {
        StorageAdapter::Mem | StorageAdapter::Sqlite => Ok(()),
        StorageAdapter::TensorBase => {
            crate::shard::tensorbase_url_sharded()?;
            Ok(())
        }
        StorageAdapter::ClickHouse => {
            crate::shard::clickhouse_url_sharded()?;
            Ok(())
        }
    }
}

/// Create a temp directory for NDJSON telemetry output during matrix runs.
#[cfg(feature = "telemetry-console")]
pub fn telemetry_dir(prefix: &str) -> Result<(TempDir, PathBuf)> {
    let dir = tempfile::tempdir().context("tempdir for telemetry ndjson")?;
    let path = dir.path().join(prefix);
    std::fs::create_dir_all(&path)?;
    Ok((dir, path))
}

/// Ensure storage adapter choice matches the declared host topology.
pub fn assert_embedded_topology(
    storage: StorageAdapter,
    topology: crate::matrix::Topology,
) -> Result<()> {
    use crate::matrix::Topology;
    match (storage, topology) {
        (StorageAdapter::Mem | StorageAdapter::Sqlite, Topology::Embedded) => Ok(()),
        (StorageAdapter::TensorBase | StorageAdapter::ClickHouse, Topology::RemoteIngest) => Ok(()),
        (StorageAdapter::Mem | StorageAdapter::Sqlite, Topology::RemoteIngest) => {
            bail!("mem/sqlite matrix rows use embedded topology only")
        }
        (StorageAdapter::TensorBase | StorageAdapter::ClickHouse, Topology::Embedded) => {
            bail!("remote storage adapters require remote-ingest topology")
        }
    }
}
