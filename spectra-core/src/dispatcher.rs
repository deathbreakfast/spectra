use std::cell::Cell;
use std::sync::{Arc, OnceLock, RwLock};

use crate::sink::SpectraSink;

static SINK: OnceLock<RwLock<Option<Arc<dyn SpectraSink>>>> = OnceLock::new();

thread_local! {
    static DISPATCH_DEPTH: Cell<u32> = const { Cell::new(0) };
}

fn sink_slot() -> &'static RwLock<Option<Arc<dyn SpectraSink>>> {
    SINK.get_or_init(|| RwLock::new(None))
}

/// Install the process-wide sink (typically once at server boot).
pub fn set_sink(sink: Arc<dyn SpectraSink>) {
    let mut guard = sink_slot().write().expect("spectra-core sink lock");
    *guard = Some(sink);
}

fn current_sink() -> Option<Arc<dyn SpectraSink>> {
    sink_slot().read().ok()?.clone()
}

/// Returns true when dispatch should proceed (not re-entrant).
pub(crate) fn enter_dispatch_counter(name: &str, f: impl FnOnce(&dyn SpectraSink)) {
    let _ = name;
    enter_dispatch_inner(f);
}

pub(crate) fn enter_dispatch_gauge(name: &str, f: impl FnOnce(&dyn SpectraSink)) {
    let _ = name;
    enter_dispatch_inner(f);
}

pub(crate) fn enter_dispatch_event(table: &str, f: impl FnOnce(&dyn SpectraSink)) {
    let _ = table;
    enter_dispatch_inner(f);
}

fn enter_dispatch_inner<F>(f: F)
where
    F: FnOnce(&dyn SpectraSink),
{
    DISPATCH_DEPTH.with(|depth| {
        if depth.get() > 0 {
            return;
        }
        depth.set(depth.get().saturating_add(1));
        let start = crate::rootcause::enabled().then(std::time::Instant::now);
        if let Some(sink) = current_sink() {
            f(sink.as_ref());
        }
        if let Some(start) = start {
            crate::rootcause::record_inline_dispatch(start.elapsed());
        }
        depth.set(depth.get().saturating_sub(1));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sinks::NoOpSink;
    use std::sync::atomic::{AtomicU32, Ordering};

    struct CountingSink {
        calls: AtomicU32,
    }

    impl SpectraSink for CountingSink {
        fn record_counter(&self, _name: &str, _labels: &[(&str, &str)], _delta: i64) {
            if DISPATCH_DEPTH.with(|d| d.get()) > 1 {
                return;
            }
            self.calls.fetch_add(1, Ordering::SeqCst);
            // Simulate re-entrant call from inside sink.
            crate::try_record_counter("nested", &[], 1);
        }

        fn record_gauge(&self, _name: &str, _labels: &[(&str, &str)], _value: f64) {}

        fn log_event(&self, _table: &str, _fields: &serde_json::Value) {}
    }

    #[test]
    fn reentrancy_guard_skips_nested_emit() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let _g = crate::test_util::GLOBAL_TEST_LOCK.lock().await;
            crate::test_util::reset_gate_disabled();
            let sink = Arc::new(CountingSink {
                calls: AtomicU32::new(0),
            });
            set_sink(sink.clone());
            crate::try_record_counter("outer", &[], 1);
            assert_eq!(sink.calls.load(Ordering::SeqCst), 1);
            set_sink(Arc::new(NoOpSink));
            crate::config::reset_config_for_test();
        });
    }

    #[test]
    fn unset_sink_is_noop() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let _g = crate::test_util::GLOBAL_TEST_LOCK.lock().await;
            crate::config::reset_config_for_test();
            set_sink(Arc::new(NoOpSink));
            crate::try_record_counter("x", &[], 1);
        });
    }
}
