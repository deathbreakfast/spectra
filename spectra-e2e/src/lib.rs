//! Correctness integration tests over the Spectra verification matrix.
//!
//! **Internal** — library surface exists for shared test harness wiring; downstream users run
//! `cargo test -p spectra-e2e`, not this crate as a dependency.

#![allow(clippy::expect_used, clippy::unwrap_used)]
//!
//! Test-only layer above `spectra_testkit` and `spectra`. Scenarios and matrix presets live in
//! testkit; this crate hosts integration test binaries and matrix drivers.
//!
//! - [`testkit`] — re-export of `spectra_testkit` for integration test modules
//! - Run `cargo test -p spectra-e2e` for embedded CI rows (mem/sqlite)
//! - Remote matrix rows require `SPECTRA_TENSORBASE_URL` or `SPECTRA_CLICKHOUSE_URL` and are
//!   typically `#[ignore]` until URLs are set.

/// Shared matrix bootstrap, scenarios, and fixtures (`spectra_testkit`).
pub use spectra_testkit as testkit;
