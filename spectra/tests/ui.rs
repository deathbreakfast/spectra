//! Compile-pass / compile-fail UI tests for `spectra_schema!` and `spectra_metric!`.

#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/pass_metric.rs");
    t.pass("tests/ui/pass_schema.rs");
    t.compile_fail("tests/ui/fail_metric_missing_name.rs");
    t.compile_fail("tests/ui/fail_schema_missing_table.rs");
}
