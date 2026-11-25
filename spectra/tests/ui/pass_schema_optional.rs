use spectra::spectra_schema;

spectra_schema! {
    UiOptionalEvent {
        store: "default",
        table: "ui_optional_event",
        version: "0.1.0",
        fields: [
            table_bytes: {
                r#type: Option<i64>,
                classification: { pii: false, safe_for_console: true },
            },
        ],
    }
}

fn main() {
    UiOptionalEventLogger::log(None);
    UiOptionalEventLogger::log(Some(7));
}
