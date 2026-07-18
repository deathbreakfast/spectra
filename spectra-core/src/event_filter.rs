//! Shared event-row filter, sort, and pagination for storage backends.
//!
//! Backends that load candidate rows in process (mem, sqlite post-fetch, remote mem store)
//! should call [`finalize_event_rows`] after table/time scoping so filter/sort semantics
//! match across adapters. Remote SQL builders mirror the same operators in
//! `spectra-backend-remote-common`.

use serde_json::Value;

use crate::query::{GridFilterItem, GridFilterModel, GridFilterOperator, GridLogicOperator};
use crate::storage::{EventRow, EventsQueryFilter};

/// Apply grid filter, partition, sort, and pagination to table/time-scoped rows.
pub fn finalize_event_rows(mut rows: Vec<EventRow>, filter: &EventsQueryFilter) -> Vec<EventRow> {
    rows.retain(|r| row_matches_partition(r, filter.partition.as_deref()));
    rows.retain(|r| row_matches_filter(r, &filter.filter));
    sort_event_rows(&mut rows, filter.sort_field.as_deref(), filter.sort_desc);
    paginate_event_rows(rows, filter.limit, filter.offset)
}

/// Whether `row` matches optional `partition` against `fields.partition`.
pub fn row_matches_partition(row: &EventRow, partition: Option<&str>) -> bool {
    match partition {
        None => true,
        Some(want) => row
            .fields
            .get("partition")
            .and_then(|v| v.as_str())
            .map(|p| p == want)
            .unwrap_or(false),
    }
}

/// Whether `row` matches the full grid filter model (items + quick filter).
pub fn row_matches_filter(row: &EventRow, filter: &GridFilterModel) -> bool {
    let items_ok = if filter.items.is_empty() {
        true
    } else {
        match filter.logic_operator {
            GridLogicOperator::And => filter.items.iter().all(|i| matches_filter_item(row, i)),
            GridLogicOperator::Or => filter.items.iter().any(|i| matches_filter_item(row, i)),
        }
    };
    if !items_ok {
        return false;
    }
    if filter.quick_filter_values.is_empty() {
        return true;
    }
    let haystack = row_quick_filter_haystack(row);
    filter.quick_filter_values.iter().all(|q| {
        let q = q.to_lowercase();
        haystack.contains(&q)
    })
}

fn row_quick_filter_haystack(row: &EventRow) -> String {
    let mut s = row.ts.to_rfc3339();
    s.push(' ');
    s.push_str(&row.fields.to_string());
    s.to_lowercase()
}

/// Evaluate one filter item against a row.
pub fn matches_filter_item(row: &EventRow, item: &GridFilterItem) -> bool {
    let field_str = field_as_string(row, &item.field);
    let field_num = field_as_f64(row, &item.field);
    match item.operator {
        GridFilterOperator::Equals => {
            value_as_string(&item.value).is_some_and(|v| field_str.as_deref() == Some(v.as_str()))
        }
        GridFilterOperator::DoesNotEqual => {
            value_as_string(&item.value).is_some_and(|v| field_str.as_deref() != Some(v.as_str()))
        }
        GridFilterOperator::Contains => value_as_string(&item.value).is_some_and(|v| {
            field_str
                .as_ref()
                .is_some_and(|f| f.to_lowercase().contains(&v.to_lowercase()))
        }),
        GridFilterOperator::StartsWith => value_as_string(&item.value).is_some_and(|v| {
            field_str
                .as_ref()
                .is_some_and(|f| f.to_lowercase().starts_with(&v.to_lowercase()))
        }),
        GridFilterOperator::EndsWith => value_as_string(&item.value).is_some_and(|v| {
            field_str
                .as_ref()
                .is_some_and(|f| f.to_lowercase().ends_with(&v.to_lowercase()))
        }),
        GridFilterOperator::IsEmpty => {
            field_str.as_ref().is_none_or(|s| s.is_empty())
        }
        GridFilterOperator::IsNotEmpty => {
            field_str.as_ref().is_some_and(|s| !s.is_empty())
        }
        GridFilterOperator::GreaterThan => cmp_ordered(field_num, field_str.as_deref(), &item.value, std::cmp::Ordering::Greater, false),
        GridFilterOperator::GreaterThanOrEqual => {
            cmp_ordered(field_num, field_str.as_deref(), &item.value, std::cmp::Ordering::Greater, true)
        }
        GridFilterOperator::LessThan => {
            cmp_ordered(field_num, field_str.as_deref(), &item.value, std::cmp::Ordering::Less, false)
        }
        GridFilterOperator::LessThanOrEqual => {
            cmp_ordered(field_num, field_str.as_deref(), &item.value, std::cmp::Ordering::Less, true)
        }
    }
}

fn cmp_ordered(
    field_num: Option<f64>,
    field_str: Option<&str>,
    value: &Value,
    want: std::cmp::Ordering,
    or_eq: bool,
) -> bool {
    if let (Some(a), Some(b)) = (field_num, value_as_f64(value)) {
        let ord = a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal);
        return ord == want || (or_eq && ord == std::cmp::Ordering::Equal);
    }
    let Some(a) = field_str else {
        return false;
    };
    let Some(b) = value_as_string(value) else {
        return false;
    };
    let ord = a.cmp(&b);
    ord == want || (or_eq && ord == std::cmp::Ordering::Equal)
}

/// Sort rows by `sort_field` (`None` or `"ts"` → timestamp; else JSON field string).
///
/// Unknown / missing JSON fields sort as empty string. Falls back to `ts` as tiebreaker.
pub fn sort_event_rows(rows: &mut [EventRow], sort_field: Option<&str>, sort_desc: bool) {
    let field = sort_field.unwrap_or("ts");
    rows.sort_by(|a, b| {
        let primary = if field == "ts" {
            a.ts.cmp(&b.ts)
        } else {
            field_as_string(a, field)
                .unwrap_or_default()
                .cmp(&field_as_string(b, field).unwrap_or_default())
                .then_with(|| a.ts.cmp(&b.ts))
        };
        if sort_desc {
            primary.reverse()
        } else {
            primary
        }
    });
}

/// Apply limit/offset pagination.
pub fn paginate_event_rows(
    mut rows: Vec<EventRow>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Vec<EventRow> {
    if let Some(offset) = offset {
        let off = offset as usize;
        if off >= rows.len() {
            return Vec::new();
        }
        rows = rows.split_off(off);
    }
    if let Some(limit) = limit {
        rows.truncate(limit as usize);
    }
    rows
}

fn field_as_string(row: &EventRow, field: &str) -> Option<String> {
    if field == "ts" {
        return Some(row.ts.to_rfc3339());
    }
    row.fields.get(field).and_then(value_as_string)
}

fn field_as_f64(row: &EventRow, field: &str) -> Option<f64> {
    if field == "ts" {
        return Some(row.ts.timestamp() as f64);
    }
    row.fields.get(field).and_then(value_as_f64)
}

fn value_as_string(v: &Value) -> Option<String> {
    match v {
        Value::Null => None,
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => Some(v.to_string()),
    }
}

fn value_as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, TimeZone, Utc};
    use serde_json::json;

    fn row(ts: DateTime<Utc>, fields: Value) -> EventRow {
        EventRow { ts, fields }
    }

    fn base_ts() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap()
    }

    fn item(field: &str, op: GridFilterOperator, value: Value) -> GridFilterItem {
        GridFilterItem {
            field: field.into(),
            operator: op,
            value,
        }
    }

    #[test]
    fn equals_and_contains() {
        let r = row(base_ts(), json!({"msg": "Hello World", "n": 10}));
        assert!(matches_filter_item(
            &r,
            &item("msg", GridFilterOperator::Equals, json!("Hello World"))
        ));
        assert!(matches_filter_item(
            &r,
            &item("msg", GridFilterOperator::Contains, json!("world"))
        ));
        assert!(matches_filter_item(
            &r,
            &item("msg", GridFilterOperator::StartsWith, json!("hello"))
        ));
        assert!(matches_filter_item(
            &r,
            &item("msg", GridFilterOperator::EndsWith, json!("World"))
        ));
        assert!(matches_filter_item(
            &r,
            &item("msg", GridFilterOperator::DoesNotEqual, json!("other"))
        ));
    }

    #[test]
    fn empty_and_numeric() {
        let r = row(base_ts(), json!({"msg": "", "n": 5}));
        assert!(matches_filter_item(
            &r,
            &item("msg", GridFilterOperator::IsEmpty, json!(null))
        ));
        assert!(matches_filter_item(
            &r,
            &item("n", GridFilterOperator::IsNotEmpty, json!(null))
        ));
        assert!(matches_filter_item(
            &r,
            &item("n", GridFilterOperator::GreaterThan, json!(3))
        ));
        assert!(matches_filter_item(
            &r,
            &item("n", GridFilterOperator::GreaterThanOrEqual, json!(5))
        ));
        assert!(matches_filter_item(
            &r,
            &item("n", GridFilterOperator::LessThan, json!(6))
        ));
        assert!(matches_filter_item(
            &r,
            &item("n", GridFilterOperator::LessThanOrEqual, json!(5))
        ));
    }

    #[test]
    fn logic_or_and_quick_filter() {
        let r = row(base_ts(), json!({"region": "us-west", "msg": "ok"}));
        let model = GridFilterModel {
            items: vec![
                item("region", GridFilterOperator::Equals, json!("eu")),
                item("region", GridFilterOperator::Equals, json!("us-west")),
            ],
            logic_operator: GridLogicOperator::Or,
            quick_filter_values: vec!["west".into()],
        };
        assert!(row_matches_filter(&r, &model));
    }

    #[test]
    fn partition_and_sort_by_field() {
        let t0 = base_ts();
        let t1 = t0 + chrono::Duration::seconds(1);
        let mut rows = vec![
            row(t1, json!({"name": "b", "partition": "hourly"})),
            row(t0, json!({"name": "a", "partition": "daily"})),
            row(t0 + chrono::Duration::seconds(2), json!({"name": "c", "partition": "hourly"})),
        ];
        assert!(!row_matches_partition(&rows[1], Some("hourly")));
        rows.retain(|r| row_matches_partition(r, Some("hourly")));
        sort_event_rows(&mut rows, Some("name"), false);
        assert_eq!(rows[0].fields["name"], "b");
        assert_eq!(rows[1].fields["name"], "c");
    }

    #[test]
    fn finalize_applies_all() {
        let t0 = base_ts();
        let rows = vec![
            row(t0, json!({"msg": "keep", "partition": "hourly"})),
            row(t0 + chrono::Duration::seconds(1), json!({"msg": "drop", "partition": "hourly"})),
            row(t0 + chrono::Duration::seconds(2), json!({"msg": "keep", "partition": "daily"})),
        ];
        let filter = EventsQueryFilter {
            table: "t".into(),
            partition: Some("hourly".into()),
            sort_field: Some("msg".into()),
            sort_desc: true,
            limit: Some(10),
            filter: GridFilterModel {
                items: vec![item("msg", GridFilterOperator::Equals, json!("keep"))],
                ..Default::default()
            },
            ..Default::default()
        };
        let out = finalize_event_rows(rows, &filter);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].fields["msg"], "keep");
    }
}
