//! Declarative scenario steps shared by e2e (assert) and bench (measure).

use serde::{Deserialize, Serialize};

/// Default poll timeout for persist visibility (covers embedded + remote ingest).
pub const DEFAULT_VISIBILITY_TIMEOUT_MS: u64 = 15_000;

/// Declarative scenario: identifier plus ordered steps.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScenarioSpec {
    /// Stable scenario name for logs and bench output.
    pub id: String,
    /// Ordered steps executed by [`ScenarioRunner`](crate::ScenarioRunner).
    pub steps: Vec<ScenarioStep>,
}

impl ScenarioSpec {
    /// Platform smoke counter + event persist roundtrip (upstream CI default).
    pub fn platform_smoke_roundtrip() -> Self {
        Self {
            id: "platform-smoke-roundtrip".to_string(),
            steps: vec![
                ScenarioStep::EmitSmokeCounter { delta: 1 },
                ScenarioStep::WaitUntilMetricCount {
                    name: "platform_smoke_counter".to_string(),
                    labels: vec![],
                    expected: 1,
                    timeout_ms: DEFAULT_VISIBILITY_TIMEOUT_MS,
                },
                ScenarioStep::EmitSmokeEvent {
                    message: "phase5 smoke".to_string(),
                },
                ScenarioStep::WaitUntilEventCount {
                    table: "platform_smoke_event".to_string(),
                    expected: 1,
                    timeout_ms: DEFAULT_VISIBILITY_TIMEOUT_MS,
                },
            ],
        }
    }

    /// Transport sink and storage both receive the same counter emit.
    pub fn transport_dual_path() -> Self {
        Self {
            id: "transport-dual-path".to_string(),
            steps: vec![
                ScenarioStep::EmitCounter {
                    name: "cache_hits".to_string(),
                    labels: vec![("region".to_string(), "us-west".to_string())],
                    delta: 1,
                },
                ScenarioStep::SleepMs { ms: 80 },
                ScenarioStep::AssertTransportCounterCount { expected: 1 },
                ScenarioStep::WaitUntilMetricCount {
                    name: "cache_hits".to_string(),
                    labels: vec![],
                    expected: 1,
                    timeout_ms: DEFAULT_VISIBILITY_TIMEOUT_MS,
                },
            ],
        }
    }

    /// BM-S0: minimal emit path for latency sampling.
    pub fn emit_only_bench() -> Self {
        Self {
            id: "emit-only-bench".to_string(),
            steps: vec![ScenarioStep::EmitCounter {
                name: "bench_emit_counter".to_string(),
                labels: vec![],
                delta: 1,
            }],
        }
    }

    /// BM-S1/S2: smoke persist roundtrip.
    pub fn persist_roundtrip_bench() -> Self {
        Self {
            id: "persist-roundtrip-bench".to_string(),
            steps: vec![
                ScenarioStep::EmitSmokeCounter { delta: 1 },
                ScenarioStep::WaitUntilMetricCount {
                    name: "platform_smoke_counter".to_string(),
                    labels: vec![],
                    expected: 1,
                    timeout_ms: DEFAULT_VISIBILITY_TIMEOUT_MS,
                },
            ],
        }
    }

    /// BM-S3: emit batch then query range.
    pub fn query_range_bench(count: u32) -> Self {
        Self {
            id: "query-range-bench".to_string(),
            steps: (0..count)
                .flat_map(|i| {
                    [
                        ScenarioStep::EmitCounter {
                            name: "bench_query_counter".to_string(),
                            labels: vec![("idx".to_string(), i.to_string())],
                            delta: 1,
                        },
                        ScenarioStep::SleepMs { ms: 5 },
                    ]
                })
                .chain([ScenarioStep::WaitUntilMetricCount {
                    name: "bench_query_counter".to_string(),
                    labels: vec![],
                    expected: count,
                    timeout_ms: DEFAULT_VISIBILITY_TIMEOUT_MS,
                }])
                .collect(),
        }
    }

    /// Gate drops debug-tier counter before persist (sad path).
    pub fn gate_drops_debug() -> Self {
        Self {
            id: "gate-drops-debug".to_string(),
            steps: vec![
                ScenarioStep::ConfigureGate {
                    min_level: "info".to_string(),
                    debug_metric_names: vec!["gate_debug_probe".to_string()],
                },
                ScenarioStep::EmitCounter {
                    name: "gate_debug_probe".to_string(),
                    labels: vec![],
                    delta: 1,
                },
                // Expected 0: wait for flush window, then assert absence.
                ScenarioStep::SleepMs { ms: 80 },
                ScenarioStep::AssertMetricCount {
                    name: "gate_debug_probe".to_string(),
                    expected: 0,
                },
            ],
        }
    }

    /// Transport receives emit but storage stays empty when persist is disabled (sad path).
    pub fn transport_only_no_storage() -> Self {
        Self {
            id: "transport-only-no-storage".to_string(),
            steps: vec![
                ScenarioStep::EmitCounter {
                    name: "transport_only_counter".to_string(),
                    labels: vec![],
                    delta: 1,
                },
                ScenarioStep::SleepMs { ms: 80 },
                ScenarioStep::AssertTransportCounterCount { expected: 1 },
                ScenarioStep::AssertMetricCount {
                    name: "transport_only_counter".to_string(),
                    expected: 0,
                },
            ],
        }
    }

    /// Labeled counter query returns expected count (happy path).
    pub fn label_filter_hit() -> Self {
        Self {
            id: "label-filter-hit".to_string(),
            steps: vec![
                ScenarioStep::EmitCounter {
                    name: "label_filter_counter".to_string(),
                    labels: vec![("region".to_string(), "us-west".to_string())],
                    delta: 1,
                },
                ScenarioStep::WaitUntilMetricCount {
                    name: "label_filter_counter".to_string(),
                    labels: vec![("region".to_string(), "us-west".to_string())],
                    expected: 1,
                    timeout_ms: DEFAULT_VISIBILITY_TIMEOUT_MS,
                },
            ],
        }
    }

    /// Wrong label filter returns zero points (sad path).
    pub fn label_filter_miss() -> Self {
        Self {
            id: "label-filter-miss".to_string(),
            steps: vec![
                ScenarioStep::EmitCounter {
                    name: "label_filter_miss_counter".to_string(),
                    labels: vec![("region".to_string(), "us-west".to_string())],
                    delta: 1,
                },
                ScenarioStep::SleepMs { ms: 80 },
                ScenarioStep::AssertMetricCountWithLabels {
                    name: "label_filter_miss_counter".to_string(),
                    labels: vec![("region".to_string(), "eu-central".to_string())],
                    expected: 0,
                },
            ],
        }
    }

    /// Gauge persist + query roundtrip (happy path).
    pub fn gauge_roundtrip() -> Self {
        Self {
            id: "gauge-roundtrip".to_string(),
            steps: vec![
                ScenarioStep::EmitGauge {
                    name: "queue_depth".to_string(),
                    labels: vec![("shard".to_string(), "0".to_string())],
                    value: 12.5,
                },
                ScenarioStep::WaitUntilMetricCount {
                    name: "queue_depth".to_string(),
                    labels: vec![],
                    expected: 1,
                    timeout_ms: DEFAULT_VISIBILITY_TIMEOUT_MS,
                },
            ],
        }
    }

    /// Query for a metric that was never emitted returns zero rows (sad path).
    pub fn query_time_range_empty() -> Self {
        Self {
            id: "query-time-range-empty".to_string(),
            steps: vec![ScenarioStep::AssertMetricCount {
                name: "never_emitted_metric".to_string(),
                expected: 0,
            }],
        }
    }

    /// Console NDJSON telemetry row emits without error (happy path).
    pub fn telemetry_console_ndjson() -> Self {
        Self {
            id: "telemetry-console-ndjson".to_string(),
            steps: vec![
                ScenarioStep::EmitSmokeCounter { delta: 1 },
                ScenarioStep::WaitUntilMetricCount {
                    name: "platform_smoke_counter".to_string(),
                    labels: vec![],
                    expected: 1,
                    timeout_ms: DEFAULT_VISIBILITY_TIMEOUT_MS,
                },
            ],
        }
    }
}

/// One declarative step in a [`ScenarioSpec`] (emit, assert, or sleep).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "step", rename_all = "snake_case")]
pub enum ScenarioStep {
    /// Emit the platform smoke counter schema by delta.
    EmitSmokeCounter {
        /// Counter delta passed to the smoke recorder.
        delta: i64,
    },
    /// Emit the platform smoke event schema with a message field.
    EmitSmokeEvent {
        /// Message payload for the smoke logger.
        message: String,
    },
    /// Emit a raw counter by name and label pairs.
    EmitCounter {
        /// Metric name.
        name: String,
        /// Label key/value pairs.
        labels: Vec<(String, String)>,
        /// Counter delta.
        delta: i64,
    },
    /// Emit a raw gauge by name and label pairs.
    EmitGauge {
        /// Metric name.
        name: String,
        /// Label key/value pairs.
        labels: Vec<(String, String)>,
        /// Gauge sample value.
        value: f64,
    },
    /// Install emit gate policy before subsequent emits.
    ConfigureGate {
        /// Global minimum level (`info`, `debug`, etc.).
        min_level: String,
        /// Metric names to treat as debug-tier via per-name overrides.
        debug_metric_names: Vec<String>,
    },
    /// Assert persisted metric point count after async flush.
    AssertMetricCount {
        /// Metric name to query.
        name: String,
        /// Expected number of points in the query window.
        expected: u32,
    },
    /// Assert persisted metric point count with label equality filters.
    AssertMetricCountWithLabels {
        /// Metric name to query.
        name: String,
        /// Label key/value pairs that must all match.
        labels: Vec<(String, String)>,
        /// Expected number of points in the query window.
        expected: u32,
    },
    /// Assert persisted event row count after async flush.
    AssertEventCount {
        /// Event table name to query.
        table: String,
        /// Expected number of rows in the query window.
        expected: u32,
    },
    /// Assert the recording transport sink saw N counter emits.
    AssertTransportCounterCount {
        /// Expected transport-side counter emit count.
        expected: u32,
    },
    /// Sleep to allow async persist or transport sinks to drain.
    SleepMs {
        /// Duration in milliseconds.
        ms: u64,
    },
    /// Poll until metric point count matches (or timeout).
    WaitUntilMetricCount {
        /// Metric name to query.
        name: String,
        /// Optional label key/value pairs that must all match.
        labels: Vec<(String, String)>,
        /// Expected number of points in the query window.
        expected: u32,
        /// Maximum wait in milliseconds before failing.
        timeout_ms: u64,
    },
    /// Poll until event row count matches (or timeout).
    WaitUntilEventCount {
        /// Event table name to query.
        table: String,
        /// Expected number of rows in the query window.
        expected: u32,
        /// Maximum wait in milliseconds before failing.
        timeout_ms: u64,
    },
}
