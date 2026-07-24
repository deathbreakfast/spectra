//! Stable transport topic name helpers (v0.1).

/// Typed event topic: `spectra.event.{table}`.
pub fn event_topic(table: &str) -> String {
    format!("spectra.event.{table}")
}

/// Metric topic: `spectra.metric.{name}`.
pub fn metric_topic(name: &str) -> String {
    format!("spectra.metric.{name}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topic_naming() {
        assert_eq!(
            event_topic("request_debug_log"),
            "spectra.event.request_debug_log"
        );
        assert_eq!(metric_topic("cache_hits"), "spectra.metric.cache_hits");
    }
}
