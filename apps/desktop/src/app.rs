//! The app's process-level parts: crash reports, one mdoc per session, a
//! quit from outside mdoc, updates, the packaged self-test, the system's
//! file pickers and the downloadable models' list.
pub(crate) mod crash;
pub(crate) mod models;
pub(crate) mod native_picker;
pub(crate) mod self_test;
pub(crate) mod single_instance;
#[cfg(any(target_os = "macos", test))]
pub(crate) mod system_quit;
pub(crate) mod updater;
