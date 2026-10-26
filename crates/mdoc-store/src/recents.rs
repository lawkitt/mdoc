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

    /// Move `path` to the front; returns whether the list changed.
    pub fn touch(&mut self, path: &Path, pseudonymized: bool) -> bool {
        if path.as_os_str().is_empty() {
            return false;
        }
        let previous = self.files.iter().position(|r| r.path == path);
        let pseudonymized = pseudonymized || previous.is_some_and(|i| self.files[i].pseudonymized);
        if previous == Some(0) && self.files[0].pseudonymized == pseudonymized {
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
        recents.touch(Path::new("/a.pdf"), true);
        recents.touch(Path::new("/b.docx"), false);
        recents.touch(Path::new("/a.pdf"), false);
        let paths: Vec<_> = recents.files.iter().map(|r| r.path.clone()).collect();
        assert_eq!(paths, [PathBuf::from("/a.pdf"), PathBuf::from("/b.docx")]);
        assert!(recents.files[0].pseudonymized);
    }

    #[test]
    fn the_list_is_capped() {
        let mut recents = Recents::default();
        for i in 0..20 {
            recents.touch(Path::new(&format!("/{i}.md")), false);
        }
        assert_eq!(recents.files.len(), LIMIT);
        assert_eq!(recents.files[0].path, PathBuf::from("/19.md"));
    }
}
