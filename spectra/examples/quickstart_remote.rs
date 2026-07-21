//! Remote storage backends example (ClickHouse or TensorBase URL).

use std::sync::Arc;

use spectra_core::SharedEventBackend;
use spectra_core::SharedMetricsBackend;

#[cfg(feature = "clickhouse")]
async fn run_clickhouse(url: &str) -> spectra_core::Result<()> {
    use spectra::{ClickHouseEventsBackend, ClickHouseMetricsBackend, Spectra};

    let metrics: SharedMetricsBackend = Arc::new(ClickHouseMetricsBackend::connect(url).await?);
    let events: SharedEventBackend = Arc::new(ClickHouseEventsBackend::connect(url).await?);

    let _spectra = Spectra::builder()
        .metrics_backend(metrics)
        .events_backend(events)
        .build()?;
    Ok(())
}

#[cfg(feature = "tensorbase")]
async fn run_tensorbase(url: &str) -> spectra_core::Result<()> {
    use spectra::{Spectra, TensorBaseEventsBackend, TensorBaseMetricsBackend};

    let metrics: SharedMetricsBackend = Arc::new(TensorBaseMetricsBackend::connect(url).await?);
    let events: SharedEventBackend = Arc::new(TensorBaseEventsBackend::connect(url).await?);

    let _spectra = Spectra::builder()
        .metrics_backend(metrics)
        .events_backend(events)
        .build()?;
    Ok(())
}

#[tokio::main]
async fn main() -> spectra_core::Result<()> {
    let url = std::env::var("SPECTRA_REMOTE_URL").unwrap_or_else(|_| {
        eprintln!("Set SPECTRA_REMOTE_URL (http://host:8123 or tcp://host:9528)");
        std::process::exit(1);
    });

    #[cfg(all(feature = "clickhouse", not(feature = "tensorbase")))]
    run_clickhouse(&url).await?;

    #[cfg(all(feature = "tensorbase", not(feature = "clickhouse")))]
    run_tensorbase(&url).await?;

    #[cfg(all(feature = "clickhouse", feature = "tensorbase"))]
    {
        if url.contains(":9528") {
            run_tensorbase(&url).await?;
        } else {
            run_clickhouse(&url).await?;
        }
    }

    Ok(())
}
