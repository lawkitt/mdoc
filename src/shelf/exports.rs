//! Shelf exports (ADR 0084): the `.md` file a drag carries, written fresh
//! from the current text into a private temporary folder that is cleared at
//! quit. Each process owns one folder and holds a lock in it; a folder whose
//! lock is free belongs to a process that ended without clearing it (a
//! crash), and the next launch removes it.

use super::ShelfDocument;
use std::{
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

/// Folders are named with this prefix in the temporary directory.
const PREFIX: &str = "mdoc-shelf-";
const LOCK: &str = ".lock";

pub(crate) struct ShelfExports {
    root: PathBuf,
    /// Held while this process runs; its lock marks the folder as in use.
    /// Released before the folder is removed (Windows keeps open files).
    lock: Option<File>,
    /// Each export gets its own numbered folder, so its file keeps the
    /// title as its name.
    next: AtomicU64,
}

impl ShelfExports {
    /// This process's folder in the system's temporary directory, after
    /// removing folders left by processes that ended without clearing them.
    pub(crate) fn create() -> io::Result<Self> {
        Self::create_in(&std::env::temp_dir())
    }

    pub(crate) fn create_in(parent: &Path) -> io::Result<Self> {
        sweep(parent);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let root = parent.join(format!("{PREFIX}{}-{nanos}", std::process::id()));
        private_dir(&root)?;
        let lock = File::create(root.join(LOCK))?;
        lock.try_lock().map_err(io::Error::other)?;
        Ok(Self {
            root,
            lock: Some(lock),
            next: AtomicU64::new(1),
        })
    }

    #[cfg(test)]
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    /// Write `document`'s Copy for AI text to a fresh `.md` file named after
    /// its pseudonymized title. `None` when the document is not ready.
    pub(crate) fn export(&self, document: &ShelfDocument) -> io::Result<Option<PathBuf>> {
        let Some(text) = document.text_for_ai() else {
            return Ok(None);
        };
        let folder = self
            .root
            .join(self.next.fetch_add(1, Ordering::Relaxed).to_string());
        private_dir(&folder)?;
        let path = folder.join(document.file_name());
        let mut file = private_file(&path)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        Ok(Some(path))
    }

    /// Remove every export, at quit. The folder itself goes with the lock.
    pub(crate) fn clear(&self) {
        let Ok(entries) = fs::read_dir(&self.root) else {
            return;
        };
        for entry in entries.flatten() {
            if entry.file_name() != LOCK {
                let _ = fs::remove_dir_all(entry.path());
            }
        }
    }
}

impl Drop for ShelfExports {
    fn drop(&mut self) {
        drop(self.lock.take());
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Remove shelf folders in `parent` whose owner no longer holds the lock.
fn sweep(parent: &Path) {
    let Ok(entries) = fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        if !entry.file_name().to_string_lossy().starts_with(PREFIX) {
            continue;
        }
        let path = entry.path();
        // A folder without a lock file is one whose creation never finished.
        let free = match File::open(path.join(LOCK)) {
            Ok(lock) => lock.try_lock().is_ok(),
            Err(_) => true,
        };
        if free && entry.file_type().is_ok_and(|t| t.is_dir()) {
            let _ = fs::remove_dir_all(&path);
        }
    }
}

/// A directory only this user can read.
fn private_dir(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new().mode(0o700).create(path)
    }
    // The temporary directory is the user's own.
    #[cfg(not(unix))]
    fs::create_dir(path)
}

/// A new file only this user can read.
fn private_file(path: &Path) -> io::Result<File> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)
}
