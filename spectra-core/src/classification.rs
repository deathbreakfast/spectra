use serde::{Deserialize, Serialize};

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
