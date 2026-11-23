//! Smoke schema inventory and typed-helper roundtrip.

use std::sync::Arc;

use spectra::helpers::{PlatformSmokeCounterRecorder, PlatformSmokeEventLogger};
use spectra::{MemEventsBackend, MemMetricsBackend, SchemaRegistry, Spectra};
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

    let record_line = line!() + 1;
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

    let label = points[0].labels[spectra::CALLSITE_FIELD]
        .as_str()
        .expect("counter carries a _callsite label");
    let callsite: serde_json::Value = serde_json::from_str(label).expect("_callsite label is JSON");
    assert_callsite(&callsite, record_line);
}

/// Asserts a `_callsite` object names this file, `line` and this test crate's module.
fn assert_callsite(callsite: &serde_json::Value, line: u32) {
    assert!(
        callsite["file"]
            .as_str()
            .is_some_and(|f| f.ends_with("smoke_inventory.rs")),
        "callsite file should be the caller, got {callsite}"
    );
    assert_eq!(callsite["line"], line, "callsite line, got {callsite}");
    assert_eq!(
        callsite["module"], "smoke_inventory",
        "callsite module, got {callsite}"
    );
}

#[tokio::test]
async fn direct_emit_records_callsite() {
    let _guard = SMOKE_TEST_LOCK.lock().await;
    let metrics: Arc<dyn MetricsStorageBackend> = Arc::new(MemMetricsBackend::new());
    let events: Arc<dyn EventStorageBackend> = Arc::new(MemEventsBackend::new());

    let spectra = Spectra::builder()
        .metrics_backend(Arc::clone(&metrics))
        .events_backend(Arc::clone(&events))
        .embedded()
        .build()
        .expect("build spectra");

    let emit_line = line!() + 1;
    spectra::try_log_event_at(
        "platform_smoke_event",
        &serde_json::json!({ "message": "direct" }),
        current_emit_ts(),
    );
    spectra::try_log_event_now(
        "platform_smoke_event",
        &serde_json::json!({
            "message": "explicit",
            "_callsite": { "file": "elsewhere.rs", "line": 9 },
        }),
    );
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
    assert_eq!(rows.len(), 2, "both direct emits should persist");
    let direct = rows
        .iter()
        .find(|r| r.fields["message"] == "direct")
        .expect("direct row");
    assert_callsite(&direct.fields[spectra::CALLSITE_FIELD], emit_line);
    let explicit = rows
        .iter()
        .find(|r| r.fields["message"] == "explicit")
        .expect("explicit row");
    assert_eq!(
        explicit.fields[spectra::CALLSITE_FIELD]["file"],
        "elsewhere.rs",
        "an explicit _callsite is kept"
    );
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

    let log_line = line!() + 1;
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

    let fields = &rows[0].fields;
    assert_eq!(fields["message"], "phase4 smoke");
    assert_callsite(&fields[spectra::CALLSITE_FIELD], log_line);
}
