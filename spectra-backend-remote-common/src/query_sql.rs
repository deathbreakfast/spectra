//! SQL fragments for remote event queries.
//!
//! Metric/table/field tokens are validated with [`spectra_core::validate_spectra_ident`]
//! before interpolation. String literals use [`escape_str`] (rejects NUL). Event paging uses
//! [`spectra_core::clamp_event_paging`].

use spectra_core::{
    clamp_event_paging, validate_spectra_ident, Error, EventsQueryFilter, GridFilterItem,
    GridFilterOperator, GridLogicOperator, Result,
};

/// Escape a string for use inside a single-quoted ClickHouse SQL literal.
///
/// Doubles `\` and `'`. Rejects NUL bytes so they cannot terminate or truncate literals.
///
/// # Errors
///
/// Returns [`Error::Config`] when `s` contains a NUL byte.
pub fn escape_str(s: &str) -> Result<String> {
    if s.contains('\0') {
        return Err(Error::config(
            "invalid SQL string literal: nul byte (operation=escape_str, reason=nul)",
        ));
    }
    Ok(s.replace('\\', "\\\\").replace('\'', "\\'"))
}

/// Build the table/time/partition scope clause.
///
/// # Errors
///
/// Returns [`Error::Config`] when `filter.table` is not a valid Spectra identifier.
pub fn scope_clause(filter: &EventsQueryFilter) -> Result<String> {
    validate_spectra_ident(&filter.table)?;
    let mut clauses = vec![format!("table_name = '{}'", escape_str(&filter.table)?)];
    if let Some(start) = filter.start {
        clauses.push(format!("ts >= '{}'", escape_str(&start.to_rfc3339())?));
    }
    if let Some(end) = filter.end {
        clauses.push(format!("ts <= '{}'", escape_str(&end.to_rfc3339())?));
    }
    if let Some(ref p) = filter.partition {
        clauses.push(format!(
            "JSONExtractString(fields, 'partition') = '{}'",
            escape_str(p)?
        ));
    }
    Ok(clauses.join(" AND "))
}

/// Build an additional `AND (…)` filter clause from the grid model.
///
/// # Errors
///
/// Returns [`Error::Config`] when any filter field name is not a valid identifier, or when
/// a filter value contains a NUL byte.
pub fn filter_where_clause(filter: &spectra_core::GridFilterModel) -> Result<String> {
    if filter.items.is_empty() && filter.quick_filter_values.is_empty() {
        return Ok(String::new());
    }
    let mut parts = Vec::new();
    for item in &filter.items {
        if let Some(clause) = filter_item_clause(item)? {
            parts.push(clause);
        }
    }
    if !filter.quick_filter_values.is_empty() {
        let mut q_parts = Vec::new();
        for v in &filter.quick_filter_values {
            q_parts.push(format!(
                "positionCaseInsensitive(fields, '{}') > 0",
                escape_str(v)?
            ));
        }
        if !q_parts.is_empty() {
            parts.push(format!("({})", q_parts.join(" OR ")));
        }
    }
    if parts.is_empty() {
        return Ok(String::new());
    }
    let op = match filter.logic_operator {
        GridLogicOperator::And => " AND ",
        GridLogicOperator::Or => " OR ",
    };
    Ok(format!(" AND ({})", parts.join(op)))
}

fn field_path(field: &str) -> Result<String> {
    if field == "ts" {
        return Ok("ts".to_string());
    }
    validate_spectra_ident(field)?;
    Ok(format!(
        "JSONExtractString(fields, '{}')",
        escape_str(field)?
    ))
}

fn filter_item_clause(item: &GridFilterItem) -> Result<Option<String>> {
    let path = field_path(&item.field)?;
    Ok(match item.operator {
        GridFilterOperator::Equals => {
            if let Some(v) = item.value.as_str() {
                Some(format!("{path} = '{}'", escape_str(v)?))
            } else {
                item.value
                    .as_f64()
                    .map(|v| format!("toFloat64OrZero({path}) = {v}"))
            }
        }
        GridFilterOperator::DoesNotEqual => {
            if let Some(v) = item.value.as_str() {
                Some(format!("{path} != '{}'", escape_str(v)?))
            } else {
                item.value
                    .as_f64()
                    .map(|v| format!("toFloat64OrZero({path}) != {v}"))
            }
        }
        GridFilterOperator::Contains => item
            .value
            .as_str()
            .map(|v| {
                escape_str(v).map(|esc| format!("positionCaseInsensitive({path}, '{esc}') > 0"))
            })
            .transpose()?,
        GridFilterOperator::StartsWith => item
            .value
            .as_str()
            .map(|v| escape_str(v).map(|esc| format!("startsWith(lower({path}), lower('{esc}'))")))
            .transpose()?,
        GridFilterOperator::EndsWith => item
            .value
            .as_str()
            .map(|v| escape_str(v).map(|esc| format!("endsWith(lower({path}), lower('{esc}'))")))
            .transpose()?,
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
    })
}

/// Build an `ORDER BY` clause.
///
/// # Errors
///
/// Returns [`Error::Config`] when `sort_field` is set and is not a valid identifier.
pub fn order_clause(filter: &EventsQueryFilter) -> Result<String> {
    let dir = if filter.sort_desc { "DESC" } else { "ASC" };
    let field = filter.sort_field.as_deref().unwrap_or("ts");
    if field == "ts" {
        return Ok(format!("ORDER BY ts {dir}"));
    }
    validate_spectra_ident(field)?;
    Ok(format!(
        "ORDER BY JSONExtractString(fields, '{}') {dir}, ts {dir}",
        escape_str(field)?
    ))
}

/// `LIMIT` / `OFFSET` clause after [`clamp_event_paging`].
///
/// Omits `OFFSET 0` for TensorBase SQL compatibility.
#[must_use]
pub fn limit_offset_clause(limit: u32, offset: u32) -> String {
    let (limit, offset) = clamp_event_paging(Some(limit), Some(offset));
    if offset == 0 {
        format!("LIMIT {limit}")
    } else {
        format!("LIMIT {limit} OFFSET {offset}")
    }
}

/// Clamp and format paging from an [`EventsQueryFilter`].
#[must_use]
pub fn paging_clause(filter: &EventsQueryFilter) -> String {
    let (limit, offset) = clamp_event_paging(filter.limit, filter.offset);
    limit_offset_clause(limit, offset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use spectra_core::{
        EventsQueryFilter, GridFilterItem, GridFilterModel, GridFilterOperator,
        MAX_EVENT_QUERY_LIMIT,
    };

    #[test]
    fn scope_includes_table() {
        let filter = EventsQueryFilter {
            table: "req_log".into(),
            ..Default::default()
        };
        let scope = scope_clause(&filter).expect("scope");
        assert!(scope.contains("table_name = 'req_log'"));
    }

    #[test]
    fn scope_rejects_bad_table() {
        let filter = EventsQueryFilter {
            table: "req; DROP".into(),
            ..Default::default()
        };
        assert!(scope_clause(&filter).is_err());
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
            let clause = filter_where_clause(&model).expect("filter");
            assert!(
                clause.starts_with(" AND ("),
                "expected clause for {:?}",
                model.items[0].operator
            );
        }
    }

    #[test]
    fn filter_rejects_bad_field() {
        let model = GridFilterModel {
            items: vec![GridFilterItem {
                field: "msg; DROP".into(),
                operator: GridFilterOperator::Equals,
                value: json!("x"),
            }],
            ..Default::default()
        };
        assert!(filter_where_clause(&model).is_err());
    }

    #[test]
    fn escape_quotes_and_backslashes() {
        assert_eq!(escape_str(r"a'b\c").expect("esc"), r"a\'b\\c");
    }

    #[test]
    fn escape_rejects_nul() {
        assert!(escape_str("a\0b").is_err());
    }

    #[test]
    fn order_honors_sort_field() {
        let filter = EventsQueryFilter {
            table: "t".into(),
            sort_field: Some("region".into()),
            sort_desc: true,
            ..Default::default()
        };
        let order = order_clause(&filter).expect("order");
        assert!(order.contains("JSONExtractString(fields, 'region')"));
        assert!(order.contains("DESC"));
    }

    #[test]
    fn order_rejects_bad_sort_field() {
        let filter = EventsQueryFilter {
            table: "t".into(),
            sort_field: Some("region DESC;--".into()),
            ..Default::default()
        };
        assert!(order_clause(&filter).is_err());
    }

    #[test]
    fn paging_clamps_huge_limit() {
        let clause = limit_offset_clause(u32::MAX, 0);
        assert_eq!(clause, format!("LIMIT {MAX_EVENT_QUERY_LIMIT}"));
    }
}
