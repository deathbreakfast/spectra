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
