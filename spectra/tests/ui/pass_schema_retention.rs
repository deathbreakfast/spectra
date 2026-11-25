use spectra::{spectra_metric, spectra_schema, SchemaRegistry};

spectra_schema! {
    UiRetentionEvent {
        store: "default",
        table: "ui_retention_event",
        version: "0.1.0",
        retention_days: 90,
        fields: [
            email: {
                r#type: String,
                classification: { pii: true, safe_for_console: false, retention_days: 30 },
            },
            region: {
                r#type: String,
                classification: { pii: false, safe_for_console: true },
            },
        ],
    }
}

spectra_metric! {
    UiLabelledCounter {
        store: "default",
        name: "ui_labelled_counter",
        version: "0.1.0",
        labels: [
            user_id: { classification: { pii: true, safe_for_console: false } },
            route: { classification: { pii: false } },
        ],
    }
}

fn main() {
    let registry = SchemaRegistry::global();
    let event = registry
        .get_schema("ui_retention_event")
        .expect("event registered");
    assert_eq!(event.retention_days, Some(90));
    assert_eq!(event.fields[0].classification.retention_days, Some(30));
    assert_eq!(event.fields[1].classification.retention_days, None);
    assert_eq!(event.effective_retention_days(), Some(30));

    let metric = registry
        .get_schema("ui_labelled_counter")
        .expect("metric registered");
    let labels: Vec<(&str, bool)> = metric
        .fields
        .iter()
        .map(|f| (f.name.as_str(), f.classification.pii))
        .collect();
    assert_eq!(labels, vec![("user_id", true), ("route", false)]);
    assert_eq!(metric.retention_days, None);
}
