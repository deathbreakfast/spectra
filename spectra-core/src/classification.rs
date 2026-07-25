use serde::{Deserialize, Serialize};
use serde_json::Value;

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
}
