use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::query::EventAggregateResult;
use crate::registry::SchemaMetadata;
use crate::storage::EventsAggregateFilter;

/// Replacement shown when a field is classified as PII (or unsafe for console).
pub const PII_MASK: &str = "***";

/// Per-field GDPR-oriented metadata for Spectra event schemas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldClassification {
    /// Whether the field contains personally identifiable information.
    pub pii: bool,
    /// Whether the field may be logged to developer consoles.
    pub safe_for_console: bool,
    /// Optional retention period in days for stored values.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retention_days: Option<u32>,
    /// Optional human-readable purpose for collecting this field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
}

/// Mask a field value for display or console when classification requires it.
///
/// Returns [`PII_MASK`] when `classification.pii` is true, or when
/// `safe_for_console` is false. Otherwise returns a display string for `value`
/// (strings cloned; other JSON types via `to_string()`).
///
/// Hosts and UI layers should call this before rendering classified columns.
/// Query authorization (`spectra.query.*`) remains a host/Gauge concern.
///
/// # Examples
///
/// ```
/// use serde_json::json;
/// use spectra_core::{mask_field_value, FieldClassification, PII_MASK};
///
/// let pii = FieldClassification {
///     pii: true,
///     safe_for_console: false,
///     retention_days: None,
///     purpose: None,
/// };
/// assert_eq!(mask_field_value(&pii, &json!("alice@example.com")), PII_MASK);
///
/// let safe = FieldClassification {
///     pii: false,
///     safe_for_console: true,
///     retention_days: None,
///     purpose: None,
/// };
/// assert_eq!(mask_field_value(&safe, &json!("us-west")), "us-west");
/// ```
#[must_use]
pub fn mask_field_value(classification: &FieldClassification, value: &Value) -> String {
    if classification.pii || !classification.safe_for_console {
        return PII_MASK.to_string();
    }
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Replaces every PII field in an event row payload with [`PII_MASK`].
///
/// Only fields the schema classifies as `pii` and that are present in `fields` are touched;
/// other values keep their JSON type, and keys the schema does not list are left alone.
/// Unlike [`mask_field_value`], `safe_for_console` is ignored: it governs console output,
/// not grid display.
#[must_use]
pub fn mask_pii_fields(mut fields: Value, schema: &SchemaMetadata) -> Value {
    if let Value::Object(map) = &mut fields {
        for field in schema.fields.iter().filter(|f| f.classification.pii) {
            if let Some(slot) = map.get_mut(&field.name) {
                *slot = Value::String(PII_MASK.to_string());
            }
        }
    }
    fields
}

/// Aggregate labels that are placeholders, not field values, and keep their text.
const AGGREGATE_PLACEHOLDERS: [&str; 3] = ["?", crate::query_map::PIVOT_BLANK_LABEL, "(other)"];

/// Stand-in labels for one PII field's values within one query.
///
/// Built by [`stand_in_labels`]. Each distinct value maps to `"<Noun> <n>"`, numbered from 1.
#[derive(Debug, Clone, Default)]
pub struct StandInLabels {
    labels: HashMap<String, String>,
}

impl StandInLabels {
    /// The stand-in for `value`, or `None` when `value` was not passed to
    /// [`stand_in_labels`] or is an aggregate placeholder (`?`, `(blank)`, `(other)`).
    #[must_use]
    pub fn label(&self, value: &str) -> Option<&str> {
        self.labels.get(value).map(String::as_str)
    }

    fn relabel(&self, value: &mut String) {
        if let Some(label) = self.labels.get(value.as_str()) {
            value.clone_from(label);
        }
    }
}

/// Gives each distinct value of a PII field a stand-in label such as `Customer 1`.
///
/// The noun comes from `field` (`customer_id` becomes `Customer`, `ship_city` becomes
/// `Ship city`). Values are numbered in the order of a hash salted with `salt`, so:
///
/// - distinct values always get distinct labels;
/// - the same `values` and `salt` always give the same labels, so a chart can match an
///   aggregate result fetched with
///   [`crate::SpectraRouter::query_event_aggregate_with_salt`] and the same salt;
/// - the numbering says nothing about the raw values or their sort order, and a new salt
///   reshuffles it.
///
/// Spectra applies this to PII `group_by_field`, `row_fields` and `pivot_field` labels in
/// [`crate::SpectraRouter::query_event_aggregate`], with a fresh salt per query that is not
/// returned; pass your own salt to `query_event_aggregate_with_salt` when labels must match.
///
/// # Examples
///
/// ```
/// use spectra_core::stand_in_labels;
///
/// let labels = stand_in_labels("customer_id", ["alice", "bob", "alice"], 7);
/// let alice = labels.label("alice").expect("labelled");
/// let bob = labels.label("bob").expect("labelled");
/// assert!(alice.starts_with("Customer "));
/// assert_ne!(alice, bob);
/// assert_eq!(labels.label("carol"), None);
/// ```
#[must_use]
pub fn stand_in_labels<'a>(
    field: &str,
    values: impl IntoIterator<Item = &'a str>,
    salt: u64,
) -> StandInLabels {
    let mut distinct: Vec<&str> = values
        .into_iter()
        .filter(|v| !AGGREGATE_PLACEHOLDERS.contains(v))
        .collect();
    distinct.sort_unstable();
    distinct.dedup();
    distinct.sort_by_cached_key(|v| (salted_hash(salt, v), *v));
    let noun = stand_in_noun(field);
    let labels = distinct
        .into_iter()
        .enumerate()
        .map(|(i, v)| (v.to_string(), format!("{noun} {}", i + 1)))
        .collect();
    StandInLabels { labels }
}

fn salted_hash(salt: u64, value: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    salt.hash(&mut hasher);
    value.hash(&mut hasher);
    hasher.finish()
}

fn stand_in_noun(field: &str) -> String {
    let base = field.strip_suffix("_id").unwrap_or(field).replace('_', " ");
    let base = base.trim();
    let mut chars = base.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => "Value".to_string(),
    }
}

/// Replaces PII values in aggregate labels with [`stand_in_labels`].
///
/// Covers Slices labels when `filter`'s `group_by_field` is PII, each PII `row_fields`
/// column of a Pivot, and Pivot `column_keys` when `pivot_field` is PII. Fields the schema
/// does not classify as PII, time series and placeholder labels are left alone.
#[must_use]
pub fn mask_aggregate_labels(
    mut result: EventAggregateResult,
    filter: &EventsAggregateFilter,
    schema: &SchemaMetadata,
    salt: u64,
) -> EventAggregateResult {
    let is_pii = |name: &str| {
        schema
            .fields
            .iter()
            .any(|f| f.name == name && f.classification.pii)
    };
    match &mut result {
        EventAggregateResult::Slices { slices, .. } => {
            if let Some(field) = filter.group_by_field_name().filter(|f| is_pii(f)) {
                let labels = stand_in_labels(field, slices.iter().map(|s| s.label.as_str()), salt);
                for slice in slices.iter_mut() {
                    labels.relabel(&mut slice.label);
                }
            }
        }
        EventAggregateResult::Pivot {
            row_fields,
            column_keys,
            rows,
            ..
        } => {
            for (i, field) in row_fields.iter().enumerate() {
                if !is_pii(field) {
                    continue;
                }
                let labels = stand_in_labels(
                    field,
                    rows.iter()
                        .filter_map(|r| r.row_values.get(i).map(String::as_str)),
                    salt,
                );
                for row in rows.iter_mut() {
                    if let Some(value) = row.row_values.get_mut(i) {
                        labels.relabel(value);
                    }
                }
            }
            if let Some(field) = filter.pivot_field_name().filter(|f| is_pii(f)) {
                let labels = stand_in_labels(field, column_keys.iter().map(String::as_str), salt);
                for key in column_keys.iter_mut() {
                    labels.relabel(key);
                }
            }
        }
        EventAggregateResult::TimeSeries { .. } => {}
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn classification_roundtrip() {
        let c = FieldClassification {
            pii: true,
            safe_for_console: false,
            retention_days: Some(30),
            purpose: Some("debug".to_string()),
        };
        let json = serde_json::to_string(&c).expect("serialize");
        let back: FieldClassification = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, c);
    }

    #[test]
    fn mask_pii_hides_value() {
        let c = FieldClassification {
            pii: true,
            safe_for_console: true,
            retention_days: None,
            purpose: None,
        };
        assert_eq!(mask_field_value(&c, &json!("secret")), PII_MASK);
    }

    #[test]
    fn mask_unsafe_console_hides_value() {
        let c = FieldClassification {
            pii: false,
            safe_for_console: false,
            retention_days: None,
            purpose: None,
        };
        assert_eq!(mask_field_value(&c, &json!(42)), PII_MASK);
    }

    #[test]
    fn mask_safe_passthrough() {
        let c = FieldClassification {
            pii: false,
            safe_for_console: true,
            retention_days: None,
            purpose: None,
        };
        assert_eq!(mask_field_value(&c, &json!("ok")), "ok");
        assert_eq!(mask_field_value(&c, &json!(1)), "1");
    }

    fn schema() -> SchemaMetadata {
        use crate::registry::SchemaFieldMetadata;
        let field = |name: &str, pii: bool| SchemaFieldMetadata {
            name: name.to_string(),
            rust_type: "String".to_string(),
            classification: FieldClassification {
                pii,
                safe_for_console: false,
                retention_days: None,
                purpose: None,
            },
        };
        SchemaMetadata {
            fields: vec![
                field("email", true),
                field("count", false),
                field("age", true),
            ],
            ..SchemaMetadata::default()
        }
    }

    #[test]
    fn mask_pii_fields_masks_only_pii_and_keeps_types() {
        let out = mask_pii_fields(
            json!({"email": "a@b.c", "count": 7, "id": "r1", "other": true}),
            &schema(),
        );
        assert_eq!(
            out,
            json!({"email": PII_MASK, "count": 7, "id": "r1", "other": true})
        );
    }

    #[test]
    fn mask_pii_fields_does_not_invent_missing_keys() {
        let out = mask_pii_fields(json!({"count": 1}), &schema());
        assert_eq!(out, json!({"count": 1}));
    }

    #[test]
    fn mask_pii_fields_ignores_non_objects() {
        assert_eq!(mask_pii_fields(json!("x"), &schema()), json!("x"));
    }

    #[test]
    fn stand_in_labels_are_distinct_one_based_and_stable_per_salt() {
        let values = ["carol", "alice", "bob", "alice"];
        let a = stand_in_labels("customer_id", values, 42);
        let b = stand_in_labels("customer_id", values.iter().rev().copied(), 42);
        let mut got: Vec<&str> = ["alice", "bob", "carol"]
            .iter()
            .map(|v| a.label(v).expect("labelled"))
            .collect();
        for v in ["alice", "bob", "carol"] {
            assert_eq!(a.label(v), b.label(v), "input order must not matter");
        }
        got.sort_unstable();
        assert_eq!(got, vec!["Customer 1", "Customer 2", "Customer 3"]);
    }

    #[test]
    fn stand_in_labels_reshuffle_across_salts() {
        let values: Vec<String> = (0..20).map(|i| format!("user{i}")).collect();
        let refs = || values.iter().map(String::as_str);
        let a = stand_in_labels("user", refs(), 1);
        let b = stand_in_labels("user", refs(), 2);
        assert!(values.iter().any(|v| a.label(v) != b.label(v)));
    }

    #[test]
    fn stand_in_labels_skip_placeholders_and_name_the_noun() {
        let labels = stand_in_labels("ship_city", ["?", "(blank)", "(other)", "Oslo"], 0);
        assert_eq!(labels.label("Oslo"), Some("Ship city 1"));
        for p in ["?", "(blank)", "(other)"] {
            assert_eq!(labels.label(p), None);
        }
        assert_eq!(stand_in_noun("_id"), "Value");
    }

    fn agg_filter(
        group_by: Option<&str>,
        rows: &[&str],
        pivot: Option<&str>,
    ) -> EventsAggregateFilter {
        EventsAggregateFilter {
            table: "t".into(),
            start: chrono::Utc::now(),
            end: chrono::Utc::now(),
            partition: None,
            filter: crate::query::GridFilterModel::default(),
            measure: crate::query::EventMeasure::Count,
            measure_field: None,
            time_bucket_secs: None,
            group_by_field: group_by.map(str::to_string),
            row_fields: rows.iter().map(|s| (*s).to_string()).collect(),
            pivot_field: pivot.map(str::to_string),
            view: crate::query::EventExploreView::Table,
        }
    }

    #[test]
    fn mask_aggregate_labels_relabels_pii_slices_only() {
        use crate::query::SliceDto;
        let slices = |labels: &[&str]| EventAggregateResult::Slices {
            slices: labels
                .iter()
                .map(|l| SliceDto {
                    label: (*l).to_string(),
                    value: 1.0,
                })
                .collect(),
            headline: Vec::new(),
        };
        let out = mask_aggregate_labels(
            slices(&["a@b.c", "c@d.e", "?"]),
            &agg_filter(Some("email"), &[], None),
            &schema(),
            9,
        );
        let EventAggregateResult::Slices { slices: got, .. } = out else {
            panic!("slices");
        };
        let mut labels: Vec<&str> = got.iter().map(|s| s.label.as_str()).collect();
        labels.sort_unstable();
        assert_eq!(labels, vec!["?", "Email 1", "Email 2"]);

        let out = mask_aggregate_labels(
            slices(&["7"]),
            &agg_filter(Some("count"), &[], None),
            &schema(),
            9,
        );
        let EventAggregateResult::Slices { slices: got, .. } = out else {
            panic!("slices");
        };
        assert_eq!(got[0].label, "7");
    }

    #[test]
    fn mask_aggregate_labels_relabels_pii_pivot_rows_and_columns() {
        use crate::query::PivotRowDto;
        let result = EventAggregateResult::Pivot {
            row_fields: vec!["email".into(), "count".into()],
            column_keys: vec!["41".into(), "52".into(), "(other)".into()],
            rows: vec![
                PivotRowDto {
                    row_values: vec!["a@b.c".into(), "1".into()],
                    cells: vec![1.0, 0.0, 0.0],
                },
                PivotRowDto {
                    row_values: vec!["c@d.e".into(), "2".into()],
                    cells: vec![0.0, 1.0, 1.0],
                },
            ],
            headline: Vec::new(),
        };
        let out = mask_aggregate_labels(
            result,
            &agg_filter(None, &["email", "count"], Some("age")),
            &schema(),
            3,
        );
        let EventAggregateResult::Pivot {
            column_keys, rows, ..
        } = out
        else {
            panic!("pivot");
        };
        let mut emails: Vec<&str> = rows.iter().map(|r| r.row_values[0].as_str()).collect();
        emails.sort_unstable();
        assert_eq!(emails, vec!["Email 1", "Email 2"]);
        assert_eq!(rows[0].row_values[1], "1");
        assert_eq!(rows[1].row_values[1], "2");
        let mut keys: Vec<&str> = column_keys.iter().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, vec!["(other)", "Age 1", "Age 2"]);
    }
}
