//! App-owned storage roots. The dev renderer installs its sandbox before any
//! app state exists; a debug build on macOS or Windows started with
//! `MDOC_DEV_PROFILE=<dir>` keeps its whole profile in that folder; other
//! builds use the platform directories.
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

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

/// The variable that moves a debug build's profile into a folder.
pub const DEV_PROFILE_VAR: &str = "MDOC_DEV_PROFILE";

/// Whether this build honours [`DEV_PROFILE_VAR`]: debug builds on macOS and
/// Windows, never a release build.
pub const DEV_PROFILE_HONOURED: bool = cfg!(all(
    debug_assertions,
    any(target_os = "macos", target_os = "windows")
));

/// The dev profile folder `value` names, for a build that honours it; an
/// empty value names none. A relative folder is taken from `cwd`.
fn dev_profile_from(value: Option<OsString>, honoured: bool, cwd: &Path) -> Option<PathBuf> {
    if !honoured {
        return None;
    }
    let value = value.filter(|value| !value.is_empty())?;
    Some(cwd.join(value))
}

/// This process's dev profile folder, read once. Settings, the session, the
/// Vault and its key file, models and the update cache all live in it, and
/// the OS key store is never asked.
pub fn dev_profile() -> Option<&'static Path> {
    static PROFILE: std::sync::LazyLock<Option<PathBuf>> = std::sync::LazyLock::new(|| {
        if !DEV_PROFILE_HONOURED {
            return None;
        }
        let cwd = std::env::current_dir().unwrap_or_default();
        dev_profile_from(std::env::var_os(DEV_PROFILE_VAR), true, &cwd)
    });
    PROFILE.as_deref()
}

pub fn data_local_dir() -> Option<PathBuf> {
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    if let Some(path) = SANDBOX.get() {
        return Some(path.join("data"));
    }
    if let Some(path) = dev_profile() {
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
    if let Some(path) = dev_profile() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_release_build_ignores_the_dev_profile_variable() {
        let cwd = Path::new("/work");
        let named = || Some(OsString::from("/scratch/profile"));
        assert_eq!(dev_profile_from(named(), false, cwd), None);
        assert_eq!(
            dev_profile_from(named(), true, cwd),
            Some(PathBuf::from("/scratch/profile"))
        );
        // This build's own answer: only a debug build on macOS or Windows.
        assert_eq!(
            DEV_PROFILE_HONOURED,
            cfg!(debug_assertions) && cfg!(any(target_os = "macos", target_os = "windows"))
        );
        #[cfg(not(debug_assertions))]
        assert!(!DEV_PROFILE_HONOURED && dev_profile().is_none());
    }

    #[test]
    fn an_empty_dev_profile_names_no_folder_and_a_relative_one_is_absolute() {
        let cwd = Path::new("/work");
        assert_eq!(dev_profile_from(None, true, cwd), None);
        assert_eq!(dev_profile_from(Some("".into()), true, cwd), None);
        assert_eq!(
            dev_profile_from(Some("scratch".into()), true, cwd),
            Some(PathBuf::from("/work/scratch"))
        );
    }
}
