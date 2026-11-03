use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::registry::SchemaMetadata;

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
}
