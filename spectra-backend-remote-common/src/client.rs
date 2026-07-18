//! ClickHouse-protocol HTTP and native TCP client wrapper.

use std::path::{Path, PathBuf};

use clickhouse::Client as HttpClient;
use spectra_core::{Error, Result};

/// Shared client for remote ClickHouse-compatible storage engines.
#[derive(Clone)]
pub struct RemoteClient {
    inner: ClientInner,
}

#[derive(Clone)]
enum ClientInner {
    Http(HttpClient),
    Native(NativeEndpoint),
}

#[derive(Clone)]
struct NativeEndpoint {
    host: String,
    port: u16,
    cli: PathBuf,
}

/// Streaming insert handle (HTTP RowBinary or native SQL insert).
pub struct RemoteInsert<T> {
    inner: InsertInner<T>,
}

enum InsertInner<T> {
    Http(clickhouse::insert::Insert<T>),
    Native {
        endpoint: NativeEndpoint,
        table: &'static str,
        rows: Vec<T>,
    },
}

impl RemoteClient {
    /// Connect to a remote engine (`http://` / `https://` or `tcp://host:port`).
    pub async fn connect(url: &str) -> Result<Self> {
        if let Some(addr) = url.strip_prefix("tcp://") {
            let (host, port) = parse_host_port(addr)?;
            let cli = resolve_clickhouse_client()?;
            Ok(Self {
                inner: ClientInner::Native(NativeEndpoint { host, port, cli }),
            })
        } else {
            let client = HttpClient::default().with_url(url);
            Ok(Self {
                inner: ClientInner::Http(client),
            })
        }
    }

    /// Execute DDL or administrative SQL.
    pub async fn execute(&self, sql: &str) -> Result<()> {
        match &self.inner {
            ClientInner::Http(client) => client
                .query(sql)
                .execute()
                .await
                .map_err(|e| Error::Internal(e.to_string())),
            ClientInner::Native(endpoint) => run_native_execute(endpoint, sql).await,
        }
    }

    /// Query three string columns (legacy helper for tests).
    pub async fn query_strings(&self, sql: &str) -> Result<Vec<(String, String, String)>> {
        match &self.inner {
            ClientInner::Http(client) => {
                #[derive(clickhouse::Row, serde::Deserialize)]
                struct Row3 {
                    c0: String,
                    c1: String,
                    c2: String,
                }
                let rows = client
                    .query(sql)
                    .fetch_all::<Row3>()
                    .await
                    .map_err(|e| Error::Internal(e.to_string()))?;
                Ok(rows.into_iter().map(|r| (r.c0, r.c1, r.c2)).collect())
            }
            ClientInner::Native(endpoint) => {
                let lines = run_native_select(endpoint, sql).await?;
                Ok(lines
                    .into_iter()
                    .map(|cols| {
                        (
                            cols.first().cloned().unwrap_or_default(),
                            cols.get(1).cloned().unwrap_or_default(),
                            cols.get(2).cloned().unwrap_or_default(),
                        )
                    })
                    .collect())
            }
        }
    }

    /// Fetch metric rows `(value, labels_json, ts)`.
    pub async fn query_metric_rows(&self, sql: &str) -> Result<Vec<(f64, String, String)>> {
        match &self.inner {
            ClientInner::Http(client) => {
                #[derive(clickhouse::Row, serde::Deserialize)]
                struct MetricRow {
                    value: f64,
                    labels: String,
                    ts: String,
                }
                let rows = client
                    .query(sql)
                    .fetch_all::<MetricRow>()
                    .await
                    .map_err(|e| Error::Internal(e.to_string()))?;
                Ok(rows
                    .into_iter()
                    .map(|r| (r.value, r.labels, r.ts))
                    .collect())
            }
            ClientInner::Native(endpoint) => {
                let lines = run_native_select(endpoint, sql).await?;
                let mut out = Vec::new();
                for cols in lines {
                    if cols.len() < 3 {
                        continue;
                    }
                    let value = cols[0]
                        .parse::<f64>()
                        .map_err(|e| Error::Internal(e.to_string()))?;
                    out.push((value, cols[1].clone(), cols[2].clone()));
                }
                Ok(out)
            }
        }
    }

    /// Fetch event rows `(fields_json, ts)`.
    pub async fn query_event_rows(&self, sql: &str) -> Result<Vec<(String, String)>> {
        match &self.inner {
            ClientInner::Http(client) => {
                #[derive(clickhouse::Row, serde::Deserialize)]
                struct EventRow {
                    fields: String,
                    ts: String,
                }
                let rows = client
                    .query(sql)
                    .fetch_all::<EventRow>()
                    .await
                    .map_err(|e| Error::Internal(e.to_string()))?;
                Ok(rows.into_iter().map(|r| (r.fields, r.ts)).collect())
            }
            ClientInner::Native(endpoint) => {
                let lines = run_native_select(endpoint, sql).await?;
                Ok(lines
                    .into_iter()
                    .filter_map(|cols| {
                        if cols.len() < 2 {
                            return None;
                        }
                        Some((cols[0].clone(), cols[1].clone()))
                    })
                    .collect())
            }
        }
    }

    /// Begin a streaming insert into `spectra_metrics`.
    pub async fn insert_metrics(&self) -> Result<RemoteInsert<MetricInsertRow>> {
        match &self.inner {
            ClientInner::Http(client) => Ok(RemoteInsert {
                inner: InsertInner::Http(
                    client
                        .insert("spectra_metrics")
                        .await
                        .map_err(|e| Error::Internal(e.to_string()))?,
                ),
            }),
            ClientInner::Native(endpoint) => Ok(RemoteInsert {
                inner: InsertInner::Native {
                    endpoint: endpoint.clone(),
                    table: "spectra_metrics",
                    rows: Vec::new(),
                },
            }),
        }
    }

    /// Begin a streaming insert into `spectra_events`.
    pub async fn insert_events(&self) -> Result<RemoteInsert<EventInsertRow>> {
        match &self.inner {
            ClientInner::Http(client) => Ok(RemoteInsert {
                inner: InsertInner::Http(
                    client
                        .insert("spectra_events")
                        .await
                        .map_err(|e| Error::Internal(e.to_string()))?,
                ),
            }),
            ClientInner::Native(endpoint) => Ok(RemoteInsert {
                inner: InsertInner::Native {
                    endpoint: endpoint.clone(),
                    table: "spectra_events",
                    rows: Vec::new(),
                },
            }),
        }
    }
}

impl<T> RemoteInsert<T>
where
    T: clickhouse::RowOwned
        + clickhouse::RowWrite
        + Clone
        + Send
        + Sync
        + 'static
        + InsertSqlRow,
{
    /// Append one row to the insert stream.
    pub async fn write(&mut self, row: &T) -> Result<()> {
        match &mut self.inner {
            InsertInner::Http(insert) => insert
                .write(row)
                .await
                .map_err(|e| Error::Internal(e.to_string())),
            InsertInner::Native { rows, .. } => {
                rows.push(row.clone());
                Ok(())
            }
        }
    }

    /// Finish the insert stream.
    pub async fn end(self) -> Result<()> {
        match self.inner {
            InsertInner::Http(insert) => insert
                .end()
                .await
                .map_err(|e| Error::Internal(e.to_string())),
            InsertInner::Native {
                endpoint,
                table,
                rows,
            } => {
                if rows.is_empty() {
                    return Ok(());
                }
                let values = rows
                    .iter()
                    .map(|row| row.insert_values_sql())
                    .collect::<Vec<_>>()
                    .join(", ");
                let sql = format!("INSERT INTO {table} VALUES {values}");
                run_native_execute(&endpoint, &sql).await
            }
        }
    }
}

/// Row shape for metric inserts.
#[derive(clickhouse::Row, serde::Serialize, Clone)]
pub struct MetricInsertRow {
    /// Metric name.
    pub name: String,
    /// `counter` or `gauge`.
    pub kind: String,
    /// Numeric value.
    pub value: f64,
    /// JSON-encoded labels.
    pub labels: String,
    /// RFC3339 timestamp string.
    pub ts: String,
    /// Optional correlation identifier.
    pub correlation_id: Option<String>,
}

/// Row shape for event inserts.
#[derive(clickhouse::Row, serde::Serialize, Clone)]
pub struct EventInsertRow {
    /// Logical event table name.
    pub table_name: String,
    /// JSON-encoded fields.
    pub fields: String,
    /// RFC3339 timestamp string.
    pub ts: String,
    /// Optional correlation identifier.
    pub correlation_id: Option<String>,
}

/// Format a UTC timestamp for remote storage.
pub fn datetime_to_ch_ts(ts: chrono::DateTime<chrono::Utc>) -> String {
    ts.to_rfc3339()
}

/// Parse an RFC3339 timestamp from remote storage.
pub fn parse_rfc3339_ts(s: &str) -> Result<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .map_err(|e| Error::Internal(e.to_string()))
}

fn parse_host_port(addr: &str) -> Result<(String, u16)> {
    let (host, port) = if let Some((host, port)) = addr.rsplit_once(':') {
        (
            host.to_string(),
            port.parse::<u16>()
                .map_err(|e| Error::Internal(e.to_string()))?,
        )
    } else {
        (addr.to_string(), 9528)
    };
    Ok((host, port))
}

fn sql_quote(s: &str) -> String {
    format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
}

trait InsertSqlRow {
    fn insert_values_sql(&self) -> String;
}

impl InsertSqlRow for MetricInsertRow {
    fn insert_values_sql(&self) -> String {
        let correlation = match &self.correlation_id {
            Some(id) => sql_quote(id),
            None => "NULL".to_string(),
        };
        format!(
            "({name}, {kind}, {value}, {labels}, {ts}, {correlation})",
            name = sql_quote(&self.name),
            kind = sql_quote(&self.kind),
            value = self.value,
            labels = sql_quote(&self.labels),
            ts = sql_quote(&self.ts),
            correlation = correlation,
        )
    }
}

impl InsertSqlRow for EventInsertRow {
    fn insert_values_sql(&self) -> String {
        let correlation = match &self.correlation_id {
            Some(id) => sql_quote(id),
            None => "NULL".to_string(),
        };
        format!(
            "({table_name}, {fields}, {ts}, {correlation})",
            table_name = sql_quote(&self.table_name),
            fields = sql_quote(&self.fields),
            ts = sql_quote(&self.ts),
            correlation = correlation,
        )
    }
}

fn resolve_clickhouse_client() -> Result<PathBuf> {
    if let Ok(path) = std::env::var("SPECTRA_CLICKHOUSE_CLIENT_PATH") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        let bundled = Path::new(&home).join("tensorbase-smoke/clickhouse-client");
        if bundled.is_file() {
            return Ok(bundled);
        }
    }
    if let Ok(path) = which_client("clickhouse-client") {
        return Ok(path);
    }
    Err(Error::Internal(
        "tcp:// URLs require clickhouse-client (set SPECTRA_CLICKHOUSE_CLIENT_PATH or install in PATH)".into(),
    ))
}

fn which_client(name: &str) -> Result<PathBuf> {
    let output = std::process::Command::new("which")
        .arg(name)
        .output()
        .map_err(|e| Error::Internal(e.to_string()))?;
    if !output.status.success() {
        return Err(Error::Internal(format!("{name} not found")));
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(PathBuf::from(path))
}

async fn run_native_execute(endpoint: &NativeEndpoint, sql: &str) -> Result<()> {
    let output = tokio::process::Command::new(&endpoint.cli)
        .arg("--host")
        .arg(&endpoint.host)
        .arg("--port")
        .arg(endpoint.port.to_string())
        .arg("--query")
        .arg(sql)
        .output()
        .await
        .map_err(|e| Error::Internal(e.to_string()))?;
    if !output.status.success() {
        return Err(Error::Internal(format!(
            "clickhouse-client execute failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(())
}

async fn run_native_select(endpoint: &NativeEndpoint, sql: &str) -> Result<Vec<Vec<String>>> {
    let output = tokio::process::Command::new(&endpoint.cli)
        .arg("--host")
        .arg(&endpoint.host)
        .arg("--port")
        .arg(endpoint.port.to_string())
        .arg("--query")
        .arg(sql)
        .output()
        .await
        .map_err(|e| Error::Internal(e.to_string()))?;
    if !output.status.success() {
        return Err(Error::Internal(format!(
            "clickhouse-client query failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| line.split('\t').map(str::to_string).collect())
        .collect())
}
