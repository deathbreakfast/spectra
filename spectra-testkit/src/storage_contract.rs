//! Shared [`MetricsStorageBackend`] and [`EventStorageBackend`] contract checks for adapters.

use std::sync::Arc;

use chrono::{Duration, Utc};
use serde_json::json;
use spectra::spectra_core::LabelMatcher;
use spectra::spectra_core::{
    EventStorageBackend, EventsQueryFilter, GridFilterItem, GridFilterModel, GridFilterOperator,
    MetricsQueryRange, MetricsStorageBackend, Result,
};

/// Run the shared storage port contract against one metrics/events backend pair.
pub async fn run_storage_contract(
    metrics: Arc<dyn MetricsStorageBackend>,
    events: Arc<dyn EventStorageBackend>,
) -> Result<()> {
    counter_roundtrip(metrics.as_ref()).await?;
    gauge_roundtrip(metrics.as_ref()).await?;
    event_roundtrip(events.as_ref()).await?;
    event_equals_filter(events.as_ref()).await?;
    event_contains_filter(events.as_ref()).await?;
    event_sort_and_pagination(events.as_ref()).await?;
    event_partition_filter(events.as_ref()).await?;
    label_filter_hit(metrics.as_ref()).await?;
    label_filter_miss(metrics.as_ref()).await?;
    time_range_empty(metrics.as_ref()).await?;
    Ok(())
}

async fn counter_roundtrip(metrics: &dyn MetricsStorageBackend) -> Result<()> {
    let ts = Utc::now();
    metrics
        .record_counter("contract_hits", &json!({"env": "test"}), 3, ts)
        .await?;
    let points = metrics
        .query_range(MetricsQueryRange {
            metric_name: "contract_hits".into(),
            start: ts - Duration::seconds(1),
            end: ts + Duration::seconds(1),
            label_matchers: vec![],
        })
        .await?;
    assert_eq!(points.len(), 1, "counter roundtrip");
    assert!((points[0].value - 3.0).abs() < f64::EPSILON);
    Ok(())
}

async fn gauge_roundtrip(metrics: &dyn MetricsStorageBackend) -> Result<()> {
    let ts = Utc::now();
    metrics
        .record_gauge("contract_load", &json!({"host": "a"}), 0.75, ts)
        .await?;
    let points = metrics
        .query_range(MetricsQueryRange {
            metric_name: "contract_load".into(),
            start: ts - Duration::seconds(1),
            end: ts + Duration::seconds(1),
            label_matchers: vec![],
        })
        .await?;
    assert_eq!(points.len(), 1, "gauge roundtrip");
    assert!((points[0].value - 0.75).abs() < f64::EPSILON);
    Ok(())
}

async fn event_roundtrip(events: &dyn EventStorageBackend) -> Result<()> {
    let ts = Utc::now();
    events
        .append_row("contract_log", &json!({"msg": "ok"}), ts, None)
        .await?;
    let rows = events
        .query_rows(EventsQueryFilter {
            table: "contract_log".into(),
            start: Some(ts - Duration::seconds(1)),
            end: Some(ts + Duration::seconds(1)),
            ..Default::default()
        })
        .await?;
    assert_eq!(rows.len(), 1, "event roundtrip");
    Ok(())
}

async fn event_equals_filter(events: &dyn EventStorageBackend) -> Result<()> {
    let ts = Utc::now();
    let table = "contract_filter_eq";
    events
        .append_row(table, &json!({"region": "us-west"}), ts, None)
        .await?;
    events
        .append_row(table, &json!({"region": "eu-central"}), ts, None)
        .await?;
    let rows = events
        .query_rows(EventsQueryFilter {
            table: table.into(),
            start: Some(ts - Duration::seconds(1)),
            end: Some(ts + Duration::seconds(1)),
            filter: GridFilterModel {
                items: vec![GridFilterItem {
                    field: "region".into(),
                    operator: GridFilterOperator::Equals,
                    value: json!("us-west"),
                }],
                ..Default::default()
            },
            ..Default::default()
        })
        .await?;
    assert_eq!(rows.len(), 1, "event equals filter");
    assert_eq!(rows[0].fields["region"], "us-west");
    Ok(())
}

async fn event_contains_filter(events: &dyn EventStorageBackend) -> Result<()> {
    let ts = Utc::now();
    let table = "contract_filter_contains";
    events
        .append_row(table, &json!({"msg": "hello world"}), ts, None)
        .await?;
    events
        .append_row(table, &json!({"msg": "goodbye"}), ts, None)
        .await?;
    let rows = events
        .query_rows(EventsQueryFilter {
            table: table.into(),
            start: Some(ts - Duration::seconds(1)),
            end: Some(ts + Duration::seconds(1)),
            filter: GridFilterModel {
                items: vec![GridFilterItem {
                    field: "msg".into(),
                    operator: GridFilterOperator::Contains,
                    value: json!("WORLD"),
                }],
                ..Default::default()
            },
            ..Default::default()
        })
        .await?;
    assert_eq!(rows.len(), 1, "event contains filter");
    Ok(())
}

async fn event_sort_and_pagination(events: &dyn EventStorageBackend) -> Result<()> {
    let ts = Utc::now();
    let table = "contract_sort_page";
    events
        .append_row(table, &json!({"name": "b"}), ts, None)
        .await?;
    events
        .append_row(table, &json!({"name": "a"}), ts + Duration::milliseconds(1), None)
        .await?;
    events
        .append_row(table, &json!({"name": "c"}), ts + Duration::milliseconds(2), None)
        .await?;
    let rows = events
        .query_rows(EventsQueryFilter {
            table: table.into(),
            start: Some(ts - Duration::seconds(1)),
            end: Some(ts + Duration::seconds(1)),
            sort_field: Some("name".into()),
            sort_desc: false,
            limit: Some(2),
            offset: Some(0),
            ..Default::default()
        })
        .await?;
    assert_eq!(rows.len(), 2, "event sort pagination length");
    assert_eq!(rows[0].fields["name"], "a");
    assert_eq!(rows[1].fields["name"], "b");
    Ok(())
}

async fn event_partition_filter(events: &dyn EventStorageBackend) -> Result<()> {
    let ts = Utc::now();
    let table = "contract_partition";
    events
        .append_row(
            table,
            &json!({"partition": "hourly", "msg": "h"}),
            ts,
            None,
        )
        .await?;
    events
        .append_row(
            table,
            &json!({"partition": "daily", "msg": "d"}),
            ts,
            None,
        )
        .await?;
    let rows = events
        .query_rows(EventsQueryFilter {
            table: table.into(),
            start: Some(ts - Duration::seconds(1)),
            end: Some(ts + Duration::seconds(1)),
            partition: Some("hourly".into()),
            ..Default::default()
        })
        .await?;
    assert_eq!(rows.len(), 1, "event partition filter");
    assert_eq!(rows[0].fields["msg"], "h");
    Ok(())
}

async fn label_filter_hit(metrics: &dyn MetricsStorageBackend) -> Result<()> {
    let ts = Utc::now();
    metrics
        .record_counter(
            "contract_labeled",
            &json!({"region": "us-west"}),
            1,
            ts,
        )
        .await?;
    let points = metrics
        .query_range(MetricsQueryRange {
            metric_name: "contract_labeled".into(),
            start: ts - Duration::seconds(1),
            end: ts + Duration::seconds(1),
            label_matchers: vec![LabelMatcher {
                key: "region".into(),
                value: "us-west".into(),
            }],
        })
        .await?;
    assert_eq!(points.len(), 1, "label filter hit");
    Ok(())
}

async fn label_filter_miss(metrics: &dyn MetricsStorageBackend) -> Result<()> {
    let ts = Utc::now();
    metrics
        .record_counter(
            "contract_labeled_miss",
            &json!({"region": "us-west"}),
            1,
            ts,
        )
        .await?;
    let points = metrics
        .query_range(MetricsQueryRange {
            metric_name: "contract_labeled_miss".into(),
            start: ts - Duration::seconds(1),
            end: ts + Duration::seconds(1),
            label_matchers: vec![LabelMatcher {
                key: "region".into(),
                value: "eu-central".into(),
            }],
        })
        .await?;
    assert!(points.is_empty(), "label filter miss");
    Ok(())
}

async fn time_range_empty(metrics: &dyn MetricsStorageBackend) -> Result<()> {
    let ts = Utc::now();
    metrics
        .record_counter("contract_time_range", &json!({}), 1, ts)
        .await?;
    let points = metrics
        .query_range(MetricsQueryRange {
            metric_name: "contract_time_range".into(),
            start: ts - Duration::hours(2),
            end: ts - Duration::hours(1),
            label_matchers: vec![],
        })
        .await?;
    assert!(points.is_empty(), "time range empty");
    Ok(())
}
