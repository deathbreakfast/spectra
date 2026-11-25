//! PII group, row and pivot fields come back from `query_event_aggregate` as stand-in labels
//! on the embedded backends, never as raw values.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use chrono::{Duration, Utc};
use serde_json::json;
use spectra::spectra_core::{
    EventAggregateResult, EventExploreView, EventMeasure, EventStorageBackend,
    EventsAggregateFilter, GridFilterModel, SharedEventBackend, SharedMetricsBackend,
    SpectraRouter,
};
use spectra::stand_in_labels;
use spectra::{MemEventsBackend, MemMetricsBackend, SqliteEventsBackend};

spectra::spectra_schema! {
    PiiOrderEvent {
        store: "default",
        table: "pii_order_event",
        version: "0.1.0",
        description: "Orders keyed by a PII customer id",
        fields: [
            customer_id: {
                r#type: String,
                classification: { pii: true, safe_for_console: false },
            },
            region: {
                r#type: String,
                classification: { pii: false, safe_for_console: true },
            },
        ],
    }
}

const CUSTOMERS: [&str; 3] = ["alice@example.com", "bob@example.com", "carol@example.com"];

async fn seed(events: &dyn EventStorageBackend) {
    let ts = Utc::now();
    for (i, customer) in CUSTOMERS.iter().enumerate() {
        for _ in 0..=i {
            events
                .append_row(
                    "pii_order_event",
                    &json!({"customer_id": customer, "region": "us"}),
                    ts,
                    None,
                )
                .await
                .expect("append");
        }
    }
}

fn filter(view: EventExploreView) -> EventsAggregateFilter {
    let now = Utc::now();
    EventsAggregateFilter {
        table: "pii_order_event".into(),
        start: now - Duration::hours(1),
        end: now + Duration::hours(1),
        partition: None,
        filter: GridFilterModel::default(),
        measure: EventMeasure::Count,
        measure_field: None,
        time_bucket_secs: None,
        group_by_field: None,
        row_fields: Vec::new(),
        pivot_field: None,
        view,
    }
}

fn assert_stand_ins(mut labels: Vec<String>) {
    for label in &labels {
        assert!(
            !CUSTOMERS.contains(&label.as_str()),
            "raw PII leaked: {label}"
        );
    }
    labels.sort();
    assert_eq!(labels, vec!["Customer 1", "Customer 2", "Customer 3"]);
}

async fn check(events: SharedEventBackend) {
    seed(events.as_ref()).await;
    let router = SpectraRouter::with_defaults(
        Arc::new(MemMetricsBackend::new()) as SharedMetricsBackend,
        events,
    );

    let mut pie = filter(EventExploreView::PieChart);
    pie.group_by_field = Some("customer_id".into());
    let EventAggregateResult::Slices { slices, .. } =
        router.query_event_aggregate(pie).await.expect("pie")
    else {
        panic!("expected slices");
    };
    let mut totals: Vec<f64> = slices.iter().map(|s| s.value).collect();
    totals.sort_by(f64::total_cmp);
    assert_eq!(totals, vec![1.0, 2.0, 3.0], "counts survive relabelling");
    assert_stand_ins(slices.into_iter().map(|s| s.label).collect());

    // With a caller's salt, `stand_in_labels` reproduces the router's labels. Customer i has
    // i + 1 rows, so each slice's count identifies its raw value.
    let mut salted = filter(EventExploreView::PieChart);
    salted.group_by_field = Some("customer_id".into());
    let EventAggregateResult::Slices { slices, .. } = router
        .query_event_aggregate_with_salt(salted, 42)
        .await
        .expect("salted pie")
    else {
        panic!("expected slices");
    };
    let expected = stand_in_labels("customer_id", CUSTOMERS, 42);
    for slice in &slices {
        let raw = CUSTOMERS[slice.value as usize - 1];
        assert_eq!(Some(slice.label.as_str()), expected.label(raw), "{raw}");
    }

    let mut table = filter(EventExploreView::Table);
    table.row_fields = vec!["region".into(), "customer_id".into()];
    let EventAggregateResult::Pivot { rows, .. } =
        router.query_event_aggregate(table).await.expect("table")
    else {
        panic!("expected pivot");
    };
    assert!(rows.iter().all(|r| r.row_values[0] == "us"), "non-PII kept");
    assert_stand_ins(rows.iter().map(|r| r.row_values[1].clone()).collect());

    let mut pivot = filter(EventExploreView::Table);
    pivot.row_fields = vec!["region".into()];
    pivot.pivot_field = Some("customer_id".into());
    let EventAggregateResult::Pivot { column_keys, .. } =
        router.query_event_aggregate(pivot).await.expect("pivot")
    else {
        panic!("expected pivot");
    };
    assert_stand_ins(column_keys);
}

#[tokio::test]
async fn mem_aggregates_use_stand_in_labels() {
    check(Arc::new(MemEventsBackend::new())).await;
}

#[tokio::test]
async fn sqlite_aggregates_use_stand_in_labels() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let events = SqliteEventsBackend::new(dir.path().join("events.db")).expect("open events");
    check(Arc::new(events)).await;
}
