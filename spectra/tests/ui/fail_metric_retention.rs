use spectra::spectra_metric;

spectra_metric! {
    BadRetentionMetric {
        store: "default",
        name: "bad_retention_metric",
        version: "0.1.0",
        retention_days: 30,
    }
}

fn main() {}
