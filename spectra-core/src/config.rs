//! Runtime Spectra emit-volume configuration (`SPECTRA_LEVEL`, per-name overrides, TOML).

use std::collections::HashMap;
use std::sync::OnceLock;

use parking_lot::RwLock;

use crate::registry::{SchemaMetadata, SchemaRegistry, SpectraLevel};

/// Per-name runtime override merged on top of schema defaults.
#[derive(Debug, Clone, Default)]
pub struct NameOverride {
    /// Override minimum verbosity tier for this name.
    pub level: Option<SpectraLevel>,
    /// Override sampling rate for this name (`0.0`–`1.0`).
    pub sample_rate: Option<f64>,
    /// `Some(None)` clears coalesce; `Some(Some(ms))` sets it; `None` leaves schema default.
    pub coalesce_ms: Option<Option<u64>>,
}

/// Host boot configuration for the emit gate.
///
/// Schema defaults (`level`, `default_sample_rate`, and metric `coalesce_ms`) come from the
/// `spectra_schema!` / `spectra_metric!` DSL; env and TOML overrides here merge on top at runtime.
#[derive(Debug, Clone)]
pub struct SpectraConfig {
    /// Global minimum verbosity tier (`SPECTRA_LEVEL`, default Info).
    pub min_level: SpectraLevel,
    /// Floor multiplier applied after the level check (default 1.0).
    pub global_sample_rate: f64,
    /// Per metric/event name overrides (env `SPECTRA_SAMPLE_<NAME>` + TOML).
    pub per_name: HashMap<String, NameOverride>,
    /// When false (`SPECTRA_GATE=0`), the gate is disabled (fail-open).
    pub enabled: bool,
}

impl Default for SpectraConfig {
    fn default() -> Self {
        Self {
            min_level: SpectraLevel::Info,
            global_sample_rate: 1.0,
            per_name: HashMap::new(),
            enabled: true,
        }
    }
}

impl SpectraConfig {
    /// Load from environment and optional TOML file (`SPECTRA_CONFIG` path).
    ///
    /// Schema `level` / `default_sample_rate` / `coalesce_ms` defaults are declared in the
    /// `spectra_schema!` / `spectra_metric!` DSL; this method applies runtime overrides on top.
    ///
    /// # Examples
    ///
    /// ```
    /// use spectra_core::SpectraConfig;
    ///
    /// // Reads `SPECTRA_GATE`, `SPECTRA_LEVEL`, `SPECTRA_SAMPLE_*`, and optional `SPECTRA_CONFIG`.
    /// let config = SpectraConfig::from_env();
    /// assert!((0.0..=1.0).contains(&config.global_sample_rate));
    /// ```
    pub fn from_env() -> Self {
        let mut config = Self::default();

        if matches!(
            std::env::var("SPECTRA_GATE").as_deref(),
            Ok("0") | Ok("false") | Ok("FALSE") | Ok("no") | Ok("NO")
        ) {
            config.enabled = false;
        }

        if let Ok(level) = std::env::var("SPECTRA_LEVEL") {
            if let Some(parsed) = SpectraLevel::parse(&level) {
                config.min_level = parsed;
            }
        }

        if let Ok(rate) = std::env::var("SPECTRA_SAMPLE_RATE") {
            if let Ok(parsed) = rate.parse::<f64>() {
                config.global_sample_rate = parsed.clamp(0.0, 1.0);
            }
        }

        for (key, value) in std::env::vars() {
            if let Some(name) = key.strip_prefix("SPECTRA_SAMPLE_") {
                if name.is_empty() {
                    continue;
                }
                if let Ok(rate) = value.parse::<f64>() {
                    config
                        .per_name
                        .entry(name.to_ascii_lowercase())
                        .or_default()
                        .sample_rate = Some(rate.clamp(0.0, 1.0));
                }
            }
        }

        if let Ok(path) = std::env::var("SPECTRA_CONFIG") {
            if let Ok(contents) = std::fs::read_to_string(&path) {
                merge_toml(&mut config, &contents);
            }
        }

        config
    }
}

fn merge_toml(config: &mut SpectraConfig, contents: &str) {
    let parsed: toml::Value = match contents.parse() {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "failed to parse SPECTRA_CONFIG");
            return;
        }
    };

    if let Some(spectra) = parsed.get("spectra").and_then(|v| v.as_table()) {
        if let Some(level) = spectra.get("level").and_then(|v| v.as_str()) {
            if let Some(parsed) = SpectraLevel::parse(level) {
                config.min_level = parsed;
            }
        }
        if let Some(rate) = spectra.get("sample_rate").and_then(|v| v.as_float()) {
            config.global_sample_rate = rate.clamp(0.0, 1.0);
        }
        if let Some(enabled) = spectra.get("enabled").and_then(|v| v.as_bool()) {
            config.enabled = enabled;
        }
        if let Some(overrides) = spectra.get("overrides").and_then(|v| v.as_table()) {
            for (name, table) in overrides {
                let Some(table) = table.as_table() else {
                    continue;
                };
                let entry = config.per_name.entry(name.clone()).or_default();
                if let Some(level) = table.get("level").and_then(|v| v.as_str()) {
                    if let Some(parsed) = SpectraLevel::parse(level) {
                        entry.level = Some(parsed);
                    }
                }
                if let Some(rate) = table.get("sample_rate").and_then(|v| v.as_float()) {
                    entry.sample_rate = Some(rate.clamp(0.0, 1.0));
                }
                if let Some(ms) = table.get("coalesce_ms").and_then(|v| v.as_integer()) {
                    entry.coalesce_ms = Some(Some(ms.max(0) as u64));
                }
            }
        }
    }
}

/// Resolved emit policy for a single metric/event name (schema defaults + runtime overrides).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EmitPolicy {
    /// Resolved verbosity tier for this schema name.
    pub level: SpectraLevel,
    /// Resolved sampling rate (`0.0`–`1.0`).
    pub sample_rate: f64,
    /// Resolved gauge coalesce window in milliseconds, if any.
    pub coalesce_ms: Option<u64>,
}

impl Default for EmitPolicy {
    fn default() -> Self {
        Self {
            level: SpectraLevel::Info,
            sample_rate: 1.0,
            coalesce_ms: None,
        }
    }
}

/// Installed gate state (global min level + resolved per-name policies).
#[derive(Debug, Clone)]
pub struct GateState {
    pub min_level: SpectraLevel,
    pub global_sample_rate: f64,
    pub enabled: bool,
    pub default_policy: EmitPolicy,
}

struct InstalledConfig {
    gate: GateState,
    resolved: HashMap<String, EmitPolicy>,
}

static INSTALLED: OnceLock<RwLock<Option<InstalledConfig>>> = OnceLock::new();

fn slot() -> &'static RwLock<Option<InstalledConfig>> {
    INSTALLED.get_or_init(|| RwLock::new(None))
}

fn resolve_policy(metadata: &SchemaMetadata, override_cfg: Option<&NameOverride>) -> EmitPolicy {
    let mut policy = EmitPolicy {
        level: metadata.default_level,
        sample_rate: metadata.default_sample_rate,
        coalesce_ms: metadata.gauge_coalesce_ms,
    };

    if let Some(o) = override_cfg {
        if let Some(level) = o.level {
            policy.level = level;
        }
        if let Some(rate) = o.sample_rate {
            policy.sample_rate = rate;
        }
        if let Some(ms) = o.coalesce_ms {
            policy.coalesce_ms = ms;
        }
    }

    policy.sample_rate = policy.sample_rate.clamp(0.0, 1.0);
    policy
}

fn build_resolved(config: &SpectraConfig) -> HashMap<String, EmitPolicy> {
    let registry = SchemaRegistry::global();
    let mut resolved = HashMap::new();

    for name in registry.list_schemas() {
        let Some(metadata) = registry.get_schema(name) else {
            continue;
        };
        let override_cfg = config.per_name.get(name);
        resolved.insert(name.to_string(), resolve_policy(metadata, override_cfg));
    }

    for (name, override_cfg) in &config.per_name {
        resolved
            .entry(name.clone())
            .and_modify(|p| {
                if let Some(level) = override_cfg.level {
                    p.level = level;
                }
                if let Some(rate) = override_cfg.sample_rate {
                    p.sample_rate = rate.clamp(0.0, 1.0);
                }
                if let Some(ms) = override_cfg.coalesce_ms {
                    p.coalesce_ms = ms;
                }
            })
            .or_insert_with(|| {
                resolve_policy(
                    &SchemaMetadata {
                        table_or_metric: name.clone(),
                        ..SchemaMetadata::default()
                    },
                    Some(override_cfg),
                )
            });
    }

    resolved
}

/// Install process-wide gate configuration (call once at host boot, before sink install).
pub fn install_config(config: SpectraConfig) {
    let gate = GateState {
        min_level: config.min_level,
        global_sample_rate: config.global_sample_rate.clamp(0.0, 1.0),
        enabled: config.enabled,
        default_policy: EmitPolicy::default(),
    };
    let resolved = build_resolved(&config);
    *slot().write() = Some(InstalledConfig { gate, resolved });
    tracing::info!(
        enabled = config.enabled,
        min_level = ?config.min_level,
        global_sample_rate = config.global_sample_rate,
        "emit gate installed"
    );
}

pub(crate) fn gate_state() -> Option<GateState> {
    slot().read().as_ref().map(|c| c.gate.clone())
}

pub(crate) fn policy_for(name: &str) -> EmitPolicy {
    slot()
        .read()
        .as_ref()
        .map(|c| {
            c.resolved
                .get(name)
                .copied()
                .unwrap_or(c.gate.default_policy)
        })
        .unwrap_or_default()
}

#[cfg(test)]
pub fn reset_config_for_test() {
    *slot().write() = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_spectra_level() {
        assert_eq!(SpectraLevel::parse("info"), Some(SpectraLevel::Info));
        assert_eq!(SpectraLevel::parse("TRACE"), Some(SpectraLevel::Trace));
        assert_eq!(SpectraLevel::parse("bogus"), None);
    }

    #[test]
    fn merge_toml_overrides() {
        let mut config = SpectraConfig::default();
        let toml = r#"
[spectra]
level = "debug"
sample_rate = 0.5

[spectra.overrides.example_db_reads]
sample_rate = 0.01
"#;
        merge_toml(&mut config, toml);
        assert_eq!(config.min_level, SpectraLevel::Debug);
        assert!((config.global_sample_rate - 0.5).abs() < f64::EPSILON);
        assert!(
            (config.per_name["example_db_reads"].sample_rate.unwrap() - 0.01).abs() < f64::EPSILON
        );
    }
}
