//! The old cases folder stays untouched (#1091 Decision 2, ROADMAP item
//! 23). Before any app state exists the runner fills the sandbox profile
//! with what an earlier build left: `cases/` and `cases-dev/` with a case
//! file it sealed and stand-ins for others, the user-wide lists, a stand-in for the OS-sealed key file,
//! nested folders, and `recents.json` beside them. After every scene, in
//! both themes, and the cut-over pass, every entry must read as it did:
//! the same names, kinds, sizes, contents and modified times.
use super::Result;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

/// What an earlier build kept in `<data>/mdoc`, relative to it.
const ROOTS: [&str; 3] = ["cases", "cases-dev", "recents.json"];

/// The old files' modified time: well before any run.
fn old_time() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_735_776_000)
}

/// A case file an earlier build sealed (`tests/fixtures/old-cases`).
const OLD_CASE: &[u8] =
    include_bytes!("../../../tests/fixtures/old-cases/0123456789abcdef0123456789abcdef.case");

/// Write an earlier build's files into `mdoc`, the profile's app folder.
pub(super) fn fill(mdoc: &Path) -> Result<()> {
    let io = |e: std::io::Error| e.to_string();
    // Bytes that stand for a sealed file: never opened here.
    let sealed = |magic: &[u8], seed: u32| -> Vec<u8> {
        let tail = (0..300u32).map(|i| ((i * 37 + seed * 11) % 251) as u8);
        magic.iter().copied().chain(tail).collect()
    };
    for (folder, cases) in [("cases", 3), ("cases-dev", 1)] {
        let dir = mdoc.join(folder);
        std::fs::create_dir_all(&dir).map_err(io)?;
        std::fs::write(dir.join("0123456789abcdef0123456789abcdef.case"), OLD_CASE).map_err(io)?;
        for n in 1..cases {
            let name = format!("{:032x}.case", 0xfeed_0000_u32 + n);
            std::fs::write(dir.join(name), sealed(b"MDOCCASE1", n)).map_err(io)?;
        }
        std::fs::write(dir.join("user.lists"), sealed(b"MDOCLIST1", 9)).map_err(io)?;
        // The OS-sealed key file's stand-in: never read here, as the
        // sandbox refuses the OS key.
        std::fs::write(dir.join("key.dpapi"), sealed(b"", 7)).map_err(io)?;
        let nested = dir.join("archive").join("2025");
        std::fs::create_dir_all(&nested).map_err(io)?;
        std::fs::write(nested.join("notes.txt"), "Kept by an earlier build.").map_err(io)?;
        std::fs::create_dir_all(dir.join("archive").join("empty")).map_err(io)?;
    }
    std::fs::write(
        mdoc.join("recents.json"),
        r#"{"files":[{"path":"/old/Lease.md","opened":1735776000,"pseudonymized":true}]}"#,
    )
    .map_err(io)?;
    for root in ROOTS {
        for file in files(&mdoc.join(root)) {
            std::fs::File::options()
                .write(true)
                .open(&file)
                .and_then(|f| f.set_modified(old_time()))
                .map_err(|e| format!("{}: {e}", file.display()))?;
        }
    }
    Ok(())
}

/// Every file at or under `path`.
fn files(path: &Path) -> Vec<PathBuf> {
    if path.is_file() {
        return vec![path.to_path_buf()];
    }
    let mut found = Vec::new();
    for entry in std::fs::read_dir(path).into_iter().flatten().flatten() {
        found.extend(files(&entry.path()));
    }
    found
}

/// One entry as read: a folder has no length or digest of its own.
#[derive(Debug, PartialEq)]
struct Entry {
    folder: bool,
    len: u64,
    digest: Option<[u8; 32]>,
    modified: Option<SystemTime>,
}

/// Every entry under the old roots, by its path relative to `mdoc`.
#[derive(Debug, PartialEq)]
pub(super) struct Snapshot(BTreeMap<PathBuf, Entry>);

impl Snapshot {
    pub(super) fn take(mdoc: &Path) -> Result<Self> {
        let mut entries = BTreeMap::new();
        for root in ROOTS {
            walk(mdoc, &mdoc.join(root), &mut entries)?;
        }
        Ok(Self(entries))
    }

    /// What differs in `now`, one line per entry; empty when nothing does.
    pub(super) fn changes(&self, now: &Self) -> Vec<String> {
        let mut changes = Vec::new();
        for (path, before) in &self.0 {
            match now.0.get(path) {
                None => changes.push(format!("{} is gone", path.display())),
                Some(after) if after != before => {
                    let mut what = Vec::new();
                    if after.folder != before.folder {
                        what.push("kind");
                    }
                    if after.len != before.len {
                        what.push("size");
                    }
                    if after.digest != before.digest {
                        what.push("contents");
                    }
                    if after.modified != before.modified {
                        what.push("modified time");
                    }
                    changes.push(format!("{}: {} changed", path.display(), what.join(", ")));
                }
                Some(_) => {}
            }
        }
        for path in now.0.keys().filter(|path| !self.0.contains_key(*path)) {
            changes.push(format!("{} is new", path.display()));
        }
        changes
    }

    /// Fail, naming each change, unless `now` reads as this did.
    pub(super) fn check(&self, mdoc: &Path) -> Result<()> {
        let changes = self.changes(&Self::take(mdoc)?);
        if changes.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "the old cases folder was touched: {}",
                changes.join("; ")
            ))
        }
    }
}

fn walk(mdoc: &Path, path: &Path, entries: &mut BTreeMap<PathBuf, Entry>) -> Result<()> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("{}: {error}", path.display())),
    };
    let relative = path.strip_prefix(mdoc).map_err(|e| e.to_string())?;
    let folder = metadata.is_dir();
    let digest = if folder {
        None
    } else {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Some(Sha256::digest(&bytes).into())
    };
    entries.insert(
        relative.to_path_buf(),
        Entry {
            folder,
            len: if folder { 0 } else { metadata.len() },
            digest,
            modified: metadata.modified().ok(),
        },
    );
    if folder {
        let mut children: Vec<_> = std::fs::read_dir(path)
            .map_err(|e| format!("{}: {e}", path.display()))?
            .flatten()
            .map(|entry| entry.path())
            .collect();
        children.sort();
        for child in children {
            walk(mdoc, &child, entries)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A change made to the filled profile.
    type Touch = fn(&Path);

    fn filled() -> (tempfile::TempDir, PathBuf, Snapshot) {
        let dir = tempfile::tempdir().unwrap();
        let mdoc = dir.path().join("mdoc");
        fill(&mdoc).unwrap();
        let snapshot = Snapshot::take(&mdoc).unwrap();
        (dir, mdoc, snapshot)
    }

    #[test]
    fn an_old_profile_has_case_files_a_key_lists_and_nested_folders() {
        let (_dir, mdoc, snapshot) = filled();
        let names: Vec<String> = snapshot.0.keys().map(|p| p.display().to_string()).collect();
        let cases = mdoc.join("cases");
        let sealed = std::fs::read_dir(&cases)
            .unwrap()
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "case"))
            .count();
        assert_eq!(sealed, 3, "{names:?}");
        for path in [
            "cases/key.dpapi",
            "cases/user.lists",
            "cases/archive/2025/notes.txt",
            "cases/archive/empty",
            "cases-dev/key.dpapi",
            "recents.json",
        ] {
            assert!(
                snapshot.0.contains_key(&PathBuf::from(path)),
                "{path} in {names:?}"
            );
        }
        assert_eq!(
            std::fs::read(cases.join("0123456789abcdef0123456789abcdef.case")).unwrap(),
            OLD_CASE
        );
        assert!(snapshot.check(&mdoc).is_ok());
    }

    #[test]
    fn every_kind_of_touch_is_named() {
        let touches: [(&str, Touch); 6] = [
            ("cases/key.dpapi: size, contents changed", |m| {
                std::fs::write(m.join("cases/key.dpapi"), b"new key").unwrap();
                set_old(&m.join("cases/key.dpapi"));
            }),
            ("cases/user.lists: contents changed", |m| {
                let path = m.join("cases/user.lists");
                let mut bytes = std::fs::read(&path).unwrap();
                bytes[0] ^= 1;
                std::fs::write(&path, bytes).unwrap();
                set_old(&path);
            }),
            ("cases/archive/2025/notes.txt: modified time changed", |m| {
                std::fs::File::options()
                    .write(true)
                    .open(m.join("cases/archive/2025/notes.txt"))
                    .unwrap()
                    .set_modified(old_time() + Duration::from_secs(60))
                    .unwrap();
            }),
            ("cases/archive/2025/extra.case is new", |m| {
                std::fs::write(m.join("cases/archive/2025/extra.case"), b"x").unwrap();
            }),
            ("cases/archive/empty is gone", |m| {
                std::fs::remove_dir(m.join("cases/archive/empty")).unwrap();
            }),
            ("recents.json is gone", |m| {
                std::fs::remove_file(m.join("recents.json")).unwrap();
            }),
        ];
        for (expected, touch) in touches {
            let (_dir, mdoc, snapshot) = filled();
            touch(&mdoc);
            let changes = snapshot.changes(&Snapshot::take(&mdoc).unwrap());
            let expected = expected.replace('/', std::path::MAIN_SEPARATOR_STR);
            assert!(
                changes.iter().any(|change| change == &expected),
                "{expected} in {changes:?}"
            );
            assert!(snapshot.check(&mdoc).is_err());
        }
    }

    fn set_old(path: &Path) {
        std::fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(old_time())
            .unwrap();
    }
}
