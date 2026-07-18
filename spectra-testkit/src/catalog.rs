//! Shared happy/sad correctness catalog for matrix E2E expansion.

use crate::fixtures::remote_env_ready;
use crate::matrix::{
    ci_embedded_rows, ci_recording_rows, ci_telemetry_rows, remote_ingest_rows, MatrixSpec,
};
use crate::runner::{DriverKind, ScenarioRunner};
use crate::scenario::ScenarioSpec;

/// Happy vs sad path label for catalog entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathKind {
    /// Expected success / policy-compliant behavior.
    Happy,
    /// Expected rejection, failure, or empty-query behavior.
    Sad,
}

/// One row in the shared correctness catalog.
#[derive(Debug, Clone, Copy)]
pub struct CatalogEntry {
    /// Stable id (matches scenario spec id prefix).
    pub id: &'static str,
    /// Happy or sad path.
    pub path: PathKind,
    /// Scenario factory.
    pub spec: fn() -> ScenarioSpec,
    /// Matrix rows to expand for this scenario.
    pub matrix_rows: fn() -> Vec<MatrixSpec>,
}

/// All catalog entries for CI matrix expansion.
pub fn catalog_entries() -> &'static [CatalogEntry] {
    &[
        CatalogEntry {
            id: "platform-smoke-roundtrip",
            path: PathKind::Happy,
            spec: ScenarioSpec::platform_smoke_roundtrip,
            matrix_rows: ci_embedded_rows,
        },
        CatalogEntry {
            id: "transport-dual-path",
            path: PathKind::Happy,
            spec: ScenarioSpec::transport_dual_path,
            matrix_rows: ci_recording_rows,
        },
        CatalogEntry {
            id: "gate-drops-debug",
            path: PathKind::Sad,
            spec: ScenarioSpec::gate_drops_debug,
            matrix_rows: ci_embedded_rows,
        },
        CatalogEntry {
            id: "transport-only-no-storage",
            path: PathKind::Sad,
            spec: ScenarioSpec::transport_only_no_storage,
            matrix_rows: transport_only_rows,
        },
        CatalogEntry {
            id: "label-filter-hit",
            path: PathKind::Happy,
            spec: ScenarioSpec::label_filter_hit,
            matrix_rows: ci_embedded_rows,
        },
        CatalogEntry {
            id: "label-filter-miss",
            path: PathKind::Sad,
            spec: ScenarioSpec::label_filter_miss,
            matrix_rows: ci_embedded_rows,
        },
        CatalogEntry {
            id: "gauge-roundtrip",
            path: PathKind::Happy,
            spec: ScenarioSpec::gauge_roundtrip,
            matrix_rows: ci_embedded_rows,
        },
        CatalogEntry {
            id: "query-time-range-empty",
            path: PathKind::Sad,
            spec: ScenarioSpec::query_time_range_empty,
            matrix_rows: ci_embedded_rows,
        },
        CatalogEntry {
            id: "telemetry-console-ndjson",
            path: PathKind::Happy,
            spec: ScenarioSpec::telemetry_console_ndjson,
            matrix_rows: ci_telemetry_rows,
        },
    ]
}

/// Remote-ingest catalog: applicable correctness scenarios × tensorbase/clickhouse.
///
/// Transport/recording and console-NDJSON scenarios are excluded (Direct/Off only).
pub fn remote_catalog_entries() -> &'static [CatalogEntry] {
    &[
        CatalogEntry {
            id: "platform-smoke-roundtrip",
            path: PathKind::Happy,
            spec: ScenarioSpec::platform_smoke_roundtrip,
            matrix_rows: remote_ingest_rows,
        },
        CatalogEntry {
            id: "gate-drops-debug",
            path: PathKind::Sad,
            spec: ScenarioSpec::gate_drops_debug,
            matrix_rows: remote_ingest_rows,
        },
        CatalogEntry {
            id: "label-filter-hit",
            path: PathKind::Happy,
            spec: ScenarioSpec::label_filter_hit,
            matrix_rows: remote_ingest_rows,
        },
        CatalogEntry {
            id: "label-filter-miss",
            path: PathKind::Sad,
            spec: ScenarioSpec::label_filter_miss,
            matrix_rows: remote_ingest_rows,
        },
        CatalogEntry {
            id: "gauge-roundtrip",
            path: PathKind::Happy,
            spec: ScenarioSpec::gauge_roundtrip,
            matrix_rows: remote_ingest_rows,
        },
        CatalogEntry {
            id: "query-time-range-empty",
            path: PathKind::Sad,
            spec: ScenarioSpec::query_time_range_empty,
            matrix_rows: remote_ingest_rows,
        },
    ]
}

fn transport_only_rows() -> Vec<MatrixSpec> {
    ci_recording_rows()
        .into_iter()
        .map(|mut row| {
            row.persist_enabled = false;
            row
        })
        .collect()
}

/// Run one catalog scenario across its configured matrix rows.
pub async fn run_catalog_scenario(entry: CatalogEntry) {
    for matrix in (entry.matrix_rows)() {
        run_catalog_scenario_on_matrix(entry, matrix).await;
    }
}

/// Run one catalog scenario on a single matrix row.
pub async fn run_catalog_scenario_on_matrix(entry: CatalogEntry, matrix: MatrixSpec) {
    let spec = (entry.spec)();
    let result = ScenarioRunner::run(matrix.clone(), &spec, DriverKind::Correctness)
        .await
        .expect("runner");
    assert!(
        result.error.is_none(),
        "scenario {} matrix {} failed: {:?}",
        result.scenario_id,
        result.matrix_slug,
        result.error
    );
}

/// Run one remote catalog scenario, soft-skipping rows without URL env.
pub async fn run_remote_catalog_scenario(entry: CatalogEntry) {
    for matrix in (entry.matrix_rows)() {
        if !remote_env_ready(matrix.storage) {
            continue;
        }
        run_catalog_scenario_on_matrix(entry, matrix).await;
    }
}

/// Forwards the canonical scenario id list to `$m` (used by matrix suite macros).
#[macro_export]
macro_rules! invoke_catalog_scenario_ids {
    ($m:path) => {
        $m!(
            platform_smoke_roundtrip,
            transport_dual_path,
            gate_drops_debug,
            transport_only_no_storage,
            label_filter_hit,
            label_filter_miss,
            gauge_roundtrip,
            query_time_range_empty,
            telemetry_console_ndjson,
        );
    };
}

/// Forwards remote-applicable scenario ids to `$m`.
#[macro_export]
macro_rules! invoke_remote_catalog_scenario_ids {
    ($m:path) => {
        $m!(
            platform_smoke_roundtrip,
            gate_drops_debug,
            label_filter_hit,
            label_filter_miss,
            gauge_roundtrip,
            query_time_range_empty,
        );
    };
}

/// Expand catalog scenarios across [`ci_embedded_rows`] as individual tokio tests.
#[macro_export]
macro_rules! matrix_ci_scenario_suite {
    ($($id:ident),* $(,)?) => {
        $(
            #[tokio::test]
            async fn $id() {
                $crate::run_catalog_scenario($crate::catalog_entries()
                    .iter()
                    .copied()
                    .find(|e| e.id == stringify!($id).replace('_', "-"))
                    .unwrap_or_else(|| panic!("unknown catalog id: {}", stringify!($id))))
                .await;
            }
        )*
    };
}

/// Expand remote catalog scenarios as ignored tokio tests (URL env required).
///
/// Place the expansion in a submodule (for example `mod remote_ingest { ... }`) so
/// test names do not collide with the default CI suite.
#[macro_export]
macro_rules! matrix_remote_ci_scenario_suite {
    ($($id:ident),* $(,)?) => {
        $(
            #[tokio::test]
            #[ignore = "requires SPECTRA_TENSORBASE_URL / SPECTRA_CLICKHOUSE_URL"]
            async fn $id() {
                $crate::run_remote_catalog_scenario($crate::remote_catalog_entries()
                    .iter()
                    .copied()
                    .find(|e| e.id == stringify!($id).replace('_', "-"))
                    .unwrap_or_else(|| panic!("unknown remote catalog id: {}", stringify!($id))))
                .await;
            }
        )*
    };
}

/// Default CI catalog suite (see [`invoke_catalog_scenario_ids!`]).
#[macro_export]
macro_rules! matrix_scenario_suite {
    () => {
        $crate::invoke_catalog_scenario_ids!($crate::matrix_ci_scenario_suite);
    };
    ($($id:ident),* $(,)?) => {
        $crate::matrix_ci_scenario_suite!($($id),*);
    };
}

/// Ignored remote-ingest catalog suite (see [`invoke_remote_catalog_scenario_ids!`]).
#[macro_export]
macro_rules! matrix_remote_scenario_suite {
    () => {
        $crate::invoke_remote_catalog_scenario_ids!($crate::matrix_remote_ci_scenario_suite);
    };
    ($($id:ident),* $(,)?) => {
        $crate::matrix_remote_ci_scenario_suite!($($id),*);
    };
}
