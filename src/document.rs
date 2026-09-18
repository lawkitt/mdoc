//! Plain-file persistence; never opens the old journal database.
use std::{
    io::{self, Write},
    path::{Path, PathBuf},
};

#[derive(Default)]
pub struct Document {
    pub path: Option<PathBuf>,
    pub saved: String,
}

impl Document {
    pub fn open(path: PathBuf) -> io::Result<Self> {
        let path = std::path::absolute(path)?;
        let saved = std::fs::read_to_string(&path)?;
        Ok(Self {
            path: Some(path),
            saved,
        })
    }

    pub fn save(&mut self, path: PathBuf, text: &str) -> io::Result<()> {
        let path = std::path::absolute(path)?;
        let target = if path.exists() {
            path.canonicalize()?
        } else {
            path.clone()
        };
        if self.path.as_ref() == Some(&path) && std::fs::read_to_string(&target)? != self.saved {
            return Err(io::Error::other(
                "The file changed on disk. Use Save As to preserve your edits.",
            ));
        }
        let parent = target
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        if let Ok(metadata) = std::fs::metadata(&target) {
            temporary
                .as_file()
                .set_permissions(metadata.permissions())?;
        }
        temporary.write_all(text.as_bytes())?;
        temporary.as_file().sync_all()?;
        temporary.persist(&target).map_err(|e| e.error)?;
        self.path = Some(path);
        self.saved = text.to_owned();
        Ok(())
    }

    pub fn directory(&self) -> PathBuf {
        self.path
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
    }
}

pub fn is_markdown(path: &Path) -> bool {
    path.extension().and_then(|s| s.to_str()).is_some_and(|s| {
        ["md", "markdown", "mdown", "txt"]
            .iter()
            .any(|ext| s.eq_ignore_ascii_case(ext))
    })
}

pub fn local_path(reference: &str, directory: &Path) -> Option<PathBuf> {
    // Drive-letter paths must not be interpreted as URL schemes on Windows.
    if Path::new(reference).is_absolute() {
        return Some(PathBuf::from(reference.split('#').next()?));
    }
    if let Ok(url) = url::Url::parse(reference) {
        return url.to_file_path().ok();
    }
    url::Url::from_directory_path(directory)
        .ok()?
        .join(reference)
        .ok()?
        .to_file_path()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn save_roundtrips_and_refuses_external_edits() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.md");
        let mut doc = Document::default();
        doc.save(path.clone(), "# Привет\n\n**Hello**\n").unwrap();
        assert_eq!(Document::open(path.clone()).unwrap().saved, doc.saved);
        std::fs::write(&path, "external").unwrap();
        assert!(doc.save(path.clone(), "my edits").is_err());
        assert_eq!(std::fs::read_to_string(path).unwrap(), "external");
    }
    #[test]
    fn failed_save_preserves_document_state() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = Document::default();
        assert!(
            doc.save(dir.path().join("missing/note.md"), "text")
                .is_err()
        );
        assert!(doc.path.is_none());
        assert!(doc.saved.is_empty());
    }
    #[test]
    fn relative_links_resolve_from_document() {
        let base = std::env::current_dir().unwrap();
        assert_eq!(
            local_path("manual.pdf#p2", &base),
            Some(base.join("manual.pdf"))
        );
        assert_eq!(local_path("https://example.com/a.pdf", &base), None);
        assert_eq!(
            local_path("my%20notes.md", &base),
            Some(base.join("my notes.md"))
        );
        let path = base.join("notes.md");
        assert_eq!(
            local_path(url::Url::from_file_path(&path).unwrap().as_ref(), &base),
            Some(path)
        );
        assert!(is_markdown(Path::new("NOTE.MD")));
        assert!(!is_markdown(Path::new("file.exe")));
    }

    #[cfg(unix)]
    #[test]
    fn saving_through_symlink_preserves_link() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target.md");
        let link = dir.path().join("link.md");
        std::fs::write(&target, "before").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let mut doc = Document::open(link.clone()).unwrap();
        doc.save(link.clone(), "after").unwrap();
        assert!(link.is_symlink());
        assert_eq!(std::fs::read_to_string(target).unwrap(), "after");
    }
}
