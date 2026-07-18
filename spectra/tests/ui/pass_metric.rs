use spectra::spectra_metric;

spectra_metric! {
    UiSmokeCounter {
        store: "default",
        name: "ui_smoke_counter",
        version: "0.1.0",
        description: "trybuild pass metric",
    }
}

fn main() {
    let _ = UiSmokeCounterRecorder;
}
