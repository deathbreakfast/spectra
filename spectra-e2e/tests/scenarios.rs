//! Matrix correctness scenarios (CI default slice + optional remote rows).

spectra_testkit::matrix_scenario_suite!();

/// Remote-ingest catalog (tensorbase/clickhouse). Ignored unless URL env is set.
mod remote_ingest {
    spectra_testkit::matrix_remote_scenario_suite!();
}
