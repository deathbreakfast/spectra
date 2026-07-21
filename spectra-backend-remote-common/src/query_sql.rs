//! SQL fragments for remote event queries.

use spectra_core::{EventsQueryFilter, GridFilterItem, GridFilterOperator, GridLogicOperator};

pub fn escape_str(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

pub fn scope_clause(filter: &EventsQueryFilter) -> String {
    let mut clauses = vec![format!("table_name = '{}'", escape_str(&filter.table))];
    if let Some(start) = filter.start {
        clauses.push(format!("ts >= '{}'", escape_str(&start.to_rfc3339())));
    }
    if let Some(end) = filter.end {
        clauses.push(format!("ts <= '{}'", escape_str(&end.to_rfc3339())));
    }
    if let Some(ref p) = filter.partition {
        clauses.push(format!(
            "JSONExtractString(fields, 'partition') = '{}'",
            escape_str(p)
        ));
    }
    clauses.join(" AND ")
}

pub fn filter_where_clause(filter: &spectra_core::GridFilterModel) -> String {
    if filter.items.is_empty() && filter.quick_filter_values.is_empty() {
        return String::new();
    }
    let mut parts = Vec::new();
    for item in &filter.items {
        if let Some(clause) = filter_item_clause(item) {
            parts.push(clause);
        }
    }
    if !filter.quick_filter_values.is_empty() {
        let q = filter
            .quick_filter_values
            .iter()
            .map(|v| format!("positionCaseInsensitive(fields, '{}') > 0", escape_str(v)))
            .collect::<Vec<_>>()
            .join(" OR ");
        if !q.is_empty() {
            parts.push(format!("({q})"));
        }
    }
    if parts.is_empty() {
        return String::new();
    }
    let op = match filter.logic_operator {
        GridLogicOperator::And => " AND ",
        GridLogicOperator::Or => " OR ",
    };
    format!(" AND ({})", parts.join(op))
}

fn field_path(field: &str) -> String {
    if field == "ts" {
        "ts".to_string()
    } else {
        format!("JSONExtractString(fields, '{}')", escape_str(field))
    }
}

fn filter_item_clause(item: &GridFilterItem) -> Option<String> {
    let path = field_path(&item.field);
    match item.operator {
        GridFilterOperator::Equals => item
            .value
            .as_str()
            .map(|v| format!("{path} = '{}'", escape_str(v)))
            .or_else(|| {
                item.value
                    .as_f64()
                    .map(|v| format!("toFloat64OrZero({path}) = {v}"))
            }),
        GridFilterOperator::DoesNotEqual => item
            .value
            .as_str()
            .map(|v| format!("{path} != '{}'", escape_str(v)))
            .or_else(|| {
                item.value
                    .as_f64()
                    .map(|v| format!("toFloat64OrZero({path}) != {v}"))
            }),
        GridFilterOperator::Contains => item
            .value
            .as_str()
            .map(|v| format!("positionCaseInsensitive({path}, '{}') > 0", escape_str(v))),
        GridFilterOperator::StartsWith => item
            .value
            .as_str()
            .map(|v| format!("startsWith(lower({path}), lower('{}'))", escape_str(v))),
        GridFilterOperator::EndsWith => item
            .value
            .as_str()
            .map(|v| format!("endsWith(lower({path}), lower('{}'))", escape_str(v))),
        GridFilterOperator::IsEmpty => Some(format!("({path} = '' OR isNull({path}))")),
        GridFilterOperator::IsNotEmpty => Some(format!("({path} != '' AND isNotNull({path}))")),
        GridFilterOperator::GreaterThan => item
            .value
            .as_f64()
            .map(|v| format!("toFloat64OrZero({path}) > {v}")),
        GridFilterOperator::GreaterThanOrEqual => item
            .value
            .as_f64()
            .map(|v| format!("toFloat64OrZero({path}) >= {v}")),
        GridFilterOperator::LessThan => item
            .value
            .as_f64()
            .map(|v| format!("toFloat64OrZero({path}) < {v}")),
        GridFilterOperator::LessThanOrEqual => item
            .value
            .as_f64()
            .map(|v| format!("toFloat64OrZero({path}) <= {v}")),
    }
}

pub fn order_clause(filter: &EventsQueryFilter) -> String {
    let dir = if filter.sort_desc { "DESC" } else { "ASC" };
    let field = filter.sort_field.as_deref().unwrap_or("ts");
    if field == "ts" {
        format!("ORDER BY ts {dir}")
    } else {
        format!(
            "ORDER BY JSONExtractString(fields, '{}') {dir}, ts {dir}",
            escape_str(field)
        )
    }
}

/// `LIMIT` clause; omits `OFFSET 0` for TensorBase SQL compatibility.
pub fn limit_offset_clause(limit: u32, offset: u32) -> String {
    if offset == 0 {
        format!("LIMIT {limit}")
    } else {
        format!("LIMIT {limit} OFFSET {offset}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use spectra_core::{EventsQueryFilter, GridFilterItem, GridFilterModel, GridFilterOperator};

    #[test]
    fn scope_includes_table() {
        let filter = EventsQueryFilter {
            table: "req_log".into(),
            ..Default::default()
        };
        assert!(scope_clause(&filter).contains("table_name = 'req_log'"));
    }

    #[test]
    fn filter_covers_all_operators() {
        let ops = [
            (GridFilterOperator::Equals, json!("x")),
            (GridFilterOperator::DoesNotEqual, json!("x")),
            (GridFilterOperator::Contains, json!("x")),
            (GridFilterOperator::StartsWith, json!("x")),
            (GridFilterOperator::EndsWith, json!("x")),
            (GridFilterOperator::IsEmpty, json!(null)),
            (GridFilterOperator::IsNotEmpty, json!(null)),
            (GridFilterOperator::GreaterThan, json!(1.0)),
            (GridFilterOperator::GreaterThanOrEqual, json!(1.0)),
            (GridFilterOperator::LessThan, json!(1.0)),
            (GridFilterOperator::LessThanOrEqual, json!(1.0)),
        ];
        for (operator, value) in ops {
            let model = GridFilterModel {
                items: vec![GridFilterItem {
                    field: "msg".into(),
                    operator,
                    value,
                }],
                ..Default::default()
            };
            let clause = filter_where_clause(&model);
            assert!(
                clause.starts_with(" AND ("),
                "expected clause for {:?}",
                model.items[0].operator
            );
        }
    }

    #[test]
    fn order_honors_sort_field() {
        let filter = EventsQueryFilter {
            table: "t".into(),
            sort_field: Some("region".into()),
            sort_desc: true,
            ..Default::default()
        };
        let order = order_clause(&filter);
        assert!(order.contains("JSONExtractString(fields, 'region')"));
        assert!(order.contains("DESC"));
    }
}
