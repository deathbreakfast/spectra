//! Shared scenario executor for e2e (correctness) and bench (timings).

use std::time::Instant;

use spectra::helpers::{PlatformSmokeCounterRecorder, PlatformSmokeEventLogger};
use spectra::spectra_core::{
    current_emit_ts, install_config, try_record_counter_now, try_record_gauge_now,
    EventsQueryFilter, LabelMatcher, MetricsQueryRange, NameOverride, SpectraConfig, SpectraLevel,
};

use crate::bootstrap::{BootstrapSession, InstalledSpectra};
use crate::matrix::{MatrixSpec, TransportAdapter};
use crate::scenario::{ScenarioSpec, ScenarioStep};

/// Driver mode: assert on counts vs collect timings only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverKind {
    /// Run scenario steps and fail on assertion mismatch.
    Correctness,
    /// Record per-step timings without enforcing assertions.
    Benchmark,
}

/// Timing sample for one executed scenario step.
#[derive(Debug, Clone)]
pub struct StepTiming {
    /// Zero-based index in the scenario step list.
    pub step_index: usize,
    /// Short operation label (for example `emit_smoke_counter`).
    pub op: String,
    /// Wall time for the step in milliseconds.
    pub elapsed_ms: f64,
}

/// Outcome of running one scenario against one matrix row.
#[derive(Debug, Clone)]
pub struct ScenarioResult {
    /// Scenario identifier from [`ScenarioSpec::id`].
    pub scenario_id: String,
    /// Slug of the matrix row ([`MatrixSpec::slug`]).
    pub matrix_slug: String,
    /// Per-step timing samples (both driver modes).
    pub step_timings: Vec<StepTiming>,
    /// First assertion error in correctness mode, if any.
    pub error: Option<String>,
}

/// Executes declarative scenarios against an installed Spectra runtime.
pub struct ScenarioRunner;

impl ScenarioRunner {
    /// Install Spectra for `matrix`, then run `spec` under the process-wide matrix test lock.
    pub async fn run(
        matrix: MatrixSpec,
        spec: &ScenarioSpec,
        mode: DriverKind,
    ) -> anyhow::Result<ScenarioResult> {
        let _guard = crate::bootstrap::MATRIX_TEST_LOCK.lock().await;
        let session = BootstrapSession::new(matrix.clone());
        let installed = session.install_async().await?;
        Self::run_installed(&installed, spec, mode).await
    }

    /// Run `spec` against an already-installed runtime (no global install lock beyond caller).
    pub async fn run_installed(
        installed: &InstalledSpectra,
        spec: &ScenarioSpec,
        mode: DriverKind,
    ) -> anyhow::Result<ScenarioResult> {
        let mut step_timings = Vec::new();
        let mut error = None;

        for (step_index, step) in spec.steps.iter().enumerate() {
            let started = Instant::now();
            let step_result = execute_step(installed, step, mode).await;
            step_timings.push(StepTiming {
                step_index,
                op: step_label(step),
                elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
            });
            if let Err(msg) = step_result {
                if mode == DriverKind::Correctness {
                    error = Some(msg);
                    break;
                }
            }
        }

        Ok(ScenarioResult {
            scenario_id: spec.id.clone(),
            matrix_slug: installed.matrix.slug(),
            step_timings,
            error,
        })
    }
}

fn step_label(step: &ScenarioStep) -> String {
    match step {
        ScenarioStep::EmitSmokeCounter { .. } => "emit_smoke_counter".into(),
        ScenarioStep::EmitSmokeEvent { .. } => "emit_smoke_event".into(),
        ScenarioStep::EmitCounter { .. } => "emit_counter".into(),
        ScenarioStep::EmitGauge { .. } => "emit_gauge".into(),
        ScenarioStep::ConfigureGate { .. } => "configure_gate".into(),
        ScenarioStep::AssertMetricCount { .. } => "assert_metric_count".into(),
        ScenarioStep::AssertMetricCountWithLabels { .. } => "assert_metric_count_labels".into(),
        ScenarioStep::AssertEventCount { .. } => "assert_event_count".into(),
        ScenarioStep::AssertTransportCounterCount { .. } => "assert_transport_counter".into(),
        ScenarioStep::SleepMs { .. } => "sleep".into(),
        ScenarioStep::WaitUntilMetricCount { .. } => "wait_until_metric_count".into(),
        ScenarioStep::WaitUntilEventCount { .. } => "wait_until_event_count".into(),
    }
}

const VISIBILITY_POLL_INTERVAL_MS: u64 = 50;

async fn execute_step(
    installed: &InstalledSpectra,
    step: &ScenarioStep,
    mode: DriverKind,
) -> Result<(), String> {
    match step {
        ScenarioStep::EmitSmokeCounter { delta } => {
            PlatformSmokeCounterRecorder::record(*delta, serde_json::json!({}));
            Ok(())
        }
        ScenarioStep::EmitSmokeEvent { message } => {
            PlatformSmokeEventLogger::log(message.clone());
            Ok(())
        }
        ScenarioStep::EmitCounter {
            name,
            labels,
            delta,
        } => {
            let pairs: Vec<(&str, &str)> = labels
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();
            try_record_counter_now(name, &pairs, *delta);
            Ok(())
        }
        ScenarioStep::EmitGauge {
            name,
            labels,
            value,
        } => {
            let pairs: Vec<(&str, &str)> = labels
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();
            try_record_gauge_now(name, &pairs, *value);
            Ok(())
        }
        ScenarioStep::ConfigureGate {
            min_level,
            debug_metric_names,
        } => {
            let min_level = SpectraLevel::parse(min_level).unwrap_or(SpectraLevel::Info);
            let mut per_name = std::collections::HashMap::new();
            for name in debug_metric_names {
                per_name.insert(
                    name.clone(),
                    NameOverride {
                        level: Some(SpectraLevel::Debug),
                        ..Default::default()
                    },
                );
            }
            install_config(SpectraConfig {
                enabled: true,
                min_level,
                per_name,
                ..Default::default()
            });
            Ok(())
        }
        ScenarioStep::SleepMs { ms } => {
            tokio::time::sleep(std::time::Duration::from_millis(*ms)).await;
            Ok(())
        }
        ScenarioStep::AssertMetricCount { name, expected } => {
            let count = query_metric_count(installed, name, &[])
                .await
                .map_err(|e| e.to_string())?;
            if mode == DriverKind::Benchmark {
                return Ok(());
            }
            if count != *expected {
                return Err(format!(
                    "metric {name}: expected {expected} points, got {count}"
                ));
            }
            Ok(())
        }
        ScenarioStep::AssertMetricCountWithLabels {
            name,
            labels,
            expected,
        } => {
            let count = query_metric_count(installed, name, labels)
                .await
                .map_err(|e| e.to_string())?;
            if mode == DriverKind::Benchmark {
                return Ok(());
            }
            if count != *expected {
                return Err(format!(
                    "metric {name} with labels {:?}: expected {expected} points, got {count}",
                    labels
                ));
            }
            Ok(())
        }
        ScenarioStep::AssertEventCount { table, expected } => {
            let count = query_event_count(installed, table)
                .await
                .map_err(|e| e.to_string())?;
            if mode == DriverKind::Benchmark {
                return Ok(());
            }
            if count != *expected {
                return Err(format!(
                    "event {table}: expected {expected} rows, got {count}"
                ));
            }
            Ok(())
        }
        ScenarioStep::AssertTransportCounterCount { expected } => {
            if mode == DriverKind::Benchmark {
                return Ok(());
            }
            if installed.matrix.transport == TransportAdapter::Direct {
                if *expected == 0 {
                    return Ok(());
                }
                return Err("AssertTransportCounterCount requires recording transport".into());
            }
            let sink = installed
                .transport
                .as_ref()
                .ok_or_else(|| "recording transport sink missing".to_string())?;
            let got = sink.counters().len() as u32;
            if got != *expected {
                return Err(format!(
                    "transport counters: expected {expected}, got {got}"
                ));
            }
            Ok(())
        }
        ScenarioStep::WaitUntilMetricCount {
            name,
            labels,
            expected,
            timeout_ms,
        } => wait_until_metric_count(installed, name, labels, *expected, *timeout_ms, mode).await,
        ScenarioStep::WaitUntilEventCount {
            table,
            expected,
            timeout_ms,
        } => wait_until_event_count(installed, table, *expected, *timeout_ms, mode).await,
    }
}

async fn wait_until_metric_count(
    installed: &InstalledSpectra,
    name: &str,
    labels: &[(String, String)],
    expected: u32,
    timeout_ms: u64,
    mode: DriverKind,
) -> Result<(), String> {
    let deadline = Instant::now() + std::time::Duration::from_millis(timeout_ms);
    let mut last;
    loop {
        last = query_metric_count(installed, name, labels)
            .await
            .map_err(|e| e.to_string())?;
        if last == expected {
            return Ok(());
        }
        if Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(
            VISIBILITY_POLL_INTERVAL_MS,
        ))
        .await;
    }
    if mode == DriverKind::Benchmark {
        return Ok(());
    }
    Err(format!(
        "metric {name} labels {:?}: timed out after {timeout_ms}ms waiting for {expected} points (last={last})",
        labels
    ))
}

async fn wait_until_event_count(
    installed: &InstalledSpectra,
    table: &str,
    expected: u32,
    timeout_ms: u64,
    mode: DriverKind,
) -> Result<(), String> {
    let deadline = Instant::now() + std::time::Duration::from_millis(timeout_ms);
    let mut last;
    loop {
        last = query_event_count(installed, table)
            .await
            .map_err(|e| e.to_string())?;
        if last == expected {
            return Ok(());
        }
        if Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(
            VISIBILITY_POLL_INTERVAL_MS,
        ))
        .await;
    }
    if mode == DriverKind::Benchmark {
        return Ok(());
    }
    Err(format!(
        "event {table}: timed out after {timeout_ms}ms waiting for {expected} rows (last={last})"
    ))
}

async fn query_metric_count(
    installed: &InstalledSpectra,
    name: &str,
    labels: &[(String, String)],
) -> anyhow::Result<u32> {
    let now = current_emit_ts();
    let label_matchers = labels
        .iter()
        .map(|(k, v)| LabelMatcher {
            key: k.clone(),
            value: v.clone(),
        })
        .collect();
    let points = installed
        .spectra
        .router()
        .query_metrics(MetricsQueryRange {
            metric_name: name.to_string(),
            start: now - chrono::Duration::seconds(30),
            end: now + chrono::Duration::seconds(5),
            label_matchers,
        })
        .await?;
    Ok(points.len() as u32)
}

async fn query_event_count(installed: &InstalledSpectra, table: &str) -> anyhow::Result<u32> {
    let now = current_emit_ts();
    let rows = installed
        .spectra
        .router()
        .query_events(EventsQueryFilter {
            table: table.to_string(),
            start: Some(now - chrono::Duration::seconds(30)),
            end: Some(now + chrono::Duration::seconds(5)),
            ..Default::default()
        })
        .await?;
    Ok(rows.len() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn platform_smoke_roundtrip_mem_direct() {
        let result = ScenarioRunner::run(
            MatrixSpec::default(),
            &ScenarioSpec::platform_smoke_roundtrip(),
            DriverKind::Correctness,
        )
        .await
        .expect("run");
        assert!(result.error.is_none(), "{:?}", result.error);
    }
}
