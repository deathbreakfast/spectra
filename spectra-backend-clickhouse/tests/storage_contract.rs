//! Shared storage port contract for the clickhouse in-memory stub.

use std::sync::Arc;

use chrono::{Duration, Utc};
use serde_json::json;
use spectra_backend_clickhouse::{ClickHouseEventsBackend, ClickHouseMetricsBackend};
use spectra_core::{
    EventStorageBackend, EventsQueryFilter, MetricsQueryRange, MetricsStorageBackend,
};
use spectra_testkit::run_storage_contract;

#[tokio::test]
async fn clickhouse_storage_contract_stub() {
    let metrics = Arc::new(ClickHouseMetricsBackend::in_memory_stub());
    let events = Arc::new(ClickHouseEventsBackend::in_memory_stub());
    run_storage_contract(metrics, events)
        .await
        .expect("clickhouse storage contract");
}

#[tokio::test]
#[ignore = "requires SPECTRA_CLICKHOUSE_URL"]
async fn clickhouse_storage_contract_live() {
    if std::env::var("SPECTRA_CLICKHOUSE_URL").is_err() {
        return;
    }
    let url = std::env::var("SPECTRA_CLICKHOUSE_URL").expect("SPECTRA_CLICKHOUSE_URL");
    let metrics = Arc::new(
        ClickHouseMetricsBackend::connect(&url)
            .await
            .expect("clickhouse metrics connect"),
    );
    let events = Arc::new(
        ClickHouseEventsBackend::connect(&url)
            .await
            .expect("clickhouse events connect"),
    );
    run_storage_contract(metrics, events)
        .await
        .expect("clickhouse live storage contract");
}

/// `ALTER TABLE … DELETE` against a live server: old rows and points go, recent ones and
/// other tables stay. Runs in its own `spectra_prune_live` database because a metrics prune is
/// store-wide.
#[tokio::test]
#[ignore = "requires SPECTRA_CLICKHOUSE_URL"]
async fn clickhouse_prune_live() {
    let Ok(url) = std::env::var("SPECTRA_CLICKHOUSE_URL") else {
        return;
    };
    let events = ClickHouseEventsBackend::connect_in_store(&url, "prune_live")
        .await
        .expect("clickhouse events connect");
    let metrics = ClickHouseMetricsBackend::connect_in_store(&url, "prune_live")
        .await
        .expect("clickhouse metrics connect");

    let now = Utc::now();
    let old = now - Duration::days(10);
    let cutoff = now - Duration::days(1);
    let run = now.timestamp_micros();
    let table = format!("prune_live_{run}");
    let other = format!("prune_live_other_{run}");
    let metric = format!("prune_live_hits_{run}");

    for ts in [old, old, now] {
        events
            .append_row(&table, &json!({"run": run}), ts, None)
            .await
            .expect("append");
        metrics
            .record_counter(&metric, &json!({}), 1, ts)
            .await
            .expect("record");
    }
    events
        .append_row(&other, &json!({"run": run}), old, None)
        .await
        .expect("append other");

    events
        .prune_before(&table, cutoff, 5000)
        .await
        .expect("event prune");
    metrics
        .prune_before(cutoff, 5000)
        .await
        .expect("metrics prune");

    let rows = |t: &str| EventsQueryFilter {
        table: t.to_string(),
        start: Some(old - Duration::days(1)),
        end: Some(now + Duration::seconds(1)),
        ..Default::default()
    };
    let kept = events.query_rows(rows(&table)).await.expect("query");
    assert_eq!(kept.len(), 1, "only the recent row survives");
    let untouched = events.query_rows(rows(&other)).await.expect("query other");
    assert_eq!(untouched.len(), 1, "other table is not pruned");
    let points = metrics
        .query_range(MetricsQueryRange {
            metric_name: metric.clone(),
            start: old - Duration::days(1),
            end: now + Duration::seconds(1),
            label_matchers: vec![],
        })
        .await
        .expect("metrics query");
    assert_eq!(points.len(), 1, "only the recent point survives");
}
