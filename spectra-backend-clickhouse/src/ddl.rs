//! ClickHouse DDL for canonical Spectra tables.

use spectra_core::{EVENTS_TABLE, METRICS_TABLE};

/// ClickHouse metrics table DDL (MergeTree, JSON labels as String).
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
) ENGINE = MergeTree()
ORDER BY (name, ts)
"
    )
}

/// ClickHouse events table DDL.
pub fn events_ddl() -> String {
    format!(
        r"
CREATE TABLE IF NOT EXISTS {EVENTS_TABLE} (
    table_name String,
    fields String,
    ts String,
    correlation_id Nullable(String)
) ENGINE = MergeTree()
ORDER BY (table_name, ts)
"
    )
}
