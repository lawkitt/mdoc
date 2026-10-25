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
pub fn isolate(path: PathBuf) -> Result<(), AlreadyIsolated> {
    SANDBOX.set(path).map_err(|_| AlreadyIsolated)
}

#[cfg(all(
    debug_assertions,
    feature = "headless-renders",
    any(target_os = "macos", target_os = "windows")
))]
#[derive(Debug, thiserror::Error)]
#[error("profile already initialized")]
pub struct AlreadyIsolated;

pub fn data_local_dir() -> Option<PathBuf> {
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

pub fn cache_dir() -> Option<PathBuf> {
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

pub fn isolated() -> bool {
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
