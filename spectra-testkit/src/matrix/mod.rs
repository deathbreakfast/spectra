//! Verification matrix dimensions for Spectra e2e and bench drivers.

mod presets;

use serde::{Deserialize, Serialize};

pub use presets::{ci_embedded_rows, ci_recording_rows, ci_telemetry_rows, remote_ingest_rows};

/// Storage backend selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum StorageAdapter {
    /// In-memory backend (default).
    #[default]
    Mem,
    /// File-backed SQLite embedded backend.
    Sqlite,
    /// Remote TensorBase native/HTTP protocol.
    TensorBase,
    /// Remote ClickHouse HTTP/native protocol.
    ClickHouse,
}

impl StorageAdapter {
    /// Stable kebab-case slug fragment for matrix rows.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Mem => "mem",
            Self::Sqlite => "sqlite",
            Self::TensorBase => "tensorbase",
            Self::ClickHouse => "clickhouse",
        }
    }

    /// Parse a storage adapter from a matrix CLI/env token.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "mem" => Some(Self::Mem),
            "sqlite" => Some(Self::Sqlite),
            "tensorbase" => Some(Self::TensorBase),
            "clickhouse" => Some(Self::ClickHouse),
            _ => None,
        }
    }
}

/// Emit transport path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum TransportAdapter {
    /// Emit goes directly to the installed global sink (no recording wrapper).
    #[default]
    Direct,
    /// Wrap transport in [`RecordingSink`](spectra::RecordingSink) for dual-path tests.
    Recording,
}

impl TransportAdapter {
    /// Stable kebab-case slug fragment for matrix rows.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Recording => "recording",
        }
    }

    /// Parse a transport adapter from a matrix CLI/env token.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "direct" => Some(Self::Direct),
            "recording" => Some(Self::Recording),
            _ => None,
        }
    }
}

/// Telemetry sink selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum TelemetryAdapter {
    /// No NDJSON/console telemetry sink.
    #[default]
    Off,
    /// Off-thread NDJSON + optional stderr mirror (`telemetry-console` feature).
    ConsoleNdjson,
}

impl TelemetryAdapter {
    /// Stable kebab-case slug fragment for matrix rows.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::ConsoleNdjson => "console-ndjson",
        }
    }

    /// Parse a telemetry adapter from a matrix CLI/env token.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "off" => Some(Self::Off),
            "console-ndjson" | "console_ndjson" | "ndjson" => Some(Self::ConsoleNdjson),
            _ => None,
        }
    }
}

/// Host topology label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Topology {
    /// In-process storage backends on the same host as the emitter.
    #[default]
    Embedded,
    /// Remote storage ingest (tensorbase/clickhouse matrix rows).
    RemoteIngest,
}

impl Topology {
    /// Stable kebab-case slug fragment for matrix rows.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Embedded => "embedded",
            Self::RemoteIngest => "remote-ingest",
        }
    }

    /// Parse a topology label from a matrix CLI/env token.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "embedded" => Some(Self::Embedded),
            "remote-ingest" | "remote_ingest" | "remote" => Some(Self::RemoteIngest),
            _ => None,
        }
    }
}

/// Full cross-product selector for e2e and bench drivers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatrixSpec {
    /// Metrics/events storage engine for this row.
    pub storage: StorageAdapter,
    /// Whether emits pass through a recording transport sink.
    pub transport: TransportAdapter,
    /// Optional NDJSON/console telemetry adapter.
    pub telemetry: TelemetryAdapter,
    /// Embedded vs remote-ingest host topology.
    pub topology: Topology,
    /// When false, only the transport sink receives emits (requires recording transport).
    #[serde(default = "default_persist_enabled")]
    pub persist_enabled: bool,
}

fn default_persist_enabled() -> bool {
    true
}

impl Default for MatrixSpec {
    fn default() -> Self {
        Self {
            storage: StorageAdapter::Mem,
            transport: TransportAdapter::Direct,
            telemetry: TelemetryAdapter::Off,
            topology: Topology::Embedded,
            persist_enabled: true,
        }
    }
}

impl MatrixSpec {
    /// Deterministic slug: `{storage}-{transport}-{telemetry}-{topology}`.
    pub fn slug(&self) -> String {
        format!(
            "{}-{}-{}-{}",
            self.storage.as_str(),
            self.transport.as_str(),
            self.telemetry.as_str(),
            self.topology.as_str()
        )
    }
}
