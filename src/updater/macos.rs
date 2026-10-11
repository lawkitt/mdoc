//! macOS install steps (ADR 0040): who may update, staging a release's
//! `.app` out of its disk image, the Developer ID gates, and the quit-time
//! swap. Stage-and-rename with rollback follows gpui-updater; the gates follow
//! OxiMux. Everything here is blocking and runs off the UI thread, except the
//! swap, which runs while mdoc quits.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use super::Ineligible;

/// The designated requirement every mdoc bundle must satisfy: an Apple-rooted
/// Developer ID Application certificate for this team, and our bundle id.
const REQUIREMENT: &str = concat!(
    "=anchor apple generic",
    " and identifier \"io.github.lawkitt.mdoc\"",
    // Developer ID Application leaf certificate.
    " and certificate leaf[field.1.2.840.113635.100.6.1.13]",
    " and certificate leaf[subject.OU] = \"F33DLM8K53\"",
);

/// The `.app` bundle enclosing `exe`.
pub fn bundle_root(exe: &Path) -> Option<PathBuf> {
    exe.ancestors()
        .find(|p| p.extension().is_some_and(|e| e == "app"))
        .map(Path::to_path_buf)
}

/// Whether the running copy at `exe` may replace itself. Runs `codesign` once
/// and probes the parent directory for writes; call off the UI thread.
pub fn eligibility(exe: &Path) -> Result<PathBuf, Ineligible> {
    let bundle = bundle_root(exe).ok_or(Ineligible::NotABundle)?;
    // Gatekeeper runs quarantined apps that were never moved from a read-only
    // randomized mirror; replacing that mirror would change nothing.
    if bundle.to_string_lossy().contains("/AppTranslocation/") {
        return Err(Ineligible::Translocated);
    }
    let parent = bundle.parent().ok_or(Ineligible::NotWritable)?;
    if tempfile::Builder::new()
        .prefix(".mdoc-update-probe")
        .tempfile_in(parent)
        .is_err()
    {
        return Err(if bundle.starts_with("/Volumes") {
            Ineligible::DiskImage
        } else {
            Ineligible::NotWritable
        });
    }
    // Pin updates to the publisher of the running copy: an ad-hoc or foreign
    // signature has no identity worth pinning to.
    codesign_requirement(&bundle, false).map_err(|_| Ineligible::NotSigned)?;
    Ok(bundle)
}

/// Copy `mdoc.app` out of `dmg` to `dest`, replacing anything there.
pub fn stage(dmg: &Path, dest: &Path) -> Result<(), String> {
    let mount = Mount::attach(dmg)?;
    let app = mount.dir.path().join("mdoc.app");
    if !app.is_dir() {
        return Err("The downloaded disk image does not contain mdoc.app.".into());
    }
    remove(dest);
    ditto(&app, dest)
}

/// The gates a bundle must pass before it may replace the installed app:
/// code signature with our team and bundle id, Gatekeeper (the only check
/// that knows about revoked certificates and notarization) and the version
/// the release claims. The feed's tag is a claim; the bundle is the fact.
pub fn verify(app: &Path, version: &str, gatekeeper: bool) -> Result<(), String> {
    codesign_requirement(app, true)
        .map_err(|e| format!("The update is not signed by the mdoc developer: {e}"))?;
    if gatekeeper {
        run(Command::new("/usr/sbin/spctl")
            .args(["--assess", "--type", "execute"])
            .arg(app))
        .map_err(|e| format!("macOS Gatekeeper rejected the update: {e}"))?;
    }
    let found = bundle_version(app)?;
    if found != version {
        return Err(format!(
            "The update contains mdoc {found}, not the {version} the release promised."
        ));
    }
    Ok(())
}

/// Replace `installed` with `staged`, which must contain `version`. The copy is
/// made beside the installed bundle so the swap is two renames on one volume,
/// verified again there, and the old bundle is restored on any failure.
pub fn swap(staged: &Path, installed: &Path, version: &str) -> Result<(), String> {
    swap_with(staged, installed, version, true)
}

fn swap_with(
    staged: &Path,
    installed: &Path,
    version: &str,
    gatekeeper: bool,
) -> Result<(), String> {
    let parent = installed
        .parent()
        .ok_or("The installed app has no parent folder.")?;
    let name = installed
        .file_name()
        .ok_or("The installed app has no name.")?
        .to_string_lossy();
    let incoming = parent.join(format!(".{name}.update"));
    let backup = parent.join(format!(".{name}.old"));
    remove(&incoming);
    remove(&backup);
    let result = ditto(staged, &incoming)
        .and_then(|()| verify(&incoming, version, gatekeeper))
        .and_then(|()| {
            fs::rename(installed, &backup)
                .map_err(|e| format!("Couldn't move the current app aside: {e}"))
        })
        .and_then(|()| {
            fs::rename(&incoming, installed).map_err(|e| {
                // Never leave the app missing.
                let _ = fs::rename(&backup, installed);
                format!("Couldn't move the new app into place: {e}")
            })
        });
    remove(&incoming);
    remove(&backup);
    result
}

fn codesign_requirement(app: &Path, deep: bool) -> Result<(), String> {
    let mut command = Command::new("/usr/bin/codesign");
    command.args(["--verify", "--strict"]);
    if deep {
        command.arg("--deep");
    }
    run(command.arg(format!("-R{REQUIREMENT}")).arg(app))
}

fn bundle_version(app: &Path) -> Result<String, String> {
    let output = Command::new("/usr/bin/plutil")
        .args(["-extract", "CFBundleShortVersionString", "raw", "-o", "-"])
        .arg(app.join("Contents/Info.plist"))
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err("The update has no readable version.".into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn ditto(from: &Path, to: &Path) -> Result<(), String> {
    run(Command::new("/usr/bin/ditto").arg(from).arg(to))
        .map_err(|e| format!("Couldn't copy the update: {e}"))
}

fn remove(path: &Path) {
    let _ = fs::remove_dir_all(path);
}

/// Run a tool; its stderr becomes the error.
fn run(command: &mut Command) -> Result<(), String> {
    let output = command.output().map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

/// A read-only disk image mount, detached on drop.
struct Mount {
    dir: tempfile::TempDir,
}

impl Mount {
    fn attach(dmg: &Path) -> Result<Self, String> {
        let dir = tempfile::Builder::new()
            .prefix("mdoc-update-mount")
            .tempdir()
            .map_err(|e| e.to_string())?;
        run(Command::new("/usr/bin/hdiutil")
            .args([
                "attach",
                "-nobrowse",
                "-readonly",
                "-noautoopen",
                "-mountpoint",
            ])
            .arg(dir.path())
            .arg(dmg))
        .map_err(|e| format!("Couldn't open the downloaded disk image: {e}"))?;
        Ok(Self { dir })
    }
}

impl Drop for Mount {
    fn drop(&mut self) {
        let _ = Command::new("/usr/bin/hdiutil")
            .args(["detach", "-force"])
            .arg(self.dir.path())
            .output();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_enclosing_bundle() {
        assert_eq!(
            bundle_root(Path::new("/Applications/mdoc.app/Contents/MacOS/mdoc")),
            Some(PathBuf::from("/Applications/mdoc.app"))
        );
        assert_eq!(bundle_root(Path::new("/usr/local/bin/mdoc")), None);
    }

    #[test]
    fn development_and_translocated_copies_are_ineligible() {
        assert_eq!(
            eligibility(Path::new("/tmp/target/debug/mdoc")),
            Err(Ineligible::NotABundle)
        );
        assert_eq!(
            eligibility(Path::new(
                "/private/var/folders/x/AppTranslocation/ABC/d/mdoc.app/Contents/MacOS/mdoc"
            )),
            Err(Ineligible::Translocated)
        );
    }

    #[test]
    fn unwritable_and_unsigned_copies_are_ineligible() {
        let root = tempfile::tempdir().unwrap();
        let exe = root.path().join("mdoc.app/Contents/MacOS/mdoc");
        fs::create_dir_all(exe.parent().unwrap()).unwrap();
        fs::write(&exe, "").unwrap();
        assert_eq!(eligibility(&exe), Err(Ineligible::NotSigned));
        assert_eq!(
            eligibility(Path::new("/System/mdoc.app/Contents/MacOS/mdoc")),
            Err(Ineligible::NotWritable)
        );
    }

    #[test]
    fn a_missing_staged_copy_leaves_the_installed_app_alone() {
        let root = tempfile::tempdir().unwrap();
        let installed = root.path().join("mdoc.app");
        fs::create_dir_all(installed.join("Contents")).unwrap();
        fs::write(installed.join("Contents/marker"), "old").unwrap();
        assert!(swap(&root.path().join("missing.app"), &installed, "9.0.0").is_err());
        assert_eq!(
            fs::read_to_string(installed.join("Contents/marker")).unwrap(),
            "old"
        );
        assert!(!root.path().join(".mdoc.app.update").exists());
        assert!(!root.path().join(".mdoc.app.old").exists());
    }

    /// The real gates on a published release (ADR 0040): stage the app out of
    /// `MDOC_UPDATE_DMG` (e.g. `gh release download v0.1.0-beta.3 -p '*.dmg'`),
    /// verify it as version `MDOC_UPDATE_VERSION`, check that an installed
    /// copy is eligible, and swap the staged copy over it — Gatekeeper included.
    #[test]
    #[ignore = "needs a published .dmg; set MDOC_UPDATE_DMG and MDOC_UPDATE_VERSION"]
    fn a_published_release_stages_verifies_and_swaps() {
        let dmg = PathBuf::from(std::env::var("MDOC_UPDATE_DMG").unwrap());
        let version = std::env::var("MDOC_UPDATE_VERSION").unwrap();
        let root = tempfile::tempdir().unwrap();
        let staged = root.path().join("staged/mdoc.app");
        fs::create_dir_all(staged.parent().unwrap()).unwrap();
        stage(&dmg, &staged).unwrap();
        verify(&staged, &version, true).unwrap();
        assert!(
            verify(&staged, "0.0.1", true).is_err(),
            "version must match"
        );

        let applications = root.path().join("Applications");
        fs::create_dir_all(&applications).unwrap();
        let installed = applications.join("mdoc.app");
        ditto(&staged, &installed).unwrap();
        assert_eq!(
            eligibility(&installed.join("Contents/MacOS/mdoc")),
            Ok(installed.clone())
        );
        fs::write(installed.join("Contents/stale-marker"), "old").unwrap();
        swap(&staged, &installed, &version).unwrap();
        assert!(
            !installed.join("Contents/stale-marker").exists(),
            "replaced"
        );
        verify(&installed, &version, true).unwrap();
        assert!(!applications.join(".mdoc.app.update").exists());
        assert!(!applications.join(".mdoc.app.old").exists());
    }

    /// End to end with real signatures (ADR 0040): needs a Developer ID
    /// Application identity for team F33DLM8K53 in the keychain, named by
    /// `APPLE_SIGNING_IDENTITY`. Gatekeeper is skipped: fixtures aren't notarized.
    #[test]
    #[ignore = "needs the Developer ID identity; run with `cargo test -- --ignored`"]
    fn signed_fixtures_swap_and_foreign_or_tampered_bundles_do_not() {
        let identity = std::env::var("APPLE_SIGNING_IDENTITY")
            .expect("set APPLE_SIGNING_IDENTITY to the Developer ID Application identity");
        let root = tempfile::tempdir().unwrap();
        let fixture = |name: &str, version: &str, id: &str, sign: Option<&str>| {
            let app = root.path().join(name).join("mdoc.app");
            fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
            fs::copy("/usr/bin/true", app.join("Contents/MacOS/mdoc")).unwrap();
            fs::write(
                app.join("Contents/Info.plist"),
                format!(
                    r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>mdoc</string>
<key>CFBundleIdentifier</key><string>{id}</string>
<key>CFBundleShortVersionString</key><string>{version}</string>
</dict></plist>"#
                ),
            )
            .unwrap();
            run(Command::new("/usr/bin/codesign")
                .args(["--force", "--timestamp=none", "--sign", sign.unwrap_or("-")])
                .arg(&app))
            .unwrap();
            app
        };
        let installed = fixture(
            "Applications",
            "1.0.0",
            "io.github.lawkitt.mdoc",
            Some(&identity),
        );
        let marker = |app: &Path| bundle_version(app).unwrap();

        // A properly signed newer bundle passes and swaps in.
        let good = fixture("good", "2.0.0", "io.github.lawkitt.mdoc", Some(&identity));
        verify(&good, "2.0.0", false).unwrap();
        assert!(verify(&good, "3.0.0", false).is_err(), "version must match");
        swap_with(&good, &installed, "2.0.0", false).unwrap();
        assert_eq!(marker(&installed), "2.0.0");

        // Ad-hoc signed and wrong-bundle-id copies are refused.
        let adhoc = fixture("adhoc", "3.0.0", "io.github.lawkitt.mdoc", None);
        assert!(verify(&adhoc, "3.0.0", false).is_err());
        let foreign = fixture("foreign", "3.0.0", "com.example.mdoc", Some(&identity));
        assert!(verify(&foreign, "3.0.0", false).is_err());

        // Tampering after signing breaks the seal.
        let tampered = fixture(
            "tampered",
            "3.0.0",
            "io.github.lawkitt.mdoc",
            Some(&identity),
        );
        fs::write(tampered.join("Contents/MacOS/mdoc"), "evil").unwrap();
        assert!(verify(&tampered, "3.0.0", false).is_err());
        assert_eq!(
            marker(&installed),
            "2.0.0",
            "refused updates change nothing"
        );
    }
}
