//! App-owned storage roots. The dev renderer installs its sandbox before any
//! app state exists; normal and release builds use the platform directories.
use std::path::PathBuf;

#[cfg(all(
    debug_assertions,
    feature = "headless-renders",
    any(target_os = "macos", target_os = "windows")
))]
static SANDBOX: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

#[cfg(all(
    debug_assertions,
    feature = "headless-renders",
    any(target_os = "macos", target_os = "windows")
))]
pub(crate) fn isolate(path: PathBuf) -> Result<(), String> {
    SANDBOX
        .set(path)
        .map_err(|_| "profile already initialized".into())
}

pub(crate) fn data_local_dir() -> Option<PathBuf> {
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    if let Some(path) = SANDBOX.get() {
        return Some(path.join("data"));
    }
    dirs::data_local_dir()
}

pub(crate) fn cache_dir() -> Option<PathBuf> {
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    if let Some(path) = SANDBOX.get() {
        return Some(path.join("cache"));
    }
    dirs::cache_dir()
}

pub(crate) fn isolated() -> bool {
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    {
        SANDBOX.get().is_some()
    }
    #[cfg(not(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    )))]
    {
        false
    }
}
