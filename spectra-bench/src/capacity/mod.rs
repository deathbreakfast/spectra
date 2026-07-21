use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde_json::Value;
use spectra::PersistConfig;
use spectra_testkit::{
    assert_embedded_topology, dw_n, dw_url_fingerprint, install_bench_matrix,
    install_bench_matrix_with_persist, remote_env_ready, remote_url_for, shard_index,
    InstalledSpectra, MatrixSpec, Topology,
};

use crate::cli::CliMatrix;
use crate::experiments::{ExperimentMeta, ExperimentTrack};
use crate::report::{BenchReport, HostUtilReport, RootcauseReport, WriteReport};
use crate::sweep::SweepParams;
use crate::workload::{
    count_event_rows, count_metric_points, prefill_events, prefill_metrics,
    run_adapter_counter_firehose, run_batched_durable_counter_firehose,
    run_durable_counter_firehose, run_durable_event_firehose, run_event_firehose,
    run_event_queries, run_full_stack_counter_firehose, run_metric_queries,
    wait_until_event_visible, wait_until_metric_visible, BATCHED_DURABLE_COUNTER_NAME,
    DURABLE_COUNTER_NAME, DURABLE_EVENT_TABLE,
};

pub struct CapacityRunArgs {
    pub meta: &'static ExperimentMeta,
    pub matrix: CliMatrix,
    pub sweep: SweepParams,
    pub report: Option<std::path::PathBuf>,
}

pub async fn run_capacity(args: CapacityRunArgs) -> Result<()> {
    let matrix = args.matrix.to_matrix_spec();
    validate_capacity_matrix(&matrix)?;

    let reports = match args.meta.track {
        ExperimentTrack::Write => run_write_experiment(args.meta.id, &matrix, &args.sweep).await?,
        ExperimentTrack::Query => run_query_experiment(args.meta.id, &matrix, &args.sweep).await?,
        ExperimentTrack::Scenario => bail!("scenario track uses run_scenario"),
    };

    let json = if reports.len() == 1 {
        reports[0].clone()
    } else {
        Value::Array(reports)
    };
    let pretty = serde_json::to_string_pretty(&json)?;
    println!("{pretty}");
    if let Some(path) = args.report {
        write_report(&path, &pretty)?;
    }
    Ok(())
}

async fn run_write_experiment(
    id: &str,
    matrix: &MatrixSpec,
    sweep: &SweepParams,
) -> Result<Vec<Value>> {
    let slug_suffix = format!("bench-{}", sweep.bench_client_index);
    let installed = if id == "bm-sw7" {
        install_batched(matrix, &slug_suffix, sweep).await?
    } else {
        install(matrix, &slug_suffix).await?
    };
    let mut report = BenchReport::base(id, experiment_summary(id), matrix, sweep);
    report.metric_kind = "write".into();
    let drain_ms = persist_drain_ms(matrix.topology);

    let result = match id {
        "bm-sw0" => {
            let fh = run_adapter_counter_firehose(
                installed.metrics_backend(),
                sweep.concurrency,
                sweep.duration,
            )
            .await?;
            report.achieved_adapter_ops_per_sec = Some(fh.achieved_ops_per_sec);
            report.write = Some(to_write_report(&fh));
            report.rootcause = Some(to_rootcause(&fh.rootcause));
            report.to_json()
        }
        "bm-sw1" | "bm-sw2" | "bm-sw3" => {
            let fh = run_full_stack_counter_firehose(sweep.concurrency, sweep.duration, drain_ms)
                .await?;
            report.achieved_counter_ops_per_sec = Some(fh.achieved_ops_per_sec);
            report.write = Some(to_write_report(&fh));
            report.rootcause = Some(to_rootcause(&fh.rootcause));
            if id == "bm-sw3" && sweep.bench_clients > 1 {
                report.summary = format!(
                    "{} (client {}/{})",
                    report.summary,
                    sweep.bench_client_index + 1,
                    sweep.bench_clients
                );
            }
            report.to_json()
        }
        "bm-sw4" => {
            let fh = run_event_firehose(sweep.concurrency, sweep.duration, false, None, drain_ms)
                .await?;
            report.achieved_event_ops_per_sec = Some(fh.achieved_ops_per_sec);
            report.write = Some(to_write_report(&fh));
            report.rootcause = Some(to_rootcause(&fh.rootcause));
            report.to_json()
        }
        "bm-sw5" => run_durable_counter_experiment(&installed, matrix, sweep, &mut report).await?,
        "bm-sw6" => run_durable_event_experiment(&installed, matrix, sweep, &mut report).await?,
        "bm-sw7" => {
            run_batched_durable_counter_experiment(&installed, matrix, sweep, &mut report).await?
        }
        _ => bail!("unmapped write experiment {id}"),
    };

    Ok(vec![result])
}

async fn run_durable_counter_experiment(
    installed: &InstalledSpectra,
    matrix: &MatrixSpec,
    sweep: &SweepParams,
    report: &mut BenchReport,
) -> Result<Value> {
    stamp_multidw_fields(report, matrix, sweep)?;
    let visibility_timeout_ms = visibility_timeout_ms(matrix.topology);

    let fh = run_durable_counter_firehose(
        installed.metrics_backend(),
        sweep.concurrency,
        sweep.duration,
    )
    .await?;

    let visible = confirm_metric_visibility(installed, visibility_timeout_ms).await?;
    report.durable_counter_ops_per_sec = Some(fh.achieved_ops_per_sec);
    report.achieved_adapter_ops_per_sec = Some(fh.achieved_ops_per_sec);
    report.visibility_confirmed = Some(visible);
    report.write = Some(to_write_report(&fh));
    report.rootcause = Some(to_rootcause(&fh.rootcause));
    report.host_util = load_host_util();
    report.binding_tier = Some(infer_binding_tier(&report.host_util));
    report.summary = format!(
        "{} (n={} shard={} path=subscriber-sim)",
        report.summary,
        report.n.unwrap_or(1),
        report.shard.unwrap_or(0)
    );
    Ok(report.to_json())
}

async fn run_durable_event_experiment(
    installed: &InstalledSpectra,
    matrix: &MatrixSpec,
    sweep: &SweepParams,
    report: &mut BenchReport,
) -> Result<Value> {
    stamp_multidw_fields(report, matrix, sweep)?;
    let visibility_timeout_ms = visibility_timeout_ms(matrix.topology);

    let fh = run_durable_event_firehose(
        installed.events_backend(),
        sweep.concurrency,
        sweep.duration,
    )
    .await?;

    let visible = confirm_event_visibility(installed, visibility_timeout_ms).await?;
    report.durable_event_ops_per_sec = Some(fh.achieved_ops_per_sec);
    report.achieved_event_ops_per_sec = Some(fh.achieved_ops_per_sec);
    report.visibility_confirmed = Some(visible);
    report.write = Some(to_write_report(&fh));
    report.rootcause = Some(to_rootcause(&fh.rootcause));
    report.host_util = load_host_util();
    report.binding_tier = Some(infer_binding_tier(&report.host_util));
    report.summary = format!(
        "{} (n={} shard={} path=subscriber-sim)",
        report.summary,
        report.n.unwrap_or(1),
        report.shard.unwrap_or(0)
    );
    Ok(report.to_json())
}

async fn run_batched_durable_counter_experiment(
    installed: &InstalledSpectra,
    matrix: &MatrixSpec,
    sweep: &SweepParams,
    report: &mut BenchReport,
) -> Result<Value> {
    stamp_multidw_fields(report, matrix, sweep)?;
    report.path = Some("l2-batch".into());
    report.batch_max = Some(sweep.batch_max);
    report.writer_n = Some(sweep.writer_n);
    let visibility_timeout_ms = visibility_timeout_ms(matrix.topology);

    let fh =
        run_batched_durable_counter_firehose(&installed.spectra, sweep.concurrency, sweep.duration)
            .await?;

    let visible = confirm_batched_metric_visibility(installed, visibility_timeout_ms).await?;
    report.durable_counter_ops_per_sec = Some(fh.achieved_ops_per_sec);
    report.achieved_counter_ops_per_sec = Some(fh.achieved_ops_per_sec);
    report.visibility_confirmed = Some(visible);
    report.write = Some(to_write_report(&fh));
    report.rootcause = Some(to_rootcause(&fh.rootcause));
    report.host_util = load_host_util();
    report.binding_tier = Some(infer_binding_tier(&report.host_util));
    report.summary = format!(
        "{} (n={} shard={} writers={} batch_max={} path=l2-batch)",
        report.summary,
        report.n.unwrap_or(1),
        report.shard.unwrap_or(0),
        sweep.writer_n,
        sweep.batch_max
    );
    Ok(report.to_json())
}

fn stamp_multidw_fields(
    report: &mut BenchReport,
    matrix: &MatrixSpec,
    sweep: &SweepParams,
) -> Result<()> {
    let n = sweep.dw_n.max(dw_n());
    let shard = shard_index();
    report.n = Some(n);
    report.shard = Some(shard);
    report.path = Some("subscriber-sim".into());
    if matches!(
        matrix.storage,
        spectra_testkit::StorageAdapter::ClickHouse | spectra_testkit::StorageAdapter::TensorBase
    ) {
        let url = remote_url_for(matrix.storage)?;
        report.dw_url_fingerprint = Some(dw_url_fingerprint(&url));
    }
    Ok(())
}

async fn confirm_metric_visibility(installed: &InstalledSpectra, timeout_ms: u64) -> Result<bool> {
    match wait_until_metric_visible(installed, DURABLE_COUNTER_NAME, &[], 1, timeout_ms).await {
        Ok(()) => {
            let _ = count_metric_points(installed, DURABLE_COUNTER_NAME, &[]).await?;
            Ok(true)
        }
        Err(_) => Ok(false),
    }
}

async fn confirm_event_visibility(installed: &InstalledSpectra, timeout_ms: u64) -> Result<bool> {
    match wait_until_event_visible(installed, DURABLE_EVENT_TABLE, 1, timeout_ms).await {
        Ok(()) => {
            let _ = count_event_rows(installed, DURABLE_EVENT_TABLE, 1).await?;
            Ok(true)
        }
        Err(_) => Ok(false),
    }
}

async fn confirm_batched_metric_visibility(
    installed: &InstalledSpectra,
    timeout_ms: u64,
) -> Result<bool> {
    match wait_until_metric_visible(installed, BATCHED_DURABLE_COUNTER_NAME, &[], 1, timeout_ms)
        .await
    {
        Ok(()) => {
            let _ = count_metric_points(installed, BATCHED_DURABLE_COUNTER_NAME, &[]).await?;
            Ok(true)
        }
        Err(_) => Ok(false),
    }
}

fn load_host_util() -> Option<Vec<HostUtilReport>> {
    let path = std::env::var("SPECTRA_BENCH_HOST_UTIL_JSON").ok()?;
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

fn infer_binding_tier(host_util: &Option<Vec<HostUtilReport>>) -> String {
    if let Ok(tier) = std::env::var("SPECTRA_BENCH_BINDING_TIER") {
        if !tier.is_empty() {
            return tier;
        }
    }
    let Some(utils) = host_util else {
        return "unset".into();
    };
    let writer_peak = utils
        .iter()
        .filter(|u| u.role.contains("writer"))
        .filter_map(|u| u.cpu_peak_pct)
        .fold(0.0_f64, f64::max);
    let dw_peak = utils
        .iter()
        .filter(|u| {
            u.role.contains("dw") || u.role.contains("clickhouse") || u.role.contains("tensorbase")
        })
        .filter_map(|u| u.cpu_peak_pct)
        .fold(0.0_f64, f64::max);
    if writer_peak >= 85.0 && dw_peak < 70.0 {
        "client-cpu".into()
    } else if dw_peak >= 70.0 {
        "dw".into()
    } else {
        "unset".into()
    }
}

async fn run_query_experiment(
    id: &str,
    matrix: &MatrixSpec,
    sweep: &SweepParams,
) -> Result<Vec<Value>> {
    let depths = match id {
        "bm-sq0" => vec![0],
        "bm-sq2" => vec![sweep.prefill.unwrap_or(10_000)],
        _ => sweep.prefill_depths(),
    };

    let mut out = Vec::new();
    for depth in depths {
        let slug_suffix = format!("query-{depth}");
        let installed = install(matrix, &slug_suffix).await?;
        let mut report = BenchReport::base(id, experiment_summary(id), matrix, sweep);
        report.metric_kind = "query".into();
        report.sweep.prefill = Some(depth);
        let visibility_timeout_ms = visibility_timeout_ms(matrix.topology);

        if depth > 0 {
            let prefill = if id == "bm-sq3" {
                prefill_events(installed.events_backend(), depth).await?
            } else {
                prefill_metrics(installed.metrics_backend(), depth).await?
            };
            report.prefill_count = Some(prefill.count);
            report.prefill_elapsed_ms = Some(prefill.elapsed_ms);
        } else {
            report.prefill_count = Some(0);
        }

        match id {
            "bm-sq0" | "bm-sq1" => {
                let qr = run_metric_queries(
                    &installed,
                    depth,
                    sweep.query_iters,
                    None,
                    visibility_timeout_ms,
                )
                .await?;
                report.query_metrics_ms = Some(qr.stats);
                report.points_returned = Some(qr.points_returned);
            }
            "bm-sq2" => {
                let hit = run_metric_queries(
                    &installed,
                    depth,
                    sweep.query_iters,
                    Some("hit"),
                    visibility_timeout_ms,
                )
                .await?;
                let miss = run_metric_queries(
                    &installed,
                    depth,
                    sweep.query_iters,
                    Some("miss"),
                    visibility_timeout_ms,
                )
                .await?;
                report.query_metrics_ms = Some(hit.stats);
                report.points_returned = Some(hit.points_returned);
                report.label_filter = Some(format!(
                    "hit_p95={:.3} miss_p95={:.3}",
                    hit.stats.p95, miss.stats.p95
                ));
            }
            "bm-sq3" => {
                let qr =
                    run_event_queries(&installed, depth, sweep.query_iters, visibility_timeout_ms)
                        .await?;
                report.query_events_ms = Some(qr.stats);
                report.points_returned = Some(qr.points_returned);
            }
            _ => bail!("unmapped query experiment {id}"),
        }

        out.push(report.to_json());
    }
    Ok(out)
}

async fn install(matrix: &MatrixSpec, slug_suffix: &str) -> Result<InstalledSpectra> {
    install_bench_matrix(matrix.clone(), slug_suffix).await
}

async fn install_batched(
    matrix: &MatrixSpec,
    slug_suffix: &str,
    sweep: &SweepParams,
) -> Result<InstalledSpectra> {
    // Bound the L2 queue so flush after a firehose stays finite on remote DW.
    // (A 1M+ queue at small batch_max can take tens of minutes to drain.)
    let queue_max = match matrix.topology {
        Topology::RemoteIngest => (sweep.batch_max.saturating_mul(256)).clamp(16_384, 131_072),
        Topology::Embedded => 1_048_576,
    };
    let persist = PersistConfig {
        queue_max,
        batch_max: sweep.batch_max,
        batch_enabled: true,
        ..PersistConfig::default()
    };
    install_bench_matrix_with_persist(matrix.clone(), slug_suffix, Some(persist)).await
}

fn validate_capacity_matrix(matrix: &MatrixSpec) -> Result<()> {
    assert_embedded_topology(matrix.storage, matrix.topology)?;
    if !remote_env_ready(matrix.storage) {
        bail!(
            "remote env not ready for storage {:?} (set SPECTRA_TENSORBASE_URL / SPECTRA_CLICKHOUSE_URL or indexed _0/_1)",
            matrix.storage
        );
    }
    Ok(())
}

fn persist_drain_ms(topology: Topology) -> u64 {
    match topology {
        Topology::Embedded => 200,
        Topology::RemoteIngest => 2_000,
    }
}

fn visibility_timeout_ms(topology: Topology) -> u64 {
    match topology {
        Topology::Embedded => 2_000,
        Topology::RemoteIngest => 15_000,
    }
}

fn to_write_report(fh: &crate::workload::FirehoseResult) -> WriteReport {
    WriteReport {
        achieved_ops_per_sec: fh.achieved_ops_per_sec,
        total_ops: fh.total_ops,
        error_count: fh.error_count,
        error_rate: fh.error_rate,
    }
}

fn to_rootcause(rc: &spectra_core::RootcauseSnapshot) -> RootcauseReport {
    RootcauseReport {
        storage_writes_metrics: rc.storage_writes_metrics,
        storage_writes_events: rc.storage_writes_events,
        storage_wall_ms: rc.storage_wall_ms,
        persist_queue_drops: rc.persist_queue_drops,
    }
}

fn experiment_summary(id: &str) -> &'static str {
    crate::experiments::resolve_experiment(id)
        .map(|m| m.summary)
        .unwrap_or("unknown experiment")
}

pub fn write_report(path: &Path, body: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("create report directory")?;
    }
    fs::write(path, body).context("write report file")?;
    Ok(())
}
