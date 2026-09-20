//! Accepted document identity and import provenance. Preview identity is separate:
//! opening Markdown does not close a reference PDF.
use crate::document::Document;
use std::{io, path::PathBuf};

#[derive(Default)]
pub(super) struct DocumentSession {
    pub document: Document,
    pub generation: u64,
    pub source: Option<PathBuf>,
    pub warning: Option<String>,
}

impl DocumentSession {
    pub fn replace(&mut self, document: Document) {
        self.generation = self.generation.wrapping_add(1);
        self.document = document;
        self.source = None;
        self.warning = None;
    }

    pub fn import(&mut self, source: PathBuf, warning: Option<String>) {
        self.replace(Document::default());
        self.source = Some(source);
        self.warning = warning;
    }

    pub fn dirty(&self, text: &str) -> bool {
        (self.document.path.is_none() && self.source.is_some()) || text != self.document.saved
    }

    /// Save As keeps the existing import-based suggestion even after a save.
    pub fn suggested_name(&self) -> String {
        self.source
            .as_ref()
            .map(|path| path.with_extension("md"))
            .and_then(|path| path.file_name()?.to_str().map(str::to_owned))
            .unwrap_or_else(|| "Untitled.md".into())
    }

    pub fn display_name(&self) -> String {
        let suggested = self.source.as_ref().map(|path| path.with_extension("md"));
        self.document
            .path
            .as_deref()
            .or(suggested.as_deref())
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled.md".into())
    }

    pub fn directory(&self) -> PathBuf {
        if self.document.path.is_none()
            && let Some(parent) = self.source.as_ref().and_then(|path| path.parent())
        {
            return parent.to_path_buf();
        }
        self.document.directory()
    }

    pub fn save(&mut self, path: PathBuf, text: &str) -> io::Result<()> {
        if let Some(source) = &self.source
            && (std::path::absolute(&path).ok().as_ref() == Some(source)
                || (path.exists() && path.canonicalize().ok() == source.canonicalize().ok()))
        {
            return Err(io::Error::other(
                "Choose a different path to preserve the imported source.",
            ));
        }
        self.document.save(path, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_changes_once_and_clears_import_provenance() {
        let mut session = DocumentSession::default();
        session.import("source.docx".into(), Some("conversion warning".into()));
        assert_eq!(session.generation, 1);
        assert!(session.dirty(""), "even an empty import is unsaved");
        session.replace(Document::default());
        assert_eq!(session.generation, 2);
        assert!(session.source.is_none());
        assert!(session.warning.is_none());
        assert!(!session.dirty(""));
    }

    #[test]
    fn saving_changes_display_name_but_preserves_import_suggestion_and_provenance() {
        let dir = tempfile::tempdir().unwrap();
        let mut session = DocumentSession::default();
        session.import(dir.path().join("source.docx"), Some("warning".into()));
        assert_eq!(session.display_name(), "source.md");
        assert_eq!(session.suggested_name(), "source.md");
        let generation = session.generation;
        session
            .save(dir.path().join("renamed.md"), "# preserved\n")
            .unwrap();
        assert_eq!(session.display_name(), "renamed.md");
        assert_eq!(session.suggested_name(), "source.md");
        assert_eq!(session.generation, generation);
        assert!(session.warning.is_some());
        assert!(!session.dirty("# preserved\n"));
        session.replace(Document::default());
        assert_eq!(session.display_name(), "Untitled.md");
        assert_eq!(session.suggested_name(), "Untitled.md");
    }
}
