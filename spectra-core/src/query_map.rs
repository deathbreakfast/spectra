//! Map UI query DTOs to adapter filters and results.

use chrono::Utc;
use serde_json::{json, Value};

use crate::query::{
    EventAggregateRequest, EventAggregateResult, EventExploreView, EventGridRow, EventMeasure,
    EventQuery, EventQueryResult, GridColumnDto, GridSortDirection, MetricsQuery,
    MetricsQueryResult, SchemaDetailDto, SchemaFieldDto, SchemaListItem, SliceDto, StatCardDto,
    TimeSeriesDto,
};
use crate::registry::{LoggingKind, SchemaMetadata, SchemaRegistry};
use crate::storage::{EventRow, EventsAggregateFilter, EventsQueryFilter, MetricPoint};

/// Converts a UI event query DTO into a storage adapter filter.
pub fn event_query_to_filter(q: &EventQuery) -> EventsQueryFilter {
    let sort = q.sort.first();
    EventsQueryFilter {
        table: q.table.clone(),
        start: Some(q.start),
        end: Some(q.end),
        partition: q.partition.map(|p| match p {
            crate::query::PartitionKind::Hourly => "hourly".to_string(),
            crate::query::PartitionKind::Daily => "daily".to_string(),
        }),
        limit: Some(q.pagination.page_size),
        offset: Some(q.pagination.page * q.pagination.page_size),
        sort_field: sort.map(|s| s.field.clone()),
        sort_desc: sort
            .map(|s| s.sort == GridSortDirection::Desc)
            .unwrap_or(true),
        filter: q.filter.clone(),
    }
}

/// Converts an aggregate request DTO into a storage adapter filter.
pub fn aggregate_request_to_filter(req: &EventAggregateRequest) -> EventsAggregateFilter {
    EventsAggregateFilter {
        table: req.table.clone(),
        start: req.start,
        end: req.end,
        partition: req.partition.map(|p| match p {
            crate::query::PartitionKind::Hourly => "hourly".to_string(),
            crate::query::PartitionKind::Daily => "daily".to_string(),
        }),
        filter: req.filter.clone(),
        measure: req.aggregation.measure,
        measure_field: req.aggregation.measure_field.clone(),
        time_bucket_secs: req.aggregation.time_bucket_secs,
        group_by_field: req.aggregation.group_by_field.clone(),
        view: req.view,
    }
}

/// Maps storage event rows into a UI event query result.
pub fn rows_to_event_result(table: &str, rows: Vec<EventRow>, row_count: u64) -> EventQueryResult {
    let meta = SchemaRegistry::global().get_schema(table);
    let columns = meta.map(schema_columns).unwrap_or_else(|| {
        vec![GridColumnDto {
            field: "ts".into(),
            header_name: "Timestamp".into(),
        }]
    });
    let grid_rows = rows
        .into_iter()
        .enumerate()
        .map(|(i, r)| {
            let id = r
                .fields
                .get("id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("row-{i}"));
            EventGridRow {
                id,
                ts: r.ts,
                fields: r.fields,
            }
        })
        .collect();
    EventQueryResult {
        rows: grid_rows,
        columns,
        row_count,
    }
}

fn schema_columns(meta: &SchemaMetadata) -> Vec<GridColumnDto> {
    let mut cols = vec![GridColumnDto {
        field: "ts".into(),
        header_name: "Timestamp".into(),
    }];
    for f in &meta.fields {
        cols.push(GridColumnDto {
            field: f.name.clone(),
            header_name: f.name.clone(),
        });
    }
    cols
}

/// Lists all registered schemas as catalog DTOs.
pub fn list_schemas() -> Vec<SchemaListItem> {
    SchemaRegistry::global()
        .list_schemas()
        .into_iter()
        .filter_map(|name| {
            SchemaRegistry::global()
                .get_schema(name)
                .map(|m| schema_list_item(m))
        })
        .collect()
}

fn schema_list_item(m: &SchemaMetadata) -> SchemaListItem {
    SchemaListItem {
        table_or_metric: m.table_or_metric.clone(),
        description: m.description.clone(),
        logging_kind: match m.logging_kind {
            LoggingKind::Event => "event".into(),
            LoggingKind::Metric => "metric".into(),
        },
        can_query: true,
    }
}

/// Returns full schema detail for a table or metric name.
pub fn schema_detail(name: &str) -> Option<SchemaDetailDto> {
    SchemaRegistry::global()
        .get_schema(name)
        .map(|m| SchemaDetailDto {
            table_or_metric: m.table_or_metric.clone(),
            description: m.description.clone(),
            logging_kind: match m.logging_kind {
                LoggingKind::Event => "event".into(),
                LoggingKind::Metric => "metric".into(),
            },
            version: m.version.clone(),
            fields: m
                .fields
                .iter()
                .map(|f| SchemaFieldDto {
                    name: f.name.clone(),
                    rust_type: f.rust_type.clone(),
                    classification: format!("{:?}", f.classification),
                })
                .collect(),
            can_query: true,
        })
}

/// Converts a UI metrics query DTO into a storage range filter.
pub fn metrics_query_to_range(q: &MetricsQuery) -> crate::storage::MetricsQueryRange {
    crate::storage::MetricsQueryRange {
        metric_name: q.metric.clone(),
        start: q.start,
        end: q.end,
        label_matchers: q.label_matchers.clone(),
    }
}

/// Maps storage metric points into a UI metrics query result.
pub fn points_to_metrics_result(points: Vec<MetricPoint>) -> MetricsQueryResult {
    let series = vec![TimeSeriesDto {
        labels: json!({}),
        points: points
            .iter()
            .map(|p| crate::query::MetricPointDto {
                ts: p.ts,
                value: p.value,
            })
            .collect(),
    }];
    let headline = metrics_headline(&points);
    MetricsQueryResult { series, headline }
}

fn metrics_headline(points: &[MetricPoint]) -> Vec<StatCardDto> {
    if points.is_empty() {
        return vec![
            StatCardDto {
                label: "Points".into(),
                value: "0".into(),
            },
            StatCardDto {
                label: "Max".into(),
                value: "—".into(),
            },
            StatCardDto {
                label: "Last".into(),
                value: "—".into(),
            },
        ];
    }
    let max = points
        .iter()
        .map(|p| p.value)
        .fold(f64::NEG_INFINITY, f64::max);
    let last = points.last().map(|p| p.value).unwrap_or(0.0);
    vec![
        StatCardDto {
            label: "Points".into(),
            value: points.len().to_string(),
        },
        StatCardDto {
            label: "Max".into(),
            value: format!("{max:.2}"),
        },
        StatCardDto {
            label: "Last".into(),
            value: format!("{last:.2}"),
        },
    ]
}

/// Maps aggregate storage rows into a UI chart result for the requested view.
pub fn aggregate_rows_to_result(
    view: EventExploreView,
    rows: Vec<Value>,
    measure: EventMeasure,
) -> EventAggregateResult {
    match view {
        EventExploreView::TimeSeries | EventExploreView::LineChart => {
            let mut points = Vec::new();
            for row in &rows {
                let bucket = row
                    .get("bucket")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let value = row.get("value").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let ts = chrono::DateTime::parse_from_rfc3339(&bucket)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());
                points.push(crate::query::MetricPointDto { ts, value });
            }
            let headline = vec![
                StatCardDto {
                    label: "Buckets".into(),
                    value: points.len().to_string(),
                },
                StatCardDto {
                    label: "Total".into(),
                    value: format!("{:.0}", points.iter().map(|p| p.value).sum::<f64>()),
                },
                StatCardDto {
                    label: "Measure".into(),
                    value: format!("{measure:?}"),
                },
            ];
            EventAggregateResult::TimeSeries {
                series: vec![TimeSeriesDto {
                    labels: json!({}),
                    points,
                }],
                headline,
            }
        }
        EventExploreView::PieChart | EventExploreView::BarChart => {
            let slices: Vec<SliceDto> = rows
                .iter()
                .map(|row| SliceDto {
                    label: row
                        .get("label")
                        .and_then(|v| v.as_str())
                        .unwrap_or("?")
                        .to_string(),
                    value: row.get("value").and_then(|v| v.as_f64()).unwrap_or(0.0),
                })
                .collect();
            let total: f64 = slices.iter().map(|s| s.value).sum();
            let headline = vec![
                StatCardDto {
                    label: "Slices".into(),
                    value: slices.len().to_string(),
                },
                StatCardDto {
                    label: "Total".into(),
                    value: format!("{total:.0}"),
                },
                StatCardDto {
                    label: "Measure".into(),
                    value: format!("{measure:?}"),
                },
            ];
            EventAggregateResult::Slices { slices, headline }
        }
        EventExploreView::EventLog => EventAggregateResult::TimeSeries {
            series: Vec::new(),
            headline: Vec::new(),
        },
    }
}

/// Aggregates in-memory event rows into a chart result for `view`.
///
/// Mem and SQLite backends call this after [`crate::EventStorageBackend::query_rows`].
/// Pie/Bar without `group_by_field` return empty slices. Sum without a numeric
/// `measure_field` contributes `0.0` per row.
///
/// # Examples
///
/// ```
/// use chrono::{Duration, Utc};
/// use serde_json::json;
/// use spectra_core::{
///     aggregate_event_rows, EventExploreView, EventMeasure, EventRow, EventsAggregateFilter,
///     EventAggregateResult, GridFilterModel,
/// };
///
/// let end = Utc::now();
/// let start = end - Duration::hours(1);
/// let rows = vec![
///     EventRow {
///         ts: end - Duration::minutes(5),
///         fields: json!({"severity": "info", "value": 10}),
///     },
///     EventRow {
///         ts: end - Duration::minutes(4),
///         fields: json!({"severity": "warn", "value": 3}),
///     },
/// ];
/// let filter = EventsAggregateFilter {
///     table: "demo.events".into(),
///     start,
///     end,
///     partition: None,
///     filter: GridFilterModel::default(),
///     measure: EventMeasure::Count,
///     measure_field: None,
///     time_bucket_secs: None,
///     group_by_field: Some("severity".into()),
///     view: EventExploreView::PieChart,
/// };
/// let result = aggregate_event_rows(filter.view, &filter, &rows);
/// match result {
///     EventAggregateResult::Slices { slices, .. } => {
///         assert_eq!(slices.len(), 2);
///     }
///     other => panic!("expected slices, got {other:?}"),
/// }
/// ```
pub fn aggregate_event_rows(
    view: EventExploreView,
    filter: &EventsAggregateFilter,
    rows: &[EventRow],
) -> EventAggregateResult {
    match view {
        EventExploreView::EventLog => EventAggregateResult::TimeSeries {
            series: Vec::new(),
            headline: Vec::new(),
        },
        EventExploreView::TimeSeries | EventExploreView::LineChart => {
            let bucket_secs = filter.time_bucket_secs.unwrap_or(3600).max(1);
            let mut buckets: std::collections::BTreeMap<i64, f64> =
                std::collections::BTreeMap::new();
            for row in rows {
                let aligned = align_bucket_ts(row.ts, bucket_secs);
                let contrib =
                    row_measure_value(row, filter.measure, filter.measure_field.as_deref());
                *buckets.entry(aligned).or_insert(0.0) += contrib;
            }
            let mapped: Vec<Value> = buckets
                .into_iter()
                .map(|(epoch, value)| {
                    let ts =
                        chrono::DateTime::<Utc>::from_timestamp(epoch, 0).unwrap_or(filter.end);
                    json!({
                        "bucket": ts.to_rfc3339(),
                        "value": value,
                    })
                })
                .collect();
            aggregate_rows_to_result(view, mapped, filter.measure)
        }
        EventExploreView::PieChart | EventExploreView::BarChart => {
            let Some(group_field) = filter
                .group_by_field
                .as_ref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
            else {
                return EventAggregateResult::Slices {
                    slices: Vec::new(),
                    headline: Vec::new(),
                };
            };
            let mut groups: std::collections::BTreeMap<String, f64> =
                std::collections::BTreeMap::new();
            let mut saw_field = false;
            for row in rows {
                let Some(raw) = row.fields.get(group_field) else {
                    continue;
                };
                saw_field = true;
                let label = match raw {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                let contrib =
                    row_measure_value(row, filter.measure, filter.measure_field.as_deref());
                *groups.entry(label).or_insert(0.0) += contrib;
            }
            if !saw_field {
                return EventAggregateResult::Slices {
                    slices: Vec::new(),
                    headline: Vec::new(),
                };
            }
            let mapped: Vec<Value> = groups
                .into_iter()
                .map(|(label, value)| json!({ "label": label, "value": value }))
                .collect();
            aggregate_rows_to_result(view, mapped, filter.measure)
        }
    }
}

fn align_bucket_ts(ts: chrono::DateTime<Utc>, bucket_secs: u64) -> i64 {
    let secs = bucket_secs as i64;
    let epoch = ts.timestamp();
    epoch - epoch.rem_euclid(secs)
}

fn row_measure_value(row: &EventRow, measure: EventMeasure, measure_field: Option<&str>) -> f64 {
    match measure {
        EventMeasure::Count => 1.0,
        EventMeasure::Sum => measure_field
            .and_then(|field| row.fields.get(field))
            .and_then(|v| v.as_f64().or_else(|| v.as_i64().map(|i| i as f64)))
            .unwrap_or(0.0),
    }
}

#[cfg(test)]
mod aggregate_event_rows_tests {
    use super::*;
    use crate::query::GridFilterModel;
    use chrono::Duration;

    fn filter(
        view: EventExploreView,
        measure: EventMeasure,
        measure_field: Option<&str>,
        group_by: Option<&str>,
        bucket: Option<u64>,
    ) -> EventsAggregateFilter {
        let end = Utc::now();
        EventsAggregateFilter {
            table: "t".into(),
            start: end - Duration::hours(1),
            end,
            partition: None,
            filter: GridFilterModel::default(),
            measure,
            measure_field: measure_field.map(str::to_string),
            time_bucket_secs: bucket,
            group_by_field: group_by.map(str::to_string),
            view,
        }
    }

    fn row(minutes_ago: i64, severity: &str, value: i64) -> EventRow {
        EventRow {
            ts: Utc::now() - Duration::minutes(minutes_ago),
            fields: json!({"severity": severity, "value": value}),
        }
    }

    #[test]
    fn aggregate_event_rows_count_timeseries_happy() {
        let f = filter(
            EventExploreView::TimeSeries,
            EventMeasure::Count,
            None,
            None,
            Some(3600),
        );
        let rows = vec![row(5, "info", 10), row(4, "warn", 3), row(3, "info", 5)];
        match aggregate_event_rows(f.view, &f, &rows) {
            EventAggregateResult::TimeSeries { series, .. } => {
                let total: f64 = series
                    .iter()
                    .flat_map(|s| s.points.iter())
                    .map(|p| p.value)
                    .sum();
                assert!((total - 3.0).abs() < f64::EPSILON, "total={total}");
            }
            other => panic!("expected time series: {other:?}"),
        }
    }

    #[test]
    fn aggregate_event_rows_sum_timeseries_happy() {
        let f = filter(
            EventExploreView::LineChart,
            EventMeasure::Sum,
            Some("value"),
            None,
            Some(3600),
        );
        let rows = vec![row(5, "info", 10), row(4, "warn", 3), row(3, "info", 5)];
        match aggregate_event_rows(f.view, &f, &rows) {
            EventAggregateResult::TimeSeries { series, .. } => {
                let total: f64 = series
                    .iter()
                    .flat_map(|s| s.points.iter())
                    .map(|p| p.value)
                    .sum();
                assert!((total - 18.0).abs() < f64::EPSILON, "total={total}");
            }
            other => panic!("expected time series: {other:?}"),
        }
    }

    #[test]
    fn aggregate_event_rows_group_by_slices_happy() {
        let f = filter(
            EventExploreView::PieChart,
            EventMeasure::Count,
            None,
            Some("severity"),
            None,
        );
        let rows = vec![row(5, "info", 10), row(4, "warn", 3), row(3, "info", 5)];
        match aggregate_event_rows(f.view, &f, &rows) {
            EventAggregateResult::Slices { slices, .. } => {
                assert!(slices.len() >= 2, "{slices:?}");
                let labels: Vec<_> = slices.iter().map(|s| s.label.as_str()).collect();
                assert!(labels.contains(&"info"));
                assert!(labels.contains(&"warn"));
            }
            other => panic!("expected slices: {other:?}"),
        }
    }

    #[test]
    fn aggregate_event_rows_pie_missing_groupby_sad() {
        let f = filter(
            EventExploreView::PieChart,
            EventMeasure::Count,
            None,
            None,
            None,
        );
        let rows = vec![row(1, "info", 1)];
        match aggregate_event_rows(f.view, &f, &rows) {
            EventAggregateResult::Slices { slices, headline } => {
                assert!(slices.is_empty());
                assert!(headline.is_empty());
            }
            other => panic!("expected empty slices: {other:?}"),
        }
    }

    #[test]
    fn aggregate_event_rows_empty_table_sad() {
        let f = filter(
            EventExploreView::TimeSeries,
            EventMeasure::Count,
            None,
            None,
            Some(3600),
        );
        match aggregate_event_rows(f.view, &f, &[]) {
            EventAggregateResult::TimeSeries { series, .. } => {
                assert!(
                    series.iter().all(|s| s.points.is_empty())
                        || series.is_empty()
                        || series
                            .iter()
                            .flat_map(|s| &s.points)
                            .map(|p| p.value)
                            .sum::<f64>()
                            == 0.0
                );
            }
            other => panic!("expected time series: {other:?}"),
        }
    }

    #[test]
    fn aggregate_request_to_filter_copies_view() {
        let end = Utc::now();
        let req = EventAggregateRequest {
            table: "t".into(),
            start: end - Duration::hours(1),
            end,
            partition: None,
            filter: GridFilterModel::default(),
            view: EventExploreView::BarChart,
            aggregation: crate::query::EventAggregationSpec {
                measure: EventMeasure::Count,
                measure_field: None,
                time_bucket_secs: Some(60),
                group_by_field: Some("severity".into()),
            },
        };
        let mapped = aggregate_request_to_filter(&req);
        assert_eq!(mapped.view, EventExploreView::BarChart);
        assert_eq!(mapped.group_by_field.as_deref(), Some("severity"));
    }
}
