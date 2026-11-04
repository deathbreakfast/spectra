//! Per-store backend routing: each `store:` name resolves to its own backend pair, and a
//! store with no registration falls back to the default pair.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use spectra::spectra_core::{
    current_emit_ts, EventStorageBackend, EventsQueryFilter, MetricsQueryRange,
    MetricsStorageBackend,
};
use spectra::{MemEventsBackend, MemMetricsBackend, Spectra};

mod alpha_event {
    spectra::spectra_schema! {
        RoutingAlphaEvent {
            store: "routing_alpha",
            table: "routing_alpha_event",
            version: "0.1.0",
            description: "Event declared on the routing_alpha store",
            fields: [
                message: {
                    r#type: String,
                    classification: { pii: false, safe_for_console: true },
                },
            ],
        }
    }
}

mod alpha_counter {
    spectra::spectra_metric! {
        RoutingAlphaCounter {
            store: "routing_alpha",
            name: "routing_alpha_counter",
            version: "0.1.0",
            description: "Counter declared on the routing_alpha store",
        }
    }
}

mod beta_event {
    spectra::spectra_schema! {
        RoutingBetaEvent {
            store: "routing_beta",
            table: "routing_beta_event",
            version: "0.1.0",
            description: "Event declared on the routing_beta store",
            fields: [
                message: {
                    r#type: String,
                    classification: { pii: false, safe_for_console: true },
                },
            ],
        }
    }
}

mod default_event {
    spectra::spectra_schema! {
        RoutingDefaultEvent {
            store: "default",
            table: "routing_default_event",
            version: "0.1.0",
            description: "Event declared on the default store",
            fields: [
                message: {
                    r#type: String,
                    classification: { pii: false, safe_for_console: true },
                },
            ],
        }
    }
}

mod unregistered_event {
    spectra::spectra_schema! {
        RoutingUnregisteredEvent {
            store: "routing_unregistered",
            table: "routing_unregistered_event",
            version: "0.1.0",
            description: "Event declared on a store the host never registers",
            fields: [
                message: {
                    r#type: String,
                    classification: { pii: false, safe_for_console: true },
                },
            ],
        }
    }
}

use alpha_counter::RoutingAlphaCounterRecorder;
use alpha_event::RoutingAlphaEventLogger;
use beta_event::RoutingBetaEventLogger;
use default_event::RoutingDefaultEventLogger;
use unregistered_event::RoutingUnregisteredEventLogger;

/// `Spectra::build` installs a process-global router and sink, so tests in this binary
/// take turns.
static ROUTING_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

struct Backends {
    metrics: Arc<MemMetricsBackend>,
    events: Arc<MemEventsBackend>,
}

impl Backends {
    fn new() -> Self {
        Self {
            metrics: Arc::new(MemMetricsBackend::new()),
            events: Arc::new(MemEventsBackend::new()),
        }
    }

    async fn event_count(&self, table: &str) -> usize {
        let now = current_emit_ts();
        self.events
            .query_rows(EventsQueryFilter {
                table: table.into(),
                start: Some(now - chrono::Duration::seconds(30)),
                end: Some(now + chrono::Duration::seconds(5)),
                ..Default::default()
            })
            .await
            .expect("query events")
            .len()
    }

    async fn metric_count(&self, name: &str) -> usize {
        let now = current_emit_ts();
        self.metrics
            .query_range(MetricsQueryRange {
                metric_name: name.into(),
                start: now - chrono::Duration::seconds(30),
                end: now + chrono::Duration::seconds(5),
                label_matchers: vec![],
            })
            .await
            .expect("query metrics")
            .len()
    }
}

fn build(default: &Backends, stores: &[(&str, &Backends)]) -> Spectra {
    let mut builder = Spectra::builder()
        .metrics_backend(Arc::clone(&default.metrics) as Arc<dyn MetricsStorageBackend>)
        .events_backend(Arc::clone(&default.events) as Arc<dyn EventStorageBackend>);
    for (name, backends) in stores {
        builder = builder.store_backend(
            *name,
            Arc::clone(&backends.metrics) as Arc<dyn MetricsStorageBackend>,
            Arc::clone(&backends.events) as Arc<dyn EventStorageBackend>,
        );
    }
    builder.embedded().build().expect("build spectra")
}

async fn settle(spectra: &Spectra) {
    spectra.flush_persist().await.expect("flush persist");
}

#[tokio::test]
async fn declared_store_receives_its_events_and_metrics_happy() {
    let _guard = ROUTING_LOCK.lock().await;
    let default = Backends::new();
    let alpha = Backends::new();
    let spectra = build(&default, &[("routing_alpha", &alpha)]);

    RoutingAlphaEventLogger::log("to alpha".to_string());
    RoutingAlphaCounterRecorder::record(1, serde_json::json!({}));
    settle(&spectra).await;

    assert_eq!(alpha.event_count("routing_alpha_event").await, 1);
    assert_eq!(alpha.metric_count("routing_alpha_counter").await, 1);
    assert_eq!(default.event_count("routing_alpha_event").await, 0);
    assert_eq!(default.metric_count("routing_alpha_counter").await, 0);
}

#[tokio::test]
async fn default_store_stays_on_default_backend_happy() {
    let _guard = ROUTING_LOCK.lock().await;
    let default = Backends::new();
    let alpha = Backends::new();
    let spectra = build(&default, &[("routing_alpha", &alpha)]);

    RoutingDefaultEventLogger::log("to default".to_string());
    settle(&spectra).await;

    assert_eq!(default.event_count("routing_default_event").await, 1);
    assert_eq!(alpha.event_count("routing_default_event").await, 0);
}

#[tokio::test]
async fn two_stores_do_not_cross_contaminate_happy() {
    let _guard = ROUTING_LOCK.lock().await;
    let default = Backends::new();
    let alpha = Backends::new();
    let beta = Backends::new();
    let spectra = build(
        &default,
        &[("routing_alpha", &alpha), ("routing_beta", &beta)],
    );

    RoutingAlphaEventLogger::log("a".to_string());
    RoutingBetaEventLogger::log("b".to_string());
    settle(&spectra).await;

    assert_eq!(alpha.event_count("routing_alpha_event").await, 1);
    assert_eq!(alpha.event_count("routing_beta_event").await, 0);
    assert_eq!(beta.event_count("routing_beta_event").await, 1);
    assert_eq!(beta.event_count("routing_alpha_event").await, 0);
    assert_eq!(default.event_count("routing_alpha_event").await, 0);
    assert_eq!(default.event_count("routing_beta_event").await, 0);
}

#[tokio::test]
async fn unregistered_store_falls_back_to_default_sad() {
    let _guard = ROUTING_LOCK.lock().await;
    let default = Backends::new();
    let alpha = Backends::new();
    let spectra = build(&default, &[("routing_alpha", &alpha)]);

    RoutingUnregisteredEventLogger::log("no dedicated backend".to_string());
    settle(&spectra).await;

    assert_eq!(default.event_count("routing_unregistered_event").await, 1);
    assert_eq!(alpha.event_count("routing_unregistered_event").await, 0);
}

#[tokio::test]
async fn re_registering_a_store_last_call_wins_sad() {
    let _guard = ROUTING_LOCK.lock().await;
    let default = Backends::new();
    let first = Backends::new();
    let second = Backends::new();
    let spectra = build(
        &default,
        &[("routing_alpha", &first), ("routing_alpha", &second)],
    );

    RoutingAlphaEventLogger::log("to the second registration".to_string());
    settle(&spectra).await;

    assert_eq!(second.event_count("routing_alpha_event").await, 1);
    assert_eq!(first.event_count("routing_alpha_event").await, 0);
    assert_eq!(default.event_count("routing_alpha_event").await, 0);
}
