//! Durable embedded Spectra setup with SQLite metrics and events backends.
//!
//! ```bash
//! cargo run -p uf-spectra --example quickstart_sqlite --features sqlite
//! ```

use std::sync::Arc;

use spectra::{Spectra, SqliteEventsBackend, SqliteMetricsBackend};

#[tokio::main]
async fn main() -> spectra::Result<()> {
    let dir = std::env::temp_dir().join("spectra-quickstart-sqlite");
    std::fs::create_dir_all(&dir)?;

    let metrics = SqliteMetricsBackend::new(dir.join("metrics.db"))?;
    let events = SqliteEventsBackend::new(dir.join("events.db"))?;

    let _spectra = Spectra::builder()
        .metrics_backend(Arc::new(metrics))
        .events_backend(Arc::new(events))
        .embedded()
        .build()?;

    eprintln!("SQLite backends ready under {}", dir.display());
    Ok(())
}
