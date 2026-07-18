use spectra::{spectra_schema};

spectra_schema! {
    PlatformSmokeEvent {
        store: "default",
        table: "platform_smoke_event",
        version: "0.1.0",
        description: "Platform smoke event for Spectra extraction Phase 4",
        fields: [
            message: {
                r#type: String,
                classification: { pii: false, safe_for_console: true },
            },
        ],
    }
}
