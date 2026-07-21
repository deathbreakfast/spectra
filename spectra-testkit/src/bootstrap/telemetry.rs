use anyhow::{bail, Result};
use spectra::SpectraBuilder;

#[cfg(feature = "telemetry-console")]
use crate::fixtures::telemetry_dir;
use crate::matrix::{TelemetryAdapter, TransportAdapter};

pub struct TelemetryState {
    pub _telemetry_dir: Option<tempfile::TempDir>,
}

pub fn apply_telemetry(
    builder: SpectraBuilder,
    telemetry: TelemetryAdapter,
    transport: TransportAdapter,
    slug: &str,
) -> Result<(SpectraBuilder, TelemetryState)> {
    match telemetry {
        TelemetryAdapter::Off => Ok((
            builder,
            TelemetryState {
                _telemetry_dir: None,
            },
        )),
        TelemetryAdapter::ConsoleNdjson => {
            if transport != TransportAdapter::Direct {
                bail!("console-ndjson telemetry requires direct transport (no RecordingSink)");
            }
            #[cfg(feature = "telemetry-console")]
            {
                let (dir, path) = telemetry_dir(slug)?;
                let builder = builder
                    .telemetry_ndjson(path)
                    .map_err(|e| anyhow::anyhow!("telemetry_ndjson: {e}"))?;
                Ok((
                    builder,
                    TelemetryState {
                        _telemetry_dir: Some(dir),
                    },
                ))
            }
            #[cfg(not(feature = "telemetry-console"))]
            {
                let _ = slug;
                bail!("telemetry-console feature not enabled on spectra-testkit")
            }
        }
    }
}
