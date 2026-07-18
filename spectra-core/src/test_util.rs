//! Serialize tests that touch process-global sink / gate state.

#[cfg(test)]
pub static GLOBAL_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[cfg(test)]
pub fn reset_gate_disabled() {
    crate::config::reset_config_for_test();
    crate::config::install_config(crate::config::SpectraConfig {
        enabled: false,
        ..Default::default()
    });
}
