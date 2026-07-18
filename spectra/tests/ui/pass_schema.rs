use spectra::spectra_schema;

spectra_schema! {
    UiSmokeEvent {
        store: "default",
        table: "ui_smoke_event",
        version: "0.1.0",
        description: "trybuild pass event",
        fields: [
            message: {
                r#type: String,
                classification: { pii: false, safe_for_console: true },
            },
        ],
    }
}

fn main() {
    let _ = UiSmokeEventLogger;
}
