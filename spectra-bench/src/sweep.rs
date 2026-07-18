//! CLI and env sweep parameter resolution.

use std::time::Duration;

/// Resolved sweep knobs for one capacity run.
#[derive(Debug, Clone)]
pub struct SweepParams {
    pub prefill: Option<u64>,
    pub prefill_sweep: Vec<u64>,
    pub query_iters: u64,
    pub duration: Duration,
    pub concurrency: u32,
    pub bench_clients: u32,
    pub bench_client_index: u32,
    /// Warehouse instance count (`SPECTRA_BENCH_DW_N`).
    pub dw_n: u32,
    /// L2 `PersistConfig.batch_max` for BM-SW7 (`SPECTRA_BENCH_BATCH_MAX` / `--batch-max`).
    pub batch_max: usize,
    /// Writer process count for ladder campaigns (`SPECTRA_BENCH_WRITER_N`).
    pub writer_n: u32,
}

impl Default for SweepParams {
    fn default() -> Self {
        Self {
            prefill: None,
            prefill_sweep: default_prefill_sweep(),
            query_iters: 1000,
            duration: Duration::from_secs(30),
            concurrency: 256,
            bench_clients: 1,
            bench_client_index: 0,
            dw_n: 1,
            batch_max: 32,
            writer_n: 1,
        }
    }
}

pub fn default_prefill_sweep() -> Vec<u64> {
    vec![1_000, 10_000, 100_000, 1_000_000]
}

/// Raw CLI sweep inputs before env fallback.
#[derive(Debug, Clone, Default)]
pub struct SweepCli {
    pub prefill: Option<u64>,
    pub prefill_sweep: Option<String>,
    pub query_iters: Option<u64>,
    pub duration_secs: Option<u64>,
    pub concurrency: Option<u32>,
    pub bench_clients: Option<u32>,
    pub batch_max: Option<usize>,
}

impl SweepParams {
    pub fn resolve(cli: &SweepCli) -> Self {
        let mut params = Self::default();
        params.prefill = cli.prefill.or_else(|| env_u64("SPECTRA_BENCH_PREFILL"));
        if let Some(raw) = &cli.prefill_sweep {
            params.prefill_sweep = parse_u64_list(raw);
        }
        params.query_iters = cli
            .query_iters
            .or_else(|| env_u64("SPECTRA_BENCH_QUERY_ITERS"))
            .unwrap_or(params.query_iters);
        let duration_secs = cli
            .duration_secs
            .or_else(|| env_u64("SPECTRA_BENCH_DURATION_SECS"))
            .unwrap_or(30);
        params.duration = Duration::from_secs(duration_secs.max(1));
        params.concurrency = cli
            .concurrency
            .or_else(|| env_u32("SPECTRA_BENCH_CONCURRENCY"))
            .unwrap_or(params.concurrency)
            .max(1);
        params.bench_clients = cli
            .bench_clients
            .or_else(|| env_u32("SPECTRA_BENCH_CLIENT_COUNT"))
            .unwrap_or(params.bench_clients)
            .max(1);
        params.bench_client_index = env_u32("SPECTRA_BENCH_CLIENT_INDEX").unwrap_or(0);
        params.dw_n = env_u32("SPECTRA_BENCH_DW_N").unwrap_or(1).max(1);
        params.batch_max = cli
            .batch_max
            .or_else(|| env_usize("SPECTRA_BENCH_BATCH_MAX"))
            .unwrap_or(params.batch_max)
            .max(1);
        params.writer_n = env_u32("SPECTRA_BENCH_WRITER_N")
            .unwrap_or(params.bench_clients)
            .max(1);
        params
    }

    /// Depth values to run for query experiments.
    pub fn prefill_depths(&self) -> Vec<u64> {
        if let Some(n) = self.prefill {
            vec![n]
        } else {
            self.prefill_sweep.clone()
        }
    }
}

fn env_u64(key: &str) -> Option<u64> {
    std::env::var(key).ok()?.parse().ok()
}

fn env_u32(key: &str) -> Option<u32> {
    std::env::var(key).ok()?.parse().ok()
}

fn env_usize(key: &str) -> Option<usize> {
    std::env::var(key).ok()?.parse().ok()
}

pub fn parse_u64_list(raw: &str) -> Vec<u64> {
    raw.split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect()
}
