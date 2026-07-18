//! Smoke schema inventory and typed-helper roundtrip.

use std::sync::Arc;

use spectra::helpers::{PlatformSmokeCounterRecorder, PlatformSmokeEventLogger};
use spectra::{MemEventsBackend, MemMetricsBackend, Spectra, SchemaRegistry};
use spectra_core::{
    current_emit_ts, EventStorageBackend, MetricsQueryRange, MetricsStorageBackend,
};

static SMOKE_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[test]
fn registry_smoke_schemas_discovered() {
    let registry = SchemaRegistry::global();
    assert!(
        registry.get_schema("platform_smoke_event").is_some(),
        "platform_smoke_event should be registered"
    );
    assert!(
        registry.get_schema("platform_smoke_counter").is_some(),
        "platform_smoke_counter should be registered"
    );
    let names = registry.list_schemas();
    assert!(names.contains(&"platform_smoke_event"));
    assert!(names.contains(&"platform_smoke_counter"));
}

#[tokio::test]
async fn emit_smoke_counter_roundtrip() {
    let _guard = SMOKE_TEST_LOCK.lock().await;
    let metrics: Arc<dyn MetricsStorageBackend> = Arc::new(MemMetricsBackend::new());
    let events: Arc<dyn EventStorageBackend> = Arc::new(MemEventsBackend::new());

    let spectra = Spectra::builder()
        .metrics_backend(Arc::clone(&metrics))
        .events_backend(Arc::clone(&events))
        .embedded()
        .build()
        .expect("build spectra");

    PlatformSmokeCounterRecorder::record(1, serde_json::json!({}));
    tokio::time::sleep(std::time::Duration::from_millis(80)).await;

    let now = current_emit_ts();
    let points = spectra
        .router()
        .query_metrics(MetricsQueryRange {
            metric_name: "platform_smoke_counter".into(),
            start: now - chrono::Duration::seconds(5),
            end: now + chrono::Duration::seconds(1),
            label_matchers: vec![],
        })
        .await
        .expect("query metrics");
    assert_eq!(points.len(), 1, "counter should persist via mem backend");
}

#[tokio::test]
async fn emit_smoke_event_roundtrip() {
    let _guard = SMOKE_TEST_LOCK.lock().await;
    let metrics: Arc<dyn MetricsStorageBackend> = Arc::new(MemMetricsBackend::new());
    let events: Arc<dyn EventStorageBackend> = Arc::new(MemEventsBackend::new());

    let spectra = Spectra::builder()
        .metrics_backend(Arc::clone(&metrics))
        .events_backend(Arc::clone(&events))
        .embedded()
        .build()
        .expect("build spectra");

    PlatformSmokeEventLogger::log("phase4 smoke".to_string());
    tokio::time::sleep(std::time::Duration::from_millis(80)).await;

    let now = current_emit_ts();
    let rows = spectra
        .router()
        .query_events(spectra_core::EventsQueryFilter {
            table: "platform_smoke_event".into(),
            start: Some(now - chrono::Duration::seconds(5)),
            end: Some(now + chrono::Duration::seconds(1)),
            ..Default::default()
        })
        .await
        .expect("query events");
    assert_eq!(rows.len(), 1, "event should persist via mem backend");
}
