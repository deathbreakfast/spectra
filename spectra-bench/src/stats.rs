use spectra_testkit::StepTiming;

pub fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

pub fn summarize_timings(timings: &[StepTiming]) -> serde_json::Value {
    let mut samples: Vec<f64> = timings.iter().map(|t| t.elapsed_ms).collect();
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    serde_json::json!({
        "count": samples.len(),
        "p50_ms": percentile(&samples, 0.50),
        "p95_ms": percentile(&samples, 0.95),
        "max_ms": samples.last().copied().unwrap_or(0.0),
    })
}
