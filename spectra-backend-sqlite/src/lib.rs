//! Durable embedded SQLite metrics and events storage.
//!
//! Enable with the `spectra` feature `sqlite` and wire through `Spectra::builder()`.
//!
//! - [`SqliteMetricsBackend::new`] / [`SqliteEventsBackend::new`] — open or create database files
//! - Parent directories are created automatically; uses `spawn_blocking` for rusqlite I/O.
//! - `query_aggregate` is not yet implemented (returns empty series).
//! - Default event query limit is 1000 rows when `limit` is unset.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde_json::Value;
use spectra_core::{
    Error, EventAggregateResult, EventRow, EventStorageBackend, EventsAggregateFilter,
    EventsQueryFilter, LabelMatcher, MetricPoint, MetricsQueryRange, MetricsStorageBackend, Result,
    StorageEngineType,
};

const METRICS_DDL: &str = r"
CREATE TABLE IF NOT EXISTS spectra_metrics (
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    value REAL NOT NULL,
    labels TEXT NOT NULL,
    ts TEXT NOT NULL,
    correlation_id TEXT
);
CREATE INDEX IF NOT EXISTS idx_spectra_metrics_name_ts ON spectra_metrics(name, ts);
";

const EVENTS_DDL: &str = r"
CREATE TABLE IF NOT EXISTS spectra_events (
    table_name TEXT NOT NULL,
    fields TEXT NOT NULL,
    ts TEXT NOT NULL,
    correlation_id TEXT
);
CREATE INDEX IF NOT EXISTS idx_spectra_events_table_ts ON spectra_events(table_name, ts);
";

fn map_sqlite(e: rusqlite::Error) -> Error {
    let message = e.to_string();
    Error::storage_source(message, e)
}

fn map_join(e: tokio::task::JoinError) -> Error {
    Error::storage_source("sqlite worker join failed", e)
}

fn open_and_migrate(path: &Path, ddl: &str) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(Error::Io)?;
    }
    let conn = Connection::open(path).map_err(map_sqlite)?;
    conn.execute_batch(ddl).map_err(map_sqlite)?;
    Ok(conn)
}

fn ts_to_rfc3339(ts: DateTime<Utc>) -> String {
    ts.to_rfc3339()
}

fn parse_ts(s: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| Error::internal(format!("invalid metric/event timestamp: {e}")))
}

fn labels_match(labels: &Value, matchers: &[LabelMatcher]) -> bool {
    matchers.iter().all(|m| {
        labels
            .get(&m.key)
            .and_then(|v| v.as_str())
            .map(|v| v == m.value)
            .unwrap_or(false)
    })
}

/// Durable SQLite metrics storage in the `spectra_metrics` table.
///
/// [`new`](Self::new) opens the database file and applies the required DDL. Use a separate
/// file from the events backend when following the standard Spectra wiring.
///
/// # Examples
///
/// Facade wiring through `Spectra::builder()` (requires the `spectra` crate with the
/// `sqlite` feature):
///
/// ```ignore
/// use std::sync::Arc;
/// use spectra::{Spectra, SqliteEventsBackend, SqliteMetricsBackend};
///
/// let dir = std::env::temp_dir().join("spectra-example");
/// let spectra = Spectra::builder()
///     .metrics_backend(Arc::new(SqliteMetricsBackend::new(dir.join("metrics.db"))?))
///     .events_backend(Arc::new(SqliteEventsBackend::new(dir.join("events.db"))?))
///     .embedded()
///     .build()?;
/// ```
///
/// Direct backend usage:
///
/// ```no_run
/// use chrono::{Duration, Utc};
/// use serde_json::json;
/// use spectra_backend_sqlite::SqliteMetricsBackend;
/// use spectra_core::{MetricsQueryRange, MetricsStorageBackend};
///
/// # async fn example() -> spectra_core::Result<()> {
/// let dir = tempfile::tempdir()?;
/// let backend = SqliteMetricsBackend::new(dir.path().join("metrics.db"))?;
/// let now = Utc::now();
/// backend.record_counter("cache_hits", &json!({}), 1, now).await?;
///
/// let points = backend.query_range(MetricsQueryRange {
///     metric_name: "cache_hits".into(),
///     start: now - Duration::seconds(1),
///     end: now + Duration::seconds(1),
///     label_matchers: vec![],
/// }).await?;
/// assert_eq!(points.len(), 1);
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct SqliteMetricsBackend {
    conn: Arc<Mutex<Connection>>,
    path: PathBuf,
}

impl SqliteMetricsBackend {
    /// Open or create a SQLite database at `path` and apply metrics DDL migrations.
    ///
    /// The constructor is synchronous. Parent directories must already exist. Use a separate
    /// database file from the events backend when following the standard Spectra wiring.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use spectra_backend_sqlite::SqliteMetricsBackend;
    ///
    /// # fn example() -> spectra_core::Result<()> {
    /// let backend = SqliteMetricsBackend::new("/tmp/spectra-metrics.db")?;
    /// assert!(backend.path().ends_with("spectra-metrics.db"));
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let conn = open_and_migrate(&path, METRICS_DDL)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            path,
        })
    }

    /// Filesystem path of the backing SQLite database.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[async_trait]
impl MetricsStorageBackend for SqliteMetricsBackend {
    fn engine_type(&self) -> StorageEngineType {
        StorageEngineType::Sqlite
    }

    async fn record_counter(
        &self,
        name: &str,
        labels: &Value,
        delta: i64,
        ts: DateTime<Utc>,
    ) -> Result<()> {
        let conn = Arc::clone(&self.conn);
        let name = name.to_string();
        let labels = labels.to_string();
        let ts = ts_to_rfc3339(ts);
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock();
            conn.execute(
                "INSERT INTO spectra_metrics (name, kind, value, labels, ts, correlation_id) VALUES (?1, 'counter', ?2, ?3, ?4, NULL)",
                params![name, delta as f64, labels, ts],
            )
            .map_err(map_sqlite)?;
            Ok(())
        })
        .await
        .map_err(map_join)?
    }

    async fn record_gauge(
        &self,
        name: &str,
        labels: &Value,
        value: f64,
        ts: DateTime<Utc>,
    ) -> Result<()> {
        let conn = Arc::clone(&self.conn);
        let name = name.to_string();
        let labels = labels.to_string();
        let ts = ts_to_rfc3339(ts);
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock();
            conn.execute(
                "INSERT INTO spectra_metrics (name, kind, value, labels, ts, correlation_id) VALUES (?1, 'gauge', ?2, ?3, ?4, NULL)",
                params![name, value, labels, ts],
            )
            .map_err(map_sqlite)?;
            Ok(())
        })
        .await
        .map_err(map_join)?
    }

    async fn query_range(&self, query: MetricsQueryRange) -> Result<Vec<MetricPoint>> {
        let conn = Arc::clone(&self.conn);
        let name = query.metric_name.clone();
        let start = ts_to_rfc3339(query.start);
        let end = ts_to_rfc3339(query.end);
        let label_matchers = query.label_matchers.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock();
            let mut stmt = conn
                .prepare(
                    "SELECT value, labels, ts FROM spectra_metrics WHERE name = ?1 AND ts >= ?2 AND ts <= ?3 ORDER BY ts ASC",
                )
                .map_err(map_sqlite)?;
            let rows = stmt
                .query_map(params![name, start, end], |row| {
                    let value: f64 = row.get(0)?;
                    let labels: String = row.get(1)?;
                    let ts: String = row.get(2)?;
                    Ok((value, labels, ts))
                })
                .map_err(map_sqlite)?;
            let mut out = Vec::new();
            for row in rows {
                let (value, labels, ts) = row.map_err(map_sqlite)?;
                let labels: Value = serde_json::from_str(&labels)?;
                if labels_match(&labels, &label_matchers) {
                    out.push(MetricPoint {
                        ts: parse_ts(&ts)?,
                        value,
                        labels,
                    });
                }
            }
            Ok(out)
        })
        .await
        .map_err(map_join)?
    }
}

/// Durable SQLite structured-event storage in the `spectra_events` table.
///
/// [`new`](Self::new) opens the database file and applies the required DDL. Event rows retain
/// their logical table name and JSON field payload.
///
/// # Examples
///
/// Facade wiring through `Spectra::builder()` (requires the `spectra` crate with the
/// `sqlite` feature):
///
/// ```ignore
/// use std::sync::Arc;
/// use spectra::{Spectra, SqliteEventsBackend, SqliteMetricsBackend};
///
/// let dir = std::env::temp_dir().join("spectra-example");
/// let spectra = Spectra::builder()
///     .metrics_backend(Arc::new(SqliteMetricsBackend::new(dir.join("metrics.db"))?))
///     .events_backend(Arc::new(SqliteEventsBackend::new(dir.join("events.db"))?))
///     .embedded()
///     .build()?;
/// ```
///
/// Direct backend usage:
///
/// ```no_run
/// use chrono::Utc;
/// use serde_json::json;
/// use spectra_backend_sqlite::SqliteEventsBackend;
/// use spectra_core::{EventStorageBackend, EventsQueryFilter};
///
/// # async fn example() -> spectra_core::Result<()> {
/// let dir = tempfile::tempdir()?;
/// let backend = SqliteEventsBackend::new(dir.path().join("events.db"))?;
/// backend.append_row(
///     "request_log",
///     &json!({"message": "handled"}),
///     Utc::now(),
///     None,
/// ).await?;
///
/// let rows = backend.query_rows(EventsQueryFilter {
///     table: "request_log".into(),
///     ..Default::default()
/// }).await?;
/// assert_eq!(rows.len(), 1);
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct SqliteEventsBackend {
    conn: Arc<Mutex<Connection>>,
    path: PathBuf,
}

impl SqliteEventsBackend {
    /// Open or create a SQLite database at `path` and apply events DDL migrations.
    ///
    /// The constructor is synchronous. Parent directories must already exist. Use a separate
    /// database file from the metrics backend when following the standard Spectra wiring.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use spectra_backend_sqlite::SqliteEventsBackend;
    ///
    /// # fn example() -> spectra_core::Result<()> {
    /// let backend = SqliteEventsBackend::new("/tmp/spectra-events.db")?;
    /// assert!(backend.path().ends_with("spectra-events.db"));
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let conn = open_and_migrate(&path, EVENTS_DDL)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            path,
        })
    }

    /// Filesystem path of the backing SQLite database.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[async_trait]
impl EventStorageBackend for SqliteEventsBackend {
    fn engine_type(&self) -> StorageEngineType {
        StorageEngineType::Sqlite
    }

    async fn append_row(
        &self,
        table: &str,
        fields: &Value,
        ts: DateTime<Utc>,
        correlation_id: Option<&str>,
    ) -> Result<()> {
        let conn = Arc::clone(&self.conn);
        let table = table.to_string();
        let fields = fields.to_string();
        let ts = ts_to_rfc3339(ts);
        let cid = correlation_id.map(str::to_string);
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock();
            conn.execute(
                "INSERT INTO spectra_events (table_name, fields, ts, correlation_id) VALUES (?1, ?2, ?3, ?4)",
                params![table, fields, ts, cid],
            )
            .map_err(map_sqlite)?;
            Ok(())
        })
        .await
        .map_err(map_join)?
    }

    async fn query_rows(&self, filter: EventsQueryFilter) -> Result<Vec<EventRow>> {
        let conn = Arc::clone(&self.conn);
        let table = filter.table.clone();
        let start = filter.start.map(ts_to_rfc3339);
        let end = filter.end.map(ts_to_rfc3339);
        // Fetch table/time-scoped candidates, then apply shared filter/sort/pagination
        // so operator semantics match mem and remote-common mem store.
        let out = tokio::task::spawn_blocking(move || {
            let conn = conn.lock();
            let sql = "SELECT fields, ts FROM spectra_events WHERE table_name = ?1 \
                 AND (?2 IS NULL OR ts >= ?2) AND (?3 IS NULL OR ts <= ?3)";
            let mut stmt = conn.prepare(sql).map_err(map_sqlite)?;
            let rows = stmt
                .query_map(params![table, start, end], |row| {
                    let fields: String = row.get(0)?;
                    let ts: String = row.get(1)?;
                    Ok((fields, ts))
                })
                .map_err(map_sqlite)?;
            let mut out = Vec::new();
            for row in rows {
                let (fields, ts) = row.map_err(map_sqlite)?;
                out.push(EventRow {
                    ts: parse_ts(&ts)?,
                    fields: serde_json::from_str(&fields)?,
                });
            }
            Ok::<_, Error>(out)
        })
        .await
        .map_err(map_join)??;
        let mut filter = filter;
        if filter.limit.is_none() {
            filter.limit = Some(1000);
        }
        Ok(spectra_core::finalize_event_rows(out, &filter))
    }

    async fn query_aggregate(
        &self,
        _filter: EventsAggregateFilter,
    ) -> Result<EventAggregateResult> {
        Ok(EventAggregateResult::TimeSeries {
            series: vec![],
            headline: vec![],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use serde_json::json;
    use tempfile::tempdir;

    #[tokio::test]
    async fn sqlite_metrics_roundtrip() {
        let dir = tempdir().expect("tempdir");
        let backend = SqliteMetricsBackend::new(dir.path().join("metrics.db")).expect("open");
        let ts = Utc::now();
        backend
            .record_counter("hits", &json!({}), 2, ts)
            .await
            .expect("write");
        let points = backend
            .query_range(MetricsQueryRange {
                metric_name: "hits".into(),
                start: ts - Duration::seconds(1),
                end: ts + Duration::seconds(1),
                label_matchers: vec![],
            })
            .await
            .expect("query");
        assert_eq!(points.len(), 1);
    }

    #[tokio::test]
    async fn sqlite_events_roundtrip() {
        let dir = tempdir().expect("tempdir");
        let backend = SqliteEventsBackend::new(dir.path().join("events.db")).expect("open");
        let ts = Utc::now();
        backend
            .append_row("req", &json!({"x": 1}), ts, None)
            .await
            .expect("write");
        let rows = backend
            .query_rows(EventsQueryFilter {
                table: "req".into(),
                ..Default::default()
            })
            .await
            .expect("query");
        assert_eq!(rows.len(), 1);
    }
}
