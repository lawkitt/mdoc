//! Generates the Swift bindings from the built library (scripts/ios/build-ios-core.sh).
fn main() {
    uniffi::uniffi_bindgen_main()
}
