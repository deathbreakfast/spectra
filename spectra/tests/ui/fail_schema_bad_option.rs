use spectra::spectra_schema;

spectra_schema! {
    BadOption {
        store: "default",
        table: "bad_option",
        version: "0.1.0",
        fields: [
            size: {
                r#type: Option<Foo>,
                classification: { pii: false, safe_for_console: true },
            },
        ],
    }
}

fn main() {}
