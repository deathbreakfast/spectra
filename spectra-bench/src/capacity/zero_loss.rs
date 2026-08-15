//! BM-SW8 capacity wiring: Block-overflow persist, paced offered-rate sweep.

use anyhow::Result;
use serde_json::{json, Value};
use spectra::{PersistConfig, PersistOverflow};
use spectra_testkit::{install_bench_matrix_with_persist, InstalledSpectra, MatrixSpec, Topology};

use super::{
    infer_binding_tier, load_host_util, stamp_multidw_fields, to_rootcause, to_write_report,
    visibility_timeout_ms,
};
use crate::report::BenchReport;
use crate::sweep::SweepParams;
use crate::workload::zero_loss::{
    highest_passing_offered_rate, run_paced_zero_loss_counter, should_stop_sweep,
    ZERO_LOSS_BATCH_MAX, ZERO_LOSS_PATH,
};

pub(super) async fn run_zero_loss_experiment(
    matrix: &MatrixSpec,
    sweep: &SweepParams,
) -> Result<Vec<Value>> {
    let slug_suffix = format!("bench-{}", sweep.bench_client_index);
    let installed = install_zero_loss(matrix, &slug_suffix).await?;
    let rates = sweep.offered_rates();
    let mut reports = Vec::new();
    let mut passing: Vec<(u64, bool)> = Vec::new();

    for offered_rate in rates {
        let cell = run_zero_loss_cell(&installed, matrix, sweep, offered_rate).await?;
        let passed = cell.zero_loss.unwrap_or(false);
        passing.push((offered_rate, passed));
        let stop = should_stop_sweep(passed);
        reports.push(cell.to_json());
        if stop {
            break;
        }
    }

    if let Some(highest) = highest_passing_offered_rate(&passing) {
        if let Some(Value::Object(map)) = reports.last_mut() {
            map.insert("highest_passing_offered_rate".into(), json!(highest));
        }
    }

    Ok(reports)
}

async fn run_zero_loss_cell(
    installed: &InstalledSpectra,
    matrix: &MatrixSpec,
    sweep: &SweepParams,
    offered_rate: u64,
) -> Result<BenchReport> {
    let mut report = BenchReport::base(
        "bm-sw8",
        crate::experiments::resolve_experiment("bm-sw8")
            .map(|m| m.summary)
            .unwrap_or("paced zero-loss durable write"),
        matrix,
        sweep,
    );
    report.metric_kind = "write".into();
    stamp_multidw_fields(&mut report, matrix, sweep)?;
    report.path = Some(ZERO_LOSS_PATH.into());
    report.batch_max = Some(ZERO_LOSS_BATCH_MAX);
    report.writer_n = Some(sweep.writer_n);
    report.offered_rate = Some(offered_rate);

    let visibility_timeout_ms = visibility_timeout_ms(matrix.topology);
    let cell = run_paced_zero_loss_counter(
        installed,
        offered_rate,
        sweep.concurrency,
        sweep.duration,
        visibility_timeout_ms,
    )
    .await?;

    report.durable_counter_ops_per_sec = Some(cell.durable_rate);
    report.achieved_counter_ops_per_sec = Some(cell.durable_rate);
    report.visibility_confirmed = Some(cell.visibility_confirmed);
    report.zero_loss = Some(cell.zero_loss);
    report.visibility_p95_ms = cell.visibility_p95_ms;
    report.persist_queue_drops = Some(cell.persist_queue_drops);
    report.durable_rate_ratio = Some(cell.durable_rate_ratio);
    report.write = Some(to_write_report(&cell.write));
    report.rootcause = Some(to_rootcause(&cell.write.rootcause));
    report.host_util = load_host_util();
    report.binding_tier = Some(infer_binding_tier(&report.host_util));
    report.summary = format!(
        "{} (offered={} zero_loss={} adapter_errors={} n={} shard={} batch_max={} path={})",
        report.summary,
        cell.offered_rate,
        cell.zero_loss,
        cell.adapter_errors,
        report.n.unwrap_or(1),
        report.shard.unwrap_or(0),
        ZERO_LOSS_BATCH_MAX,
        ZERO_LOSS_PATH
    );
    if let Some(reason) = cell.fail_reason {
        report.summary = format!("{} fail={reason:?}", report.summary);
    }
    Ok(report)
}

async fn install_zero_loss(matrix: &MatrixSpec, slug_suffix: &str) -> Result<InstalledSpectra> {
    let queue_max = match matrix.topology {
        Topology::RemoteIngest => (ZERO_LOSS_BATCH_MAX.saturating_mul(256)).clamp(16_384, 131_072),
        Topology::Embedded => 1_048_576,
    };
    let persist = PersistConfig {
        queue_max,
        batch_max: ZERO_LOSS_BATCH_MAX,
        batch_enabled: true,
        overflow: PersistOverflow::Block,
        ..PersistConfig::default()
    };
    install_bench_matrix_with_persist(matrix.clone(), slug_suffix, Some(persist)).await
}
