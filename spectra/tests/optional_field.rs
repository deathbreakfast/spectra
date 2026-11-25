//! `Option<T>` field types in `spectra_schema!` emit JSON `null` for `None`.

use spectra::spectra_schema;
use spectra::SchemaRegistry;

spectra_schema! {
    OptionalSample {
        store: "default",
        table: "optional_sample",
        version: "0.1.0",
        fields: [
            table_bytes: {
                r#type: Option<i64>,
                classification: { pii: false, safe_for_console: true },
            },
        ],
    }
}

#[test]
fn none_serializes_as_null_and_some_as_value() {
    let none = OptionalSamplePayload {
        schema_table: "optional_sample",
        table_bytes: None,
        ts: None,
    };
    assert!(none.to_spectra_event().fields["table_bytes"].is_null());

    let some = OptionalSamplePayload {
        table_bytes: Some(42),
        ..none
    };
    assert_eq!(some.to_spectra_event().fields["table_bytes"], 42);
}

#[test]
fn inventory_records_option_type() {
    let schema = SchemaRegistry::global()
        .get_schema("optional_sample")
        .expect("registered");
    assert_eq!(schema.fields[0].rust_type, "Option<i64>");
}
