//! Emit-volume gate: level threshold, statistical sampling, gauge coalesce.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use dashmap::DashMap;

use crate::config::{gate_state, policy_for, EmitPolicy};
use crate::registry::SpectraLevel;

static COALESCE: OnceLock<DashMap<u64, AtomicU64>> = OnceLock::new();

fn coalesce_map() -> &'static DashMap<u64, AtomicU64> {
    COALESCE.get_or_init(DashMap::new)
}

fn now_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn coalesce_key(name: &str, labels: &[(&str, &str)]) -> u64 {
    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    for (k, v) in labels {
        k.hash(&mut hasher);
        v.hash(&mut hasher);
    }
    hasher.finish()
}

thread_local! {
    static SAMPLE_RNG: std::cell::Cell<u64> = const { std::cell::Cell::new(0x853c49e6748fea9b) };
}

fn sample_passes(effective_rate: f64) -> bool {
    if effective_rate >= 1.0 {
        return true;
    }
    if effective_rate <= 0.0 {
        return false;
    }
    SAMPLE_RNG.with(|rng| {
        let mut x = rng.get();
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        rng.set(x);
        let draw = (x as f64) / (u64::MAX as f64);
        draw < effective_rate
    })
}

fn passes_level_gate(policy: EmitPolicy, min_level: SpectraLevel) -> bool {
    if policy.level.is_always_on() {
        return true;
    }
    policy.level <= min_level
}

fn passes_sample_gate(policy: EmitPolicy, global_sample_rate: f64) -> bool {
    if policy.level.is_always_on() {
        return true;
    }
    let effective = (policy.sample_rate * global_sample_rate).clamp(0.0, 1.0);
    sample_passes(effective)
}

fn passes_coalesce_gate(name: &str, labels: &[(&str, &str)], coalesce_ms: u64) -> bool {
    let key = coalesce_key(name, labels);
    let now = now_epoch_ms();
    let entry = coalesce_map()
        .entry(key)
        .or_insert_with(|| AtomicU64::new(0));
    let last = entry.load(Ordering::Relaxed);
    if last > 0 && now.saturating_sub(last) < coalesce_ms {
        record_gate_drop();
        return false;
    }
    entry.store(now, Ordering::Relaxed);
    true
}

fn record_gate_drop() {
    crate::rootcause::record_gate_drop();
}

fn should_drop(name: &str, labels: &[(&str, &str)], is_gauge: bool) -> bool {
    let Some(state) = gate_state() else {
        return false;
    };
    if !state.enabled {
        return false;
    }

    let policy = policy_for(name);

    if !passes_level_gate(policy, state.min_level) {
        record_gate_drop();
        return true;
    }

    if is_gauge {
        if let Some(ms) = policy.coalesce_ms {
            if !passes_coalesce_gate(name, labels, ms) {
                return true;
            }
            return false;
        }
    }

    if !passes_sample_gate(policy, state.global_sample_rate) {
        record_gate_drop();
        return true;
    }

    false
}

/// Returns true when the emit should be dropped (level / sample / coalesce gate).
pub fn drop_counter(name: &str, labels: &[(&str, &str)]) -> bool {
    should_drop(name, labels, false)
}

/// Returns true when the emit should be dropped (level / sample / coalesce gate).
pub fn drop_gauge(name: &str, labels: &[(&str, &str)], _value: f64) -> bool {
    should_drop(name, labels, true)
}

/// Returns true when the emit should be dropped (level / sample gate).
pub fn drop_event(table: &str) -> bool {
    should_drop(table, &[], false)
}

#[cfg(test)]
pub fn reset_coalesce_for_test() {
    coalesce_map().clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{install_config, reset_config_for_test, NameOverride, SpectraConfig};
    use crate::set_sink;
    use crate::sinks::{NoOpSink, RecordingSink};
    use std::collections::HashMap;
    use std::sync::Arc;

    async fn with_gate<F, Fut>(config: SpectraConfig, f: F)
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        let _g = crate::test_util::GLOBAL_TEST_LOCK.lock().await;
        reset_config_for_test();
        reset_coalesce_for_test();
        install_config(config);
        f().await;
        reset_config_for_test();
        reset_coalesce_for_test();
    }

    #[tokio::test]
    async fn fail_open_when_config_not_installed() {
        let _g = crate::test_util::GLOBAL_TEST_LOCK.lock().await;
        reset_config_for_test();
        assert!(!drop_counter("unknown", &[]));
        assert!(!drop_gauge("example_backlog", &[("topic", "t")], 1.0));
    }

    #[tokio::test]
    async fn gate_disabled_passes_all() {
        with_gate(
            SpectraConfig {
                enabled: false,
                min_level: SpectraLevel::Info,
                ..SpectraConfig::default()
            },
            || async {
                assert!(!drop_counter("example_db_reads", &[]));
            },
        )
        .await;
    }

    #[tokio::test]
    async fn level_gate_drops_debug_at_info() {
        with_gate(
            SpectraConfig {
                min_level: SpectraLevel::Info,
                per_name: HashMap::from([(
                    "example_db_reads".to_string(),
                    NameOverride {
                        level: Some(SpectraLevel::Debug),
                        ..Default::default()
                    },
                )]),
                ..SpectraConfig::default()
            },
            || async {
                assert!(drop_counter("example_db_reads", &[]));
                assert!(!drop_counter("example_db_writes", &[]));
            },
        )
        .await;
    }

    #[tokio::test]
    async fn coalesce_drops_within_window() {
        with_gate(
            SpectraConfig {
                per_name: HashMap::from([(
                    "example_backlog".to_string(),
                    NameOverride {
                        coalesce_ms: Some(Some(200)),
                        ..Default::default()
                    },
                )]),
                ..SpectraConfig::default()
            },
            || async {
                let labels = [("topic", "counter.events")];
                assert!(!drop_gauge("example_backlog", &labels, 1.0));
                assert!(drop_gauge("example_backlog", &labels, 2.0));
            },
        )
        .await;
    }

    #[tokio::test]
    async fn sample_rate_zero_drops_non_error() {
        with_gate(
            SpectraConfig {
                global_sample_rate: 0.0,
                ..SpectraConfig::default()
            },
            || async {
                assert!(drop_counter("unknown_metric", &[]));
            },
        )
        .await;
    }

    #[tokio::test]
    async fn errors_never_sampled_at_zero_global_rate() {
        with_gate(
            SpectraConfig {
                global_sample_rate: 0.0,
                per_name: HashMap::from([(
                    "example_db_errors".to_string(),
                    NameOverride {
                        level: Some(SpectraLevel::Error),
                        ..Default::default()
                    },
                )]),
                ..SpectraConfig::default()
            },
            || async {
                assert!(!drop_counter("example_db_errors", &[]));
            },
        )
        .await;
    }

    #[tokio::test]
    async fn default_info_drops_platform_debug_and_trace_metrics() {
        with_gate(
            SpectraConfig {
                min_level: SpectraLevel::Info,
                per_name: HashMap::from([
                    (
                        "example_backlog".to_string(),
                        NameOverride {
                            level: Some(SpectraLevel::Trace),
                            coalesce_ms: Some(Some(200)),
                            ..Default::default()
                        },
                    ),
                    (
                        "example_publishes".to_string(),
                        NameOverride {
                            level: Some(SpectraLevel::Debug),
                            ..Default::default()
                        },
                    ),
                    (
                        "example_db_reads".to_string(),
                        NameOverride {
                            level: Some(SpectraLevel::Debug),
                            ..Default::default()
                        },
                    ),
                    (
                        "example_db_writes".to_string(),
                        NameOverride {
                            level: Some(SpectraLevel::Info),
                            ..Default::default()
                        },
                    ),
                ]),
                ..SpectraConfig::default()
            },
            || async {
                assert!(drop_gauge("example_backlog", &[("topic", "t")], 1.0));
                assert!(drop_counter("example_publishes", &[("topic", "t"), ("mode", "local")]));
                assert!(drop_counter("example_db_reads", &[]));
                assert!(!drop_counter("example_db_writes", &[]));
            },
        )
        .await;
    }

    #[tokio::test]
    async fn facade_runs_with_gate_installed() {
        with_gate(SpectraConfig::default(), || async {
            let sink = RecordingSink::new();
            set_sink(Arc::new(sink.clone()));
            crate::try_record_counter("test_counter", &[], 1);
            assert_eq!(sink.counters().len(), 1);
            set_sink(Arc::new(NoOpSink));
        })
        .await;
    }
}
