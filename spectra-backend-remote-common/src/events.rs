//! Remote events storage backend (ClickHouse-compatible protocol).

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use spectra_core::{
    EventAggregateResult, EventRow, EventStorageBackend, EventWriteRow, EventsAggregateFilter,
    EventsQueryFilter, Result, StorageEngineType,
};

use crate::client::{
    datetime_to_ch_ts, map_remote, parse_rfc3339_ts, EventInsertRow, RemoteClient,
};
use crate::mem_store::MemStore;
use crate::query_sql;

#[derive(Clone)]
enum EventsInner {
    Remote(Arc<RemoteClient>),
    Mem(Arc<MemStore>),
}

/// Parameterized remote events backend shared by ClickHouse and TensorBase adapters.
pub struct RemoteEventsBackend {
    inner: EventsInner,
    engine: StorageEngineType,
}

impl RemoteEventsBackend {
    /// Connect to a remote engine and ensure the events table exists.
    pub async fn connect(url: &str, engine: StorageEngineType, events_ddl: &str) -> Result<Self> {
        let client = Arc::new(RemoteClient::connect(url).await?);
        client.execute(events_ddl).await?;
        Ok(Self {
            inner: EventsInner::Remote(client),
            engine,
        })
    }

    /// In-memory backend for unit tests (no remote server required).
    pub fn in_memory_for_test(engine: StorageEngineType) -> Self {
        Self {
            inner: EventsInner::Mem(Arc::new(MemStore::new())),
            engine,
        }
    }
}

#[async_trait]
impl EventStorageBackend for RemoteEventsBackend {
    fn engine_type(&self) -> StorageEngineType {
        self.engine
    }

    async fn append_row(
        &self,
        table: &str,
        fields: &Value,
        ts: DateTime<Utc>,
        correlation_id: Option<&str>,
    ) -> Result<()> {
        match &self.inner {
            EventsInner::Mem(store) => store.append_row(table, fields, ts),
            EventsInner::Remote(client) => {
                let mut insert = client.insert_events().await?;
                insert
                    .write(&EventInsertRow {
                        table_name: table.to_string(),
                        fields: fields.to_string(),
                        ts: datetime_to_ch_ts(ts),
                        correlation_id: correlation_id.map(str::to_string),
                    })
                    .await
                    .map_err(map_remote)?;
                insert.end().await.map_err(map_remote)?;
                Ok(())
            }
        }
    }

    async fn append_rows_batch(&self, rows: &[EventWriteRow]) -> Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        match &self.inner {
            EventsInner::Mem(store) => {
                for row in rows {
                    store.append_row(&row.table, &row.fields, row.ts)?;
                }
                Ok(())
            }
            EventsInner::Remote(client) => {
                let mut insert = client.insert_events().await?;
                for row in rows {
                    insert
                        .write(&EventInsertRow {
                            table_name: row.table.clone(),
                            fields: row.fields.to_string(),
                            ts: datetime_to_ch_ts(row.ts),
                            correlation_id: row.correlation_id.clone(),
                        })
                        .await
                        .map_err(map_remote)?;
                }
                insert.end().await.map_err(map_remote)?;
                Ok(())
            }
        }
    }

    async fn query_rows(&self, filter: EventsQueryFilter) -> Result<Vec<EventRow>> {
        match &self.inner {
            EventsInner::Mem(store) => store.query_rows(filter),
            EventsInner::Remote(client) => {
                let scope = query_sql::scope_clause(&filter);
                let extra = query_sql::filter_where_clause(&filter.filter);
                let order = query_sql::order_clause(&filter);
                let limit = filter.limit.unwrap_or(1000);
                let offset = filter.offset.unwrap_or(0);
                let paging = query_sql::limit_offset_clause(limit, offset);
                let sql = format!(
                    "SELECT fields, ts FROM spectra_events WHERE {scope}{extra} {order} {paging}"
                );
                let rows = client.query_event_rows(&sql).await?;
                let mut out = Vec::new();
                for (fields, ts) in rows {
                    out.push(EventRow {
                        ts: parse_rfc3339_ts(&ts)?,
                        fields: serde_json::from_str(&fields)?,
                    });
                }
                Ok(out)
            }
        }
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
