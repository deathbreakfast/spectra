use spectra::{spectra_metric};

spectra_metric! {
    PlatformSmokeCounter {
        store: "default",
        name: "platform_smoke_counter",
        version: "0.1.0",
        description: "Platform smoke counter for Spectra extraction Phase 4",
    }
}
