//! TensorBase DDL (ClickHouse-compatible dialect, BaseStorage engine).

use spectra_core::{EVENTS_TABLE, METRICS_TABLE};

/// TensorBase metrics table DDL.
pub fn metrics_ddl() -> String {
    format!(
        r"
CREATE TABLE IF NOT EXISTS {METRICS_TABLE} (
    name String,
    kind String,
    value Float64,
    labels String,
    ts String,
    correlation_id Nullable(String)
) ENGINE = BaseStorage
"
    )
}

/// TensorBase events table DDL.
pub fn events_ddl() -> String {
    format!(
        r"
CREATE TABLE IF NOT EXISTS {EVENTS_TABLE} (
    table_name String,
    fields String,
    ts String,
    correlation_id Nullable(String)
) ENGINE = BaseStorage
"
    )
}

/// Default TensorBase native protocol URL (port 9528).
pub fn default_url(host: &str) -> String {
    format!("tcp://{host}:9528")
}
