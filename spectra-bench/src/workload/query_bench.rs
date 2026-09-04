use std::time::{Duration, Instant};

use anyhow::Result;
use chrono::{Duration as ChronoDuration, Utc};
use spectra_core::{EventsQueryFilter, LabelMatcher, MetricsQueryRange};
use spectra_testkit::InstalledSpectra;

use crate::report::metric_stats;
use crate::workload::prefill::{EVENT_PREFILL_TABLE, METRIC_PREFILL_NAME};

const VISIBILITY_POLL_INTERVAL_MS: u64 = 50;

#[derive(Debug, Clone)]
pub struct QueryBenchResult {
    pub stats: crate::report::LatencyStats,
    pub points_returned: u64,
}

pub async fn run_metric_queries(
    installed: &InstalledSpectra,
    prefill_count: u64,
    query_iters: u64,
    label_filter: Option<&str>,
    visibility_timeout_ms: u64,
) -> Result<QueryBenchResult> {
    let router = installed.spectra.router();
    let now = Utc::now();
    let start = now - ChronoDuration::hours(2);
    let label_matchers = match label_filter {
        Some("hit") => vec![LabelMatcher {
            key: "region".into(),
            value: "us-west".into(),
        }],
        Some("miss") => vec![LabelMatcher {
            key: "region".into(),
            value: "ap-south".into(),
        }],
        _ => vec![],
    };

    let expected = match label_filter {
        Some("hit") => prefill_count.div_ceil(2),
        Some("miss") => 0,
        _ => prefill_count,
    };

    // Wait until at least one matching point is visible before timing (remote may lag).
    if prefill_count > 0 && label_filter != Some("miss") {
        wait_until_metric_visible(
            installed,
            METRIC_PREFILL_NAME,
            &label_matchers,
            1,
            visibility_timeout_ms,
        )
        .await?;
    }

    let mut samples = Vec::with_capacity(query_iters as usize);
    let mut last_points = 0u64;
    for _ in 0..query_iters {
        let q_start = Instant::now();
        let points = router
            .query_metrics(MetricsQueryRange {
                metric_name: METRIC_PREFILL_NAME.to_string(),
                start,
                end: now + ChronoDuration::seconds(5),
                label_matchers: label_matchers.clone(),
            })
            .await?;
        samples.push(q_start.elapsed().as_secs_f64() * 1000.0);
        last_points = points.len() as u64;
    }

    if prefill_count > 0 && label_filter.is_none() && last_points == 0 {
        anyhow::bail!("query returned zero points after prefill count {prefill_count}");
    }
    let _ = expected;

    Ok(QueryBenchResult {
        stats: metric_stats(&samples),
        points_returned: last_points,
    })
}

pub async fn run_event_queries(
    installed: &InstalledSpectra,
    prefill_count: u64,
    query_iters: u64,
    visibility_timeout_ms: u64,
) -> Result<QueryBenchResult> {
    let router = installed.spectra.router();
    let now = Utc::now();
    let start = now - ChronoDuration::hours(2);

    if prefill_count > 0 {
        wait_until_event_visible(installed, EVENT_PREFILL_TABLE, 1, visibility_timeout_ms).await?;
    }

    let mut samples = Vec::with_capacity(query_iters as usize);
    let mut last_rows = 0u64;
    for _ in 0..query_iters {
        let q_start = Instant::now();
        let rows = router
            .query_events(EventsQueryFilter {
                table: EVENT_PREFILL_TABLE.to_string(),
                start: Some(start),
                end: Some(now + ChronoDuration::seconds(5)),
                limit: Some(prefill_count.min(10_000) as u32),
                ..Default::default()
            })
            .await?;
        samples.push(q_start.elapsed().as_secs_f64() * 1000.0);
        last_rows = rows.len() as u64;
    }

    Ok(QueryBenchResult {
        stats: metric_stats(&samples),
        points_returned: last_rows,
    })
}

pub async fn wait_until_metric_visible(
    installed: &InstalledSpectra,
    name: &str,
    label_matchers: &[LabelMatcher],
    min_points: u64,
    timeout_ms: u64,
) -> Result<()> {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let mut last;
    loop {
        last = count_metric_points(installed, name, label_matchers).await?;
        if last >= min_points {
            return Ok(());
        }
        if Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(VISIBILITY_POLL_INTERVAL_MS)).await;
    }
    anyhow::bail!(
        "timed out after {timeout_ms}ms waiting for metric {name} visibility (want>={min_points}, last={last})"
    )
}

pub async fn wait_until_event_visible(
    installed: &InstalledSpectra,
    table: &str,
    min_rows: u64,
    timeout_ms: u64,
) -> Result<()> {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let mut last;
    loop {
        last = count_event_rows(installed, table, 10_000).await?;
        if last >= min_rows {
            return Ok(());
        }
        if Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(VISIBILITY_POLL_INTERVAL_MS)).await;
    }
    anyhow::bail!(
        "timed out after {timeout_ms}ms waiting for event {table} visibility (want>={min_rows}, last={last})"
    )
}

/// Count metric points currently visible for `name` (capped by backend result size).
pub async fn count_metric_points(
    installed: &InstalledSpectra,
    name: &str,
    label_matchers: &[LabelMatcher],
) -> Result<u64> {
    let router = installed.spectra.router();
    let now = Utc::now();
    let points = router
        .query_metrics(MetricsQueryRange {
            metric_name: name.to_string(),
            start: now - ChronoDuration::hours(2),
            end: now + ChronoDuration::seconds(5),
            label_matchers: label_matchers.to_vec(),
        })
        .await?;
    Ok(points.len() as u64)
}

/// Count event rows currently visible for `table` (limit caps scan).
pub async fn count_event_rows(
    installed: &InstalledSpectra,
    table: &str,
    limit: u32,
) -> Result<u64> {
    let router = installed.spectra.router();
    let now = Utc::now();
    let rows = router
        .query_events(EventsQueryFilter {
            table: table.to_string(),
            start: Some(now - ChronoDuration::hours(2)),
            end: Some(now + ChronoDuration::seconds(5)),
            limit: Some(limit),
            ..Default::default()
        })
        .await?;
    Ok(rows.len() as u64)
}
