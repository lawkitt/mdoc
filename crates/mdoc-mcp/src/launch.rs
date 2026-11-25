//! Starting mdoc when nothing listens (#1164 Decision 21): the mdoc the
//! shim ships in, never one found elsewhere. Windows: `mdoc.exe` beside
//! `mdoc-mcp.exe`. A release shim in a macOS bundle asks Launch Services
//! (`open -g`, so mdoc doesn't come to the front). Otherwise, as in a
//! build folder: the `mdoc` beside the shim. A debug build passes its dev
//! profile on; a release one removes the variable. mdoc takes
//! [`LAUNCH_FLAG`] as "started for the connector": nothing to open, and on
//! Windows a running mdoc is left alone rather than brought forward.
use crate::ipc::DEV_PROFILE_VAR;
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

/// What mdoc is started with.
pub const LAUNCH_FLAG: &str = "--mdoc-connector-launch";

const APP: &str = if cfg!(windows) { "mdoc.exe" } else { "mdoc" };

/// The command that starts the mdoc beside the shim at `shim`, with
/// `dev_profile`; `None` when there is none beside it.
pub fn command(shim: &Path, dev_profile: Option<&Path>) -> Option<Command> {
    let folder = shim.parent()?;
    let mut command = match bundle(folder) {
        Some(app) if !cfg!(debug_assertions) => {
            // A running mdoc is left alone: Launch Services would send it
            // a reopen, which shows its window.
            if running() {
                return None;
            }
            let mut open = Command::new("/usr/bin/open");
            open.arg("-g")
                .arg("-a")
                .arg(app)
                .args(["--args", LAUNCH_FLAG]);
            open
        }
        _ => {
            let app = folder.join(APP);
            if !app.is_file() {
                return None;
            }
            let mut app = Command::new(app);
            app.arg(LAUNCH_FLAG);
            app
        }
    };
    match dev_profile {
        Some(profile) => command.env(DEV_PROFILE_VAR, profile),
        None => command.env_remove(DEV_PROFILE_VAR),
    };
    // The shim's stdout is the MCP channel: mdoc writes nothing to it.
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    Some(command)
}

/// The `.app` holding `folder` when it is a bundle's `Contents/MacOS`.
fn bundle(folder: &Path) -> Option<PathBuf> {
    if !cfg!(target_os = "macos") || folder.file_name()? != "MacOS" {
        return None;
    }
    let contents = folder.parent()?;
    let app = contents.parent()?;
    (contents.file_name()? == "Contents" && app.extension()? == "app").then(|| app.to_path_buf())
}

/// Whether an mdoc of this user runs (macOS: `pgrep`).
fn running() -> bool {
    #[cfg(unix)]
    let uid = crate::ipc::euid().to_string();
    #[cfg(not(unix))]
    let uid = String::new();
    (Command::new("/usr/bin/pgrep").args(["-q", "-x", "-U", &uid, "mdoc"]))
        .stdin(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// Start `command` apart from the shim, so mdoc outlives it.
pub fn start(mut command: Command) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
        let apart = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP;
        // Out of the host's job when it allows, so closing the AI app
        // doesn't end mdoc; a job that refuses keeps it.
        command.creation_flags(apart | CREATE_BREAKAWAY_FROM_JOB);
        if command.spawn().is_ok() {
            return Ok(());
        }
        command.creation_flags(apart);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
        // Reaped when it ends (`open` at once), so no zombie is left.
        let mut child = command.spawn()?;
        std::thread::spawn(move || child.wait());
        Ok(())
    }
    #[cfg(not(unix))]
    command.spawn().map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(command: &Command) -> Option<Option<std::ffi::OsString>> {
        command
            .get_envs()
            .find(|(key, _)| *key == DEV_PROFILE_VAR)
            .map(|(_, value)| value.map(ToOwned::to_owned))
    }

    /// Done when: the shim starts the mdoc beside it, with the flag, and
    /// passes a dev profile on or removes the variable.
    #[test]
    fn the_mdoc_beside_the_shim_is_started_with_its_profile() {
        let folder = tempfile::tempdir().unwrap();
        let shim = folder.path().join("mdoc-mcp");
        assert!(command(&shim, None).is_none(), "no mdoc beside it");
        std::fs::write(folder.path().join(APP), b"").unwrap();
        let profile = folder.path().join("profile");
        let with = command(&shim, Some(&profile)).unwrap();
        assert_eq!(with.get_program(), folder.path().join(APP));
        assert_eq!(with.get_args().collect::<Vec<_>>(), [LAUNCH_FLAG]);
        assert_eq!(env(&with), Some(Some(profile.into_os_string())));
        let without = command(&shim, None).unwrap();
        assert_eq!(env(&without), Some(None), "a release shim removes it");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_bundle_is_found_from_its_executables_folder() {
        let folder = Path::new("/Applications/mdoc.app/Contents/MacOS");
        assert_eq!(
            bundle(folder),
            Some(PathBuf::from("/Applications/mdoc.app"))
        );
        assert_eq!(bundle(Path::new("/tmp/target/debug")), None);
    }
}
