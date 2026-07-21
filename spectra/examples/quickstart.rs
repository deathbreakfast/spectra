//! Minimal embedded Spectra setup with in-memory metrics and events backends.
//!
//! ```bash
//! cargo run -p uf-spectra --example quickstart --features mem
//! ```

use spectra::{MemEventsBackend, MemMetricsBackend, Spectra};

#[tokio::main]
async fn main() -> spectra::Result<()> {
    // Hosts own the subscriber; libraries only emit `tracing` events.
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let _spectra = Spectra::builder()
        .metrics_backend(std::sync::Arc::new(MemMetricsBackend::new()))
        .events_backend(std::sync::Arc::new(MemEventsBackend::new()))
        .embedded()
        .build()?;
    Ok(())
}
