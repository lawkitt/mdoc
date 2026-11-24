//! Accepted document identity and import provenance. Preview identity is separate:
//! opening Markdown does not close a reference PDF.
use crate::document::Document;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct DocumentSession {
    pub document: Document,
    pub generation: u64,
    pub source: Option<PathBuf>,
    /// The import's notice: a partial import.
    pub warning: Option<String>,
    /// The left-out line: what else the Markdown leaves out, counted.
    pub left_out: Option<String>,
    /// The comments the Markdown leaves out ("3 comments not shown."), which
    /// join the left-out line while no rendering of the source shows them.
    pub comments_left_out: Option<String>,
    pub ocr_configuration: Option<crate::settings::OcrConfig>,
    /// Fallback name before a file loads, a Vault document's title, or the
    /// name of a document not yet created.
    pub name: Option<String>,
    /// A new document's title follows its first line.
    pub follows_first_line: bool,
    /// Pasted text: its document is an ordinary one, not a new one.
    pub pasted: bool,
    /// A conversion replaced the text: it is written even if it reads as
    /// the text last written, so the document is known to be converted.
    pub write_due: bool,
}

impl DocumentSession {
    pub fn replace(&mut self, document: Document) {
        self.generation = self.generation.wrapping_add(1);
        self.document = document;
        self.source = None;
        self.warning = None;
        self.left_out = None;
        self.comments_left_out = None;
        self.ocr_configuration = None;
        self.name = None;
        self.follows_first_line = false;
        self.pasted = false;
        self.write_due = false;
    }

    pub fn import(&mut self, source: PathBuf, warning: Option<String>) {
        self.replace(Document::default());
        self.source = Some(source);
        self.warning = warning;
    }

    /// Whether `text` differs from what was last written: an autosave is
    /// due. A document not yet created has nothing written.
    pub fn unsaved(&self, text: &str) -> bool {
        self.write_due || text != self.document.saved
    }

    /// Export Markdown's suggested file name: the source's, else the
    /// document's own.
    pub fn suggested_name(&self) -> String {
        self.source
            .as_ref()
            .map(|path| path.with_extension("md"))
            .and_then(|path| path.file_name()?.to_str().map(str::to_owned))
            .or_else(|| self.name.clone())
            .unwrap_or_else(|| "Untitled.md".into())
    }

    pub fn display_name(&self) -> String {
        // A Vault document goes by its title.
        if self.document.vault.is_some()
            && let Some(name) = &self.name
        {
            return name.clone();
        }
        let suggested = self.source.as_ref().map(|path| path.with_extension("md"));
        suggested
            .as_deref()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .or_else(|| self.name.clone())
            .unwrap_or_else(|| "Untitled.md".into())
    }

    pub fn directory(&self) -> PathBuf {
        if let Some(parent) = self.source.as_ref().and_then(|path| path.parent()) {
            return parent.to_path_buf();
        }
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    }

    /// Where Export Markdown starts (#180): the document's folder, or its
    /// source's for a conversion. A document with neither (Untitled, pasted
    /// text, an AI reply), or whose folder is gone, starts in `last_folder`
    /// (the folder of the last file imported or exported this run) when it
    /// exists, then Documents, then home: never the process's working
    /// directory, which for a launched app is its install folder.
    pub fn export_directory(&self, last_folder: Option<&Path>) -> PathBuf {
        let own = self.source.is_some().then(|| self.directory());
        own.into_iter()
            .chain(last_folder.map(Path::to_path_buf))
            .chain(dirs::document_dir())
            .chain(dirs::home_dir())
            .find(|dir| dir.is_dir())
            .unwrap_or_else(|| self.directory())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Where Export Markdown starts with no folder of its own.
    fn fallback() -> PathBuf {
        dirs::document_dir()
            .filter(|dir| dir.is_dir())
            .or_else(dirs::home_dir)
            .unwrap()
    }

    #[test]
    fn a_conversion_exports_from_its_source_folder() {
        let dir = tempfile::tempdir().unwrap();
        let last = tempfile::tempdir().unwrap();
        let mut session = DocumentSession::default();
        session.import(dir.path().join("scan.pdf"), None);
        assert_eq!(session.export_directory(Some(last.path())), dir.path());
    }

    #[test]
    fn a_document_without_a_folder_exports_from_the_last_folder_then_documents() {
        let last = tempfile::tempdir().unwrap();
        let gone = last.path().join("gone");
        let session = DocumentSession::default();
        assert_eq!(session.export_directory(Some(last.path())), last.path());
        assert_eq!(session.export_directory(Some(&gone)), fallback());
        assert_eq!(session.export_directory(None), fallback());
        assert_ne!(
            session.export_directory(None),
            std::env::current_dir().unwrap()
        );
    }

    #[test]
    fn a_conversion_whose_source_folder_is_gone_exports_from_the_last_folder() {
        let dir = tempfile::tempdir().unwrap();
        let last = tempfile::tempdir().unwrap();
        let mut session = DocumentSession::default();
        session.import(dir.path().join("gone").join("scan.pdf"), None);
        assert_eq!(session.export_directory(Some(last.path())), last.path());
        assert_eq!(session.export_directory(None), fallback());
    }

    #[test]
    fn identity_changes_once_and_clears_import_provenance() {
        let mut session = DocumentSession::default();
        session.import("source.docx".into(), Some("conversion warning".into()));
        session.left_out = Some("1 picture not imported.".into());
        session.comments_left_out = Some("1 comment not shown.".into());
        session.follows_first_line = true;
        assert_eq!(session.generation, 1);
        assert_eq!(session.display_name(), "source.md");
        assert_eq!(session.suggested_name(), "source.md");
        session.replace(Document::default());
        assert_eq!(session.generation, 2);
        assert!(session.source.is_none());
        assert!(session.warning.is_none());
        assert!(session.left_out.is_none() && session.comments_left_out.is_none());
        assert!(!session.follows_first_line);
        assert_eq!(session.display_name(), "Untitled.md");
    }

    #[test]
    fn a_vault_document_is_named_by_its_title() {
        let mut session = DocumentSession::default();
        let id = crate::document_id::VaultId::parse(&"0b".repeat(16)).unwrap();
        session.replace(Document::in_vault(id, "# Lease".into()));
        session.name = Some("Lease".into());
        assert_eq!(session.display_name(), "Lease");
        assert_eq!(session.suggested_name(), "Lease");
        assert!(!session.unsaved("# Lease"));
        assert!(session.unsaved("# Lease 2"));
    }
}
