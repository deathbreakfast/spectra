//! Minimal embedded Spectra setup with in-memory metrics and events backends.
//!
//! ```bash
//! cargo run -p uf-spectra --example quickstart --features mem
//! ```

use spectra::{MemEventsBackend, MemMetricsBackend, Spectra};

#[tokio::main]
async fn main() -> spectra::Result<()> {
    let _spectra = Spectra::builder()
        .metrics_backend(std::sync::Arc::new(MemMetricsBackend::new()))
        .events_backend(std::sync::Arc::new(MemEventsBackend::new()))
        .embedded()
        .build()?;
    Ok(())
}
