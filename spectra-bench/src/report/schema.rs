use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use spectra_testkit::MatrixSpec;

use super::stats::{latency_json, LatencyStats};
use crate::sweep::SweepParams;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatrixReport {
    pub storage: String,
    pub topology: String,
    pub telemetry: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SweepReport {
    pub prefill: Option<u64>,
    pub query_iters: Option<u64>,
    pub duration_secs: Option<u64>,
    pub concurrency: Option<u32>,
    pub bench_clients: Option<u32>,
    pub bench_client_index: Option<u32>,
    /// Warehouse instance count (`SPECTRA_BENCH_DW_N`).
    pub dw_n: Option<u32>,
    /// L2 batch_max for BM-SW7.
    pub batch_max: Option<usize>,
    /// Writer process count for ladder campaigns.
    pub writer_n: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteReport {
    pub achieved_ops_per_sec: f64,
    pub total_ops: u64,
    pub error_count: u64,
    pub error_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootcauseReport {
    pub storage_writes_metrics: u64,
    pub storage_writes_events: u64,
    pub storage_wall_ms: f64,
    pub persist_queue_drops: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostUtilReport {
    pub role: String,
    pub cpu_avg_pct: Option<f64>,
    pub cpu_peak_pct: Option<f64>,
    pub mem_used_pct: Option<f64>,
    pub mem_available_mb: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BenchReport {
    pub experiment: String,
    pub summary: String,
    pub matrix: MatrixReport,
    /// Hardware label from `SPECTRA_BENCH_HARDWARE` (for example `aws-t3-xlarge`).
    pub hardware: Option<String>,
    pub sweep: SweepReport,
    pub metric_kind: String,
    pub prefill_count: Option<u64>,
    pub prefill_elapsed_ms: Option<f64>,
    pub points_returned: Option<u64>,
    pub achieved_counter_ops_per_sec: Option<f64>,
    pub achieved_event_ops_per_sec: Option<f64>,
    pub achieved_adapter_ops_per_sec: Option<f64>,
    /// Durable counter ops/s (awaited Spectra→DW writes).
    pub durable_counter_ops_per_sec: Option<f64>,
    /// Durable event ops/s (awaited Spectra→DW writes).
    pub durable_event_ops_per_sec: Option<f64>,
    /// Warehouse count for multi-DW campaigns.
    pub n: Option<u32>,
    /// Shard index this process wrote to.
    pub shard: Option<u32>,
    /// Fingerprint of the DW URL (host:port hash).
    pub dw_url_fingerprint: Option<String>,
    /// `subscriber-sim` for BM-SW5/SW6 (no live bus).
    pub path: Option<String>,
    /// `dw` | `client-cpu` | `unset`
    pub binding_tier: Option<String>,
    /// True when at least one row/point was query-visible after the firehose.
    pub visibility_confirmed: Option<bool>,
    /// L2 `PersistConfig.batch_max` (BM-SW7).
    pub batch_max: Option<usize>,
    /// Writer process count for ladder campaigns (BM-SW7).
    pub writer_n: Option<u32>,
    pub host_util: Option<Vec<HostUtilReport>>,
    pub query_metrics_ms: Option<LatencyStats>,
    pub query_events_ms: Option<LatencyStats>,
    pub label_filter: Option<String>,
    pub write: Option<WriteReport>,
    pub rootcause: Option<RootcauseReport>,
}

impl BenchReport {
    pub fn base(experiment: &str, summary: &str, matrix: &MatrixSpec, sweep: &SweepParams) -> Self {
        Self {
            experiment: experiment.to_string(),
            summary: summary.to_string(),
            matrix: MatrixReport {
                storage: matrix.storage.as_str().to_string(),
                topology: matrix.topology.as_str().to_string(),
                telemetry: matrix.telemetry.as_str().to_string(),
            },
            hardware: std::env::var("SPECTRA_BENCH_HARDWARE").ok(),
            sweep: SweepReport {
                prefill: sweep.prefill,
                query_iters: Some(sweep.query_iters),
                duration_secs: Some(sweep.duration.as_secs()),
                concurrency: Some(sweep.concurrency),
                bench_clients: Some(sweep.bench_clients),
                bench_client_index: Some(sweep.bench_client_index),
                dw_n: Some(sweep.dw_n),
                batch_max: Some(sweep.batch_max),
                writer_n: Some(sweep.writer_n),
            },
            metric_kind: String::new(),
            prefill_count: None,
            prefill_elapsed_ms: None,
            points_returned: None,
            achieved_counter_ops_per_sec: None,
            achieved_event_ops_per_sec: None,
            achieved_adapter_ops_per_sec: None,
            durable_counter_ops_per_sec: None,
            durable_event_ops_per_sec: None,
            n: None,
            shard: None,
            dw_url_fingerprint: None,
            path: None,
            binding_tier: None,
            visibility_confirmed: None,
            batch_max: None,
            writer_n: None,
            host_util: None,
            query_metrics_ms: None,
            query_events_ms: None,
            label_filter: None,
            write: None,
            rootcause: None,
        }
    }

    pub fn to_json(&self) -> Value {
        json!({
            "experiment": self.experiment,
            "summary": self.summary,
            "matrix": self.matrix,
            "hardware": self.hardware,
            "sweep": self.sweep,
            "metric_kind": self.metric_kind,
            "prefill_count": self.prefill_count,
            "prefill_elapsed_ms": self.prefill_elapsed_ms,
            "points_returned": self.points_returned,
            "achieved_counter_ops_per_sec": self.achieved_counter_ops_per_sec,
            "achieved_event_ops_per_sec": self.achieved_event_ops_per_sec,
            "achieved_adapter_ops_per_sec": self.achieved_adapter_ops_per_sec,
            "durable_counter_ops_per_sec": self.durable_counter_ops_per_sec,
            "durable_event_ops_per_sec": self.durable_event_ops_per_sec,
            "n": self.n,
            "shard": self.shard,
            "dw_url_fingerprint": self.dw_url_fingerprint,
            "path": self.path,
            "binding_tier": self.binding_tier,
            "visibility_confirmed": self.visibility_confirmed,
            "batch_max": self.batch_max,
            "writer_n": self.writer_n,
            "host_util": self.host_util,
            "query_metrics_ms": self.query_metrics_ms.as_ref().map(latency_json),
            "query_events_ms": self.query_events_ms.as_ref().map(latency_json),
            "label_filter": self.label_filter,
            "write": self.write,
            "rootcause": self.rootcause,
        })
    }
}
