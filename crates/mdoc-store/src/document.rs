//! What a view holds of its document: a Vault document's id and the text
//! last saved. Documents are never plain files: Export Markdown writes a
//! copy. Never opens the old journal database.
use crate::document_id::{DocumentId, VaultId};
use std::{
    io::{self, Write},
    path::{Path, PathBuf},
};

#[derive(Default)]
pub struct Document {
    /// The Vault document this is, once it was created.
    pub vault: Option<VaultId>,
    pub saved: String,
    /// A Vault document's review as last saved with `saved`.
    pub review: Option<mdoc_pii::saved::SavedReview>,
}

impl Document {
    /// A Vault document whose text was last saved as `saved`.
    pub fn in_vault(id: VaultId, saved: String) -> Self {
        Self {
            vault: Some(id),
            saved,
            review: None,
        }
    }

    /// The Vault document this is, once it was created.
    pub fn id(&self) -> Option<DocumentId> {
        self.vault.clone().map(DocumentId::Vault)
    }

    /// Whether this is `document`, without building an id.
    pub fn is(&self, document: &DocumentId) -> bool {
        self.vault.as_ref() == Some(document.vault())
    }
}

/// Write `text` to `path` whole, replacing what is there, through a
/// temporary file in its folder: Export Markdown. A link is written
/// through, and an existing file keeps its permissions.
pub fn export(path: &Path, text: &str) -> io::Result<()> {
    let path = std::path::absolute(path)?;
    let target = if path.exists() {
        path.canonicalize()?
    } else {
        path
    };
    let parent = target
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    // A new file gets the usual umask permissions, not tempfile's 0600 (#76).
    #[cfg(unix)]
    let builder = {
        let mut builder = tempfile::Builder::new();
        builder.permissions(std::os::unix::fs::PermissionsExt::from_mode(0o666));
        builder
    };
    #[cfg(not(unix))]
    let builder = tempfile::Builder::new();
    let mut temporary = builder.tempfile_in(parent)?;
    if let Ok(metadata) = std::fs::metadata(&target) {
        temporary
            .as_file()
            .set_permissions(metadata.permissions())?;
    }
    temporary.write_all(text.as_bytes())?;
    temporary.as_file().sync_all()?;
    temporary.persist(&target).map_err(|e| e.error)?;
    Ok(())
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
    fn export_writes_the_text_whole_and_replaces_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.md");
        export(&path, "# Привет\n\n**Hello**\n").unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "# Привет\n\n**Hello**\n"
        );
        std::fs::write(&path, "external").unwrap();
        export(&path, "again").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "again");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
    #[cfg(unix)]
    #[test]
    fn a_new_file_gets_umask_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let reference = dir.path().join("reference");
        std::fs::write(&reference, "").unwrap();
        let path = dir.path().join("note.md");
        export(&path, "text").unwrap();
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&path), mode(&reference));
    }

    #[test]
    fn a_failed_export_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(export(&dir.path().join("missing/note.md"), "text").is_err());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
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
    fn exporting_through_a_symlink_preserves_the_link() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target.md");
        let link = dir.path().join("link.md");
        std::fs::write(&target, "before").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();
        export(&link, "after").unwrap();
        assert!(link.is_symlink());
        assert_eq!(std::fs::read_to_string(target).unwrap(), "after");
    }
}
