//! Query roundtrip + PII masking for console/UI display.
//!
//! Emits a smoke event, queries it back, and demonstrates [`mask_field_value`] so hosts never
//! print classified PII in the clear. Event query paging is clamped by the library
//! (`MAX_EVENT_QUERY_LIMIT`).
//!
//! ```bash
//! cargo run -p uf-spectra --example query_and_pii_console --features mem
//! ```

#![allow(clippy::print_stderr)]

use std::sync::Arc;

use serde_json::json;
use spectra::helpers::PlatformSmokeEventLogger;
use spectra::{
    mask_field_value, FieldClassification, MemEventsBackend, MemMetricsBackend, Spectra, PII_MASK,
};
use spectra_core::{
    current_emit_ts, EventsQueryFilter, SharedEventBackend, SharedMetricsBackend,
};

#[tokio::main]
async fn main() -> spectra::Result<()> {
    let metrics: SharedMetricsBackend = Arc::new(MemMetricsBackend::new());
    let events: SharedEventBackend = Arc::new(MemEventsBackend::new());
    let spectra = Spectra::builder()
        .metrics_backend(metrics)
        .events_backend(events)
        .embedded()
        .build()?;

    PlatformSmokeEventLogger::log("query-and-pii console demo".to_string());
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let now = current_emit_ts();
    let rows = spectra
        .router()
        .query_events(EventsQueryFilter {
            table: "platform_smoke_event".into(),
            start: Some(now - chrono::Duration::seconds(30)),
            end: Some(now + chrono::Duration::seconds(5)),
            limit: Some(10),
            ..Default::default()
        })
        .await?;

    let pii = FieldClassification {
        pii: true,
        safe_for_console: false,
        retention_days: None,
        purpose: None,
    };
    let safe = FieldClassification {
        pii: false,
        safe_for_console: true,
        retention_days: None,
        purpose: None,
    };
    let email = json!("alice@example.com");
    let region = json!("us-west");
    let masked = mask_field_value(&pii, &email);
    let plain = mask_field_value(&safe, &region);
    assert_eq!(masked, PII_MASK);
    assert_eq!(plain, "us-west");

    eprintln!(
        "query-and-pii OK: {} event row(s); PII masked as {masked:?}; safe field={plain}",
        rows.len()
    );
    if rows.is_empty() {
        std::process::exit(1);
    }
    Ok(())
}
