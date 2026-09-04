use spectra::spectra_schema;

spectra_schema! {
    BadEvent {
        store: "default",
        version: "0.1.0",
        fields: [
            message: {
                r#type: String,
                classification: { pii: false, safe_for_console: true },
            },
        ],
    }
}

fn main() {}
