//! Multi-DW URL selection for durable write campaigns (n∈{1,2}+).
//!
//! Shard rule: `SPECTRA_BENCH_CLIENT_INDEX % SPECTRA_BENCH_DW_N`.
//! Prefer `SPECTRA_{CLICKHOUSE|TENSORBASE}_URL_{i}`; fall back to bare `SPECTRA_*_URL` when n=1.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use anyhow::{bail, Context, Result};

use crate::matrix::StorageAdapter;

/// Warehouse instance count (`SPECTRA_BENCH_DW_N`, default 1).
pub fn dw_n() -> u32 {
    std::env::var("SPECTRA_BENCH_DW_N")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1)
        .max(1)
}

/// This process's client index (`SPECTRA_BENCH_CLIENT_INDEX`, default 0).
pub fn client_index() -> u32 {
    std::env::var("SPECTRA_BENCH_CLIENT_INDEX")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

/// Shard index for this process: `client_index % dw_n`.
pub fn shard_index() -> u32 {
    client_index() % dw_n()
}

/// Resolve ClickHouse URL for the selected shard.
pub fn clickhouse_url_sharded() -> Result<String> {
    resolve_sharded_url("SPECTRA_CLICKHOUSE_URL", shard_index(), dw_n())
}

/// Resolve TensorBase URL for the selected shard.
pub fn tensorbase_url_sharded() -> Result<String> {
    resolve_sharded_url("SPECTRA_TENSORBASE_URL", shard_index(), dw_n())
}

/// Resolve remote URL for `storage` using multi-DW env (or bare URL for n=1).
pub fn remote_url_for(storage: StorageAdapter) -> Result<String> {
    match storage {
        StorageAdapter::ClickHouse => clickhouse_url_sharded(),
        StorageAdapter::TensorBase => tensorbase_url_sharded(),
        StorageAdapter::Mem | StorageAdapter::Sqlite => {
            bail!("remote_url_for is only for clickhouse/tensorbase")
        }
    }
}

/// Short fingerprint of a DW URL (host:port hash) for report stamping.
pub fn dw_url_fingerprint(url: &str) -> String {
    let mut hasher = DefaultHasher::new();
    url_host_port(url).hash(&mut hasher);
    format!("{:x}", hasher.finish() & 0xffff_ffff)
}

fn resolve_sharded_url(base_key: &str, shard: u32, n: u32) -> Result<String> {
    let indexed = format!("{base_key}_{shard}");
    if let Ok(url) = std::env::var(&indexed) {
        if !url.is_empty() {
            return Ok(url);
        }
    }
    if n == 1 {
        return std::env::var(base_key)
            .with_context(|| format!("{base_key} (or {indexed}) required for remote matrix rows"));
    }
    // n>1: require indexed URLs for every shard so misconfig fails loudly.
    for i in 0..n {
        let key = format!("{base_key}_{i}");
        if std::env::var(&key).ok().is_none_or(|s| s.is_empty()) {
            bail!("{key} required when SPECTRA_BENCH_DW_N={n} (got missing/empty for shard {i})");
        }
    }
    std::env::var(&indexed)
        .with_context(|| format!("{indexed} required when SPECTRA_BENCH_DW_N={n}"))
}

fn url_host_port(url: &str) -> String {
    // http://host:8123/... or tcp://host:9528
    let rest = url.split("://").nth(1).unwrap_or(url);
    rest.split('/').next().unwrap_or(rest).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_stable() {
        let a = dw_url_fingerprint("http://10.0.0.1:8123");
        let b = dw_url_fingerprint("http://10.0.0.1:8123/db");
        assert_eq!(a, b);
        assert_ne!(a, dw_url_fingerprint("http://10.0.0.2:8123"));
    }
}
