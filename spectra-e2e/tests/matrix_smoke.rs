//! ScenarioSpec JSON roundtrip and registry smoke.

use spectra::SchemaRegistry;
use spectra_e2e::testkit::ScenarioSpec;

#[test]
fn scenario_platform_smoke_roundtrip_json() {
    let spec = ScenarioSpec::platform_smoke_roundtrip();
    let json = serde_json::to_string(&spec).expect("serialize");
    let back: ScenarioSpec = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.id, "platform-smoke-roundtrip");
    assert_eq!(back.steps.len(), spec.steps.len());
}

#[test]
fn registry_contains_platform_smoke_schemas() {
    let registry = SchemaRegistry::global();
    assert!(registry.get_schema("platform_smoke_event").is_some());
    assert!(registry.get_schema("platform_smoke_counter").is_some());
}
