//! Serialize tests that touch process-global sink / gate state.

/// Cross-crate test lock for global sink / gate state (spectra-runtime integration tests).
pub static GLOBAL_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[cfg(test)]
pub fn reset_gate_disabled() {
    reset_config_and_sink_for_test();
}

/// Reset installed gate config and sink between integration tests in dependent crates.
pub fn reset_config_and_sink_for_test() {
    crate::config::reset_config_for_test();
    crate::set_sink(std::sync::Arc::new(crate::sinks::NoOpSink));
}
