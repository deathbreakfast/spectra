//! Sqlite storage survives closing and reopening the database files.

use chrono::{Duration, Utc};
use serde_json::json;
use spectra::spectra_core::{
    EventStorageBackend, EventsQueryFilter, MetricsQueryRange, MetricsStorageBackend,
};
use spectra::{SqliteEventsBackend, SqliteMetricsBackend};

#[tokio::test]
async fn sqlite_rows_survive_reopen() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let metrics_path = dir.path().join("metrics.db");
    let events_path = dir.path().join("events.db");
    let ts = Utc::now();

    {
        let metrics = SqliteMetricsBackend::new(&metrics_path).expect("open metrics");
        let events = SqliteEventsBackend::new(&events_path).expect("open events");
        metrics
            .record_counter("durable_hits", &json!({"env": "test"}), 4, ts)
            .await
            .expect("record counter");
        events
            .append_row("durable_log", &json!({"msg": "kept"}), ts, None)
            .await
            .expect("append row");
    }

    let metrics = SqliteMetricsBackend::new(&metrics_path).expect("reopen metrics");
    let events = SqliteEventsBackend::new(&events_path).expect("reopen events");
    let window = (ts - Duration::seconds(1), ts + Duration::seconds(1));

    let points = metrics
        .query_range(MetricsQueryRange {
            metric_name: "durable_hits".into(),
            start: window.0,
            end: window.1,
            label_matchers: vec![],
        })
        .await
        .expect("query metrics");
    assert_eq!(points.len(), 1);
    assert!((points[0].value - 4.0).abs() < f64::EPSILON);

    let rows = events
        .query_rows(EventsQueryFilter {
            table: "durable_log".into(),
            start: Some(window.0),
            end: Some(window.1),
            ..Default::default()
        })
        .await
        .expect("query events");
    assert_eq!(rows.len(), 1);

    let missing = metrics
        .query_range(MetricsQueryRange {
            metric_name: "durable_never".into(),
            start: window.0,
            end: window.1,
            label_matchers: vec![],
        })
        .await
        .expect("query missing metric");
    assert!(missing.is_empty());
}
