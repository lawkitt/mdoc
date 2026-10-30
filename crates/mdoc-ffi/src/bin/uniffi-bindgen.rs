//! Generates the Swift bindings from the built library (scripts/build-ios-core.sh).
fn main() {
    uniffi::uniffi_bindgen_main()
}
