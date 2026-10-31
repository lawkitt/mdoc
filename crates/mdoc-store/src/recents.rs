//! Recently opened files for Home and the command palette (ADR 0048).
//! Paths and dates only, never content; stored beside the session and
//! cleared from Settings.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const LIMIT: usize = 12;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Recent {
    pub path: PathBuf,
    /// Seconds since the Unix epoch.
    pub opened: u64,
    /// The document had applied replacements; a scan alone doesn't count
    /// (#306).
    #[serde(default)]
    pub pseudonymized: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Recents {
    pub files: Vec<Recent>,
    /// Where changes are saved; none keeps them in memory.
    #[serde(skip)]
    path: Option<PathBuf>,
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

impl Recents {
    /// The recents file under app data, if the system has a folder for it.
    pub fn default_path() -> Option<PathBuf> {
        crate::app_paths::data_local_dir().map(|path| path.join("mdoc/recents.json"))
    }

    /// The list saved at `path`; changes are saved back there.
    pub fn load(path: Option<PathBuf>) -> Self {
        let recents: Self = path
            .as_ref()
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self { path, ..recents }
    }

    fn save(&self) {
        let Some(path) = &self.path else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(bytes) = serde_json::to_vec_pretty(self) {
            let _ = std::fs::write(path, bytes);
        }
    }

    /// Move `path` to the front; returns whether the list changed. `source`
    /// is the file `path` was converted from: its row folds into this one,
    /// so a conversion saved as `.md` is listed once (#306).
    pub fn touch(&mut self, path: &Path, pseudonymized: bool, source: Option<&Path>) -> bool {
        if path.as_os_str().is_empty() {
            return false;
        }
        let folded = source
            .filter(|source| *source != path)
            .and_then(|source| self.files.iter().position(|r| r.path == source));
        if let Some(i) = folded {
            self.files.remove(i);
        }
        let previous = self.files.iter().position(|r| r.path == path);
        let pseudonymized = pseudonymized || previous.is_some_and(|i| self.files[i].pseudonymized);
        if folded.is_none() && previous == Some(0) && self.files[0].pseudonymized == pseudonymized {
            return false;
        }
        if let Some(i) = previous {
            self.files.remove(i);
        }
        self.files.insert(
            0,
            Recent {
                path: path.to_path_buf(),
                opened: now(),
                pseudonymized,
            },
        );
        self.files.truncate(LIMIT);
        self.save();
        true
    }

    /// The recent files' folders, most recent first: where Save As starts for
    /// a document with no folder of its own (#180).
    pub fn folders(&self) -> Vec<PathBuf> {
        let mut folders: Vec<PathBuf> = Vec::new();
        for folder in self.files.iter().filter_map(|recent| recent.path.parent()) {
            if !folders.iter().any(|known| known == folder) {
                folders.push(folder.to_path_buf());
            }
        }
        folders
    }

    pub fn clear(&mut self) {
        self.files.clear();
        self.save();
    }
}

/// "Today 10:37", "Yesterday", "3 days ago", "Oct 14".
pub fn when(opened: u64) -> String {
    let age = now().saturating_sub(opened);
    let days = age / 86_400;
    match days {
        0 if age < 60 => "Just now".into(),
        0 if age < 3600 => format!("{} min ago", age / 60),
        0 => format!("{} h ago", age / 3600),
        1 => "Yesterday".into(),
        2..=6 => format!("{days} days ago"),
        _ => format!("{} weeks ago", days / 7),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn touching_moves_a_file_to_the_front_and_keeps_its_badge() {
        let mut recents = Recents::default();
        recents.touch(Path::new("/a.pdf"), true, None);
        recents.touch(Path::new("/b.docx"), false, None);
        recents.touch(Path::new("/a.pdf"), false, None);
        let paths: Vec<_> = recents.files.iter().map(|r| r.path.clone()).collect();
        assert_eq!(paths, [PathBuf::from("/a.pdf"), PathBuf::from("/b.docx")]);
        assert!(recents.files[0].pseudonymized);
    }

    #[test]
    fn a_saved_conversion_folds_its_source_into_its_row() {
        let mut recents = Recents::default();
        recents.touch(Path::new("/notes.md"), false, None);
        recents.touch(Path::new("/demo.docx"), true, None);
        assert!(recents.touch(Path::new("/demo.md"), true, Some(Path::new("/demo.docx"))));
        let paths: Vec<_> = recents.files.iter().map(|r| r.path.clone()).collect();
        assert_eq!(
            paths,
            [PathBuf::from("/demo.md"), PathBuf::from("/notes.md")]
        );
        // Already in front with no source row left: nothing changes.
        assert!(!recents.touch(Path::new("/demo.md"), true, Some(Path::new("/demo.docx"))));
        // A source that is the file itself is not folded away.
        assert!(!recents.touch(Path::new("/demo.md"), true, Some(Path::new("/demo.md"))));
        assert_eq!(recents.files.len(), 2);
    }

    #[test]
    fn folding_a_source_already_behind_the_front_row_still_counts() {
        let mut recents = Recents::default();
        recents.touch(Path::new("/demo.docx"), false, None);
        recents.touch(Path::new("/demo.md"), false, None);
        assert!(recents.touch(Path::new("/demo.md"), false, Some(Path::new("/demo.docx"))));
        assert_eq!(recents.files.len(), 1);
    }

    #[test]
    fn folders_are_most_recent_first_without_repeats() {
        let mut recents = Recents::default();
        recents.touch(Path::new("/a/1.pdf"), false, None);
        recents.touch(Path::new("/b/2.md"), false, None);
        recents.touch(Path::new("/a/3.docx"), false, None);
        assert_eq!(
            recents.folders(),
            [PathBuf::from("/a"), PathBuf::from("/b")]
        );
        recents.clear();
        assert!(recents.folders().is_empty());
    }

    #[test]
    fn the_list_is_capped() {
        let mut recents = Recents::default();
        for i in 0..20 {
            recents.touch(Path::new(&format!("/{i}.md")), false, None);
        }
        assert_eq!(recents.files.len(), LIMIT);
        assert_eq!(recents.files[0].path, PathBuf::from("/19.md"));
    }
}
