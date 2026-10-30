//! Accepted document identity and import provenance. Preview identity is separate:
//! opening Markdown does not close a reference PDF.
use crate::document::Document;
use std::{io, path::PathBuf};

#[derive(Default)]
pub struct DocumentSession {
    pub document: Document,
    pub generation: u64,
    pub source: Option<PathBuf>,
    pub warning: Option<String>,
    pub ocr_configuration: Option<crate::settings::OcrConfig>,
    /// Fallback name before a file loads, or for an unsaved document without a source.
    pub name: Option<String>,
}

impl DocumentSession {
    pub fn replace(&mut self, document: Document) {
        self.generation = self.generation.wrapping_add(1);
        self.document = document;
        self.source = None;
        self.warning = None;
        self.ocr_configuration = None;
        self.name = None;
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
            .or_else(|| {
                self.document
                    .path
                    .as_ref()
                    .and_then(|path| path.file_name())
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .or_else(|| self.name.clone())
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
            .or_else(|| self.name.clone())
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

    /// Where Save As starts (#180): the document's folder, or its source's
    /// for an unsaved conversion. A document with neither (Untitled, pasted
    /// text, an AI reply), or whose folder is gone, starts in the first of
    /// `recent_folders` that exists (Recents, most recent first), then
    /// Documents, then home: never the process's working directory, which
    /// for a launched app is its install folder.
    pub fn save_as_directory(&self, recent_folders: &[PathBuf]) -> PathBuf {
        let own = (self.document.path.is_some() || self.source.is_some()).then(|| self.directory());
        own.into_iter()
            .chain(recent_folders.iter().cloned())
            .chain(dirs::document_dir())
            .chain(dirs::home_dir())
            .find(|dir| dir.is_dir())
            .unwrap_or_else(|| self.directory())
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

    /// Where Save As starts with no folder of its own and no Recents.
    fn fallback() -> PathBuf {
        dirs::document_dir()
            .filter(|dir| dir.is_dir())
            .or_else(dirs::home_dir)
            .unwrap()
    }

    #[test]
    fn save_as_starts_in_the_document_or_source_folder() {
        let dir = tempfile::tempdir().unwrap();
        let recent = tempfile::tempdir().unwrap();
        let recents = [recent.path().to_path_buf()];
        let mut session = DocumentSession::default();
        session.import(dir.path().join("scan.pdf"), None);
        assert_eq!(session.save_as_directory(&recents), dir.path());
        session.save(dir.path().join("scan.md"), "text").unwrap();
        assert_eq!(session.save_as_directory(&recents), dir.path());
    }

    #[test]
    fn untitled_save_as_starts_in_a_recent_folder_then_documents() {
        let recent = tempfile::tempdir().unwrap();
        let gone = recent.path().join("gone");
        let session = DocumentSession::default();
        let recents = [gone.clone(), recent.path().to_path_buf()];
        assert_eq!(session.save_as_directory(&recents), recent.path());
        assert_eq!(session.save_as_directory(&[gone]), fallback());
        assert_eq!(session.save_as_directory(&[]), fallback());
        assert_ne!(
            session.save_as_directory(&[]),
            std::env::current_dir().unwrap()
        );
    }

    #[test]
    fn a_conversion_whose_source_folder_is_gone_starts_in_a_recent_folder() {
        let dir = tempfile::tempdir().unwrap();
        let recent = tempfile::tempdir().unwrap();
        let mut session = DocumentSession::default();
        session.import(dir.path().join("gone").join("scan.pdf"), None);
        let recents = [recent.path().to_path_buf()];
        assert_eq!(session.save_as_directory(&recents), recent.path());
    }

    #[test]
    fn saved_untitled_tab_suggests_its_current_filename() {
        let dir = tempfile::tempdir().unwrap();
        let mut session = DocumentSession {
            name: Some("Untitled 2.md".into()),
            ..DocumentSession::default()
        };
        assert_eq!(session.suggested_name(), "Untitled 2.md");
        session.save(dir.path().join("notes.md"), "notes").unwrap();
        assert_eq!(session.display_name(), "notes.md");
        assert_eq!(session.suggested_name(), "notes.md");
    }

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
