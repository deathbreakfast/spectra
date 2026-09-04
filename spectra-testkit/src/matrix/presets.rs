use super::{MatrixSpec, StorageAdapter, TelemetryAdapter, Topology, TransportAdapter};

/// Default CI slice: mem/sqlite × direct/recording × telemetry off × embedded.
pub fn ci_embedded_rows() -> Vec<MatrixSpec> {
    let mut rows = Vec::new();
    for storage in [StorageAdapter::Mem, StorageAdapter::Sqlite] {
        for transport in [TransportAdapter::Direct, TransportAdapter::Recording] {
            rows.push(MatrixSpec {
                storage,
                transport,
                telemetry: TelemetryAdapter::Off,
                topology: Topology::Embedded,
                persist_enabled: true,
            });
        }
    }
    rows
}

/// CI rows with recording transport (dual-path and transport-only scenarios).
pub fn ci_recording_rows() -> Vec<MatrixSpec> {
    ci_embedded_rows()
        .into_iter()
        .filter(|row| row.transport == TransportAdapter::Recording)
        .collect()
}

/// CI telemetry slice: mem + direct + console NDJSON (`telemetry-console` feature).
pub fn ci_telemetry_rows() -> Vec<MatrixSpec> {
    vec![MatrixSpec {
        storage: StorageAdapter::Mem,
        transport: TransportAdapter::Direct,
        telemetry: TelemetryAdapter::ConsoleNdjson,
        topology: Topology::Embedded,
        persist_enabled: true,
    }]
}

/// Remote ingest rows (require env URLs; use with `#[ignore]` when unset).
pub fn remote_ingest_rows() -> Vec<MatrixSpec> {
    vec![
        MatrixSpec {
            storage: StorageAdapter::TensorBase,
            transport: TransportAdapter::Direct,
            telemetry: TelemetryAdapter::Off,
            topology: Topology::RemoteIngest,
            persist_enabled: true,
        },
        MatrixSpec {
            storage: StorageAdapter::ClickHouse,
            transport: TransportAdapter::Direct,
            telemetry: TelemetryAdapter::Off,
            topology: Topology::RemoteIngest,
            persist_enabled: true,
        },
    ]
}
