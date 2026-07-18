//! Query DTO serialization and mapping tests.

use chrono::Utc;
use spectra_core::{
    event_query_to_filter, metrics_query_to_range, EventQuery, GridFilterModel,
    GridPaginationModel, GridSortDirection, GridSortItem, MetricsQuery,
};

#[test]
fn event_query_maps_to_filter() {
    let now = Utc::now();
    let q = EventQuery {
        table: "platform_smoke_event".into(),
        start: now - chrono::Duration::hours(1),
        end: now,
        partition: None,
        pagination: GridPaginationModel {
            page: 1,
            page_size: 25,
        },
        sort: vec![GridSortItem {
            field: "ts".into(),
            sort: GridSortDirection::Desc,
        }],
        filter: GridFilterModel::default(),
    };
    let f = event_query_to_filter(&q);
    assert_eq!(f.table, "platform_smoke_event");
    assert_eq!(f.limit, Some(25));
    assert_eq!(f.offset, Some(25));
    assert!(f.sort_desc);
}

#[test]
fn metrics_query_maps_to_range() {
    let now = Utc::now();
    let q = MetricsQuery {
        metric: "request_count".into(),
        start: now - chrono::Duration::hours(1),
        end: now,
        step_secs: Some(60),
        label_matchers: vec![],
    };
    let r = metrics_query_to_range(&q);
    assert_eq!(r.metric_name, "request_count");
}
