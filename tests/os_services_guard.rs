//! The guard on the OS's security and identity services (ADR 0095,
//! development.md "Tests and OS services"): in a test build, asking for the
//! build's signature or the case key panics, so such a test fails at once
//! instead of waiting on macOS for seconds per process (#510).

#[test]
#[should_panic(expected = "a test asked the OS for the build identity")]
fn a_test_that_asks_for_the_build_identity_fails() {
    mdoc_store::cases::build_identity();
}

#[test]
#[should_panic(expected = "a test asked the OS for the case key")]
fn a_test_that_opens_the_real_case_store_fails() {
    let dir = tempfile::tempdir().expect("a temporary folder");
    let _ = mdoc_store::cases::Store::open(dir.path().to_path_buf());
}
