//! The guard on the OS's security and identity services (ADR 0095,
//! development.md "Tests and OS services"): in a test build, asking for the
//! build's signature or the Vault key panics, so such a test fails at once
//! instead of waiting on macOS for seconds per process (#510).

#[test]
#[should_panic(expected = "a test asked the OS for the build identity")]
fn a_test_that_asks_for_the_build_identity_fails() {
    mdoc_store::os_key::build_identity();
}

#[test]
#[should_panic(expected = "a test asked the OS for the Vault key")]
fn a_test_that_reads_the_vault_key_fails() {
    let _ = mdoc_store::os_key::vault_key(false);
}

#[test]
#[should_panic(expected = "a test asked the OS for the Vault key")]
fn a_test_that_reads_the_vault_key_quietly_fails() {
    let _ = mdoc_store::os_key::vault_key(true);
}
