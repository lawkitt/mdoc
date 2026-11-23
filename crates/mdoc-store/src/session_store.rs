//! Small, versioned session manifest: Vault documents by id, never their
//! titles or contents.
use crate::document_id::{DocumentId, VaultId};
use serde::{Deserialize, Serialize};
use std::{
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum PreviewFit {
    // Older manifests lack a fit policy. Restore them with the same responsive
    // default as newly opened PDF/DOCX previews.
    #[default]
    Width,
    Page,
    Manual,
}

/// What the user picked at the foot of the sidebar. System follows
/// the OS appearance; the app resolves it to the theme everything paints with.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeChoice {
    pub fn next(self) -> Self {
        match self {
            Self::System => Self::Light,
            Self::Light => Self::Dark,
            Self::Dark => Self::System,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TabRecord {
    /// The Vault document this tab shows; such a tab keeps no path, name or
    /// attachment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document: Option<VaultId>,
    pub markdown: PathBuf,
    pub source_only: bool,
    pub blank: bool,
    pub blank_disposable: bool,
    /// The unsaved tab's name, retained across restarts without its contents.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub attachment: Option<PathBuf>,
    pub preview_visible: bool,
    pub caret: usize,
    pub scroll_y: f32,
    pub preview_page: usize,
    pub preview_zoom: Option<f32>,
    pub preview_fit: PreviewFit,
    pub preview_split: Option<f32>,
    /// The saved case this document belongs to; never its contents.
    pub case: Option<String>,
}

impl TabRecord {
    /// The document this tab names: none for a blank or a source tab.
    pub fn document(&self) -> Option<DocumentId> {
        match &self.document {
            Some(id) => Some(DocumentId::Vault(id.clone())),
            None => self
                .names_document()
                .then(|| DocumentId::File(self.markdown.clone())),
        }
    }

    /// Whether this tab names a document, without building its id.
    pub fn names_document(&self) -> bool {
        self.document.is_some() || !self.markdown.as_os_str().is_empty()
    }

    /// Whether both tabs name the same document.
    pub fn same_document(&self, other: &TabRecord) -> bool {
        self.names_document() && self.document == other.document && self.markdown == other.markdown
    }

    /// The tab names `document` from now on.
    pub fn set_document(&mut self, document: DocumentId) {
        match document {
            DocumentId::Vault(id) => {
                self.document = Some(id);
                self.markdown = PathBuf::new();
                self.name = None;
            }
            DocumentId::File(path) => {
                self.document = None;
                self.markdown = path;
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub version: u32,
    pub tabs: Vec<TabRecord>,
    pub active: usize,
    pub sidebar_visible: bool,
    #[serde(default)]
    pub sidebar_choice: Option<bool>,
    /// Saved cases folded in the sidebar, by id.
    #[serde(default)]
    pub collapsed_cases: Vec<String>,
    /// Case labels and counts for locked pages; never originals or member contents.
    #[serde(default)]
    pub cases: Vec<crate::cases::CaseSummary>,
    /// The sidebar's System / Light / Dark pick.
    #[serde(default)]
    pub theme_choice: ThemeChoice,
}
/// The session format: 2 names Vault documents by id. A version 1 session's
/// file tabs are dropped on load, with no migration; their files stay on
/// disk.
pub const VERSION: u32 = 2;

impl Default for Session {
    fn default() -> Self {
        Self {
            version: VERSION,
            tabs: Vec::new(),
            active: 0,
            sidebar_visible: true,
            sidebar_choice: None,
            collapsed_cases: Vec::new(),
            cases: Vec::new(),
            theme_choice: Default::default(),
        }
    }
}

pub fn load(path: &Path) -> io::Result<Session> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Session::default()),
        Err(error) => return Err(error),
    };
    // A corrupt/untrusted manifest must not consume unbounded startup memory.
    let mut bytes = Vec::new();
    file.take(4 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(io::Error::other("Session file is too large"));
    }
    let mut session: Session = serde_json::from_slice(&bytes)?;
    match session.version {
        1 => {
            session.tabs.retain(|tab| {
                tab.blank && !tab.source_only && tab.document.is_none() && tab.attachment.is_none()
            });
            session.active = 0;
            session.version = VERSION;
        }
        VERSION => {}
        _ => return Err(io::Error::other("Unsupported session version")),
    }
    if session
        .collapsed_cases
        .iter()
        .chain(session.cases.iter().map(|case| &case.id))
        .any(|id| id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err(io::Error::other("Invalid case id in session"));
    }
    if session.tabs.iter().any(|tab| {
        !(if tab.document.is_some() {
            !tab.source_only
                && !tab.blank
                && tab.markdown.as_os_str().is_empty()
                && tab.attachment.is_none()
        } else if tab.source_only {
            tab.markdown.as_os_str().is_empty()
                && tab
                    .attachment
                    .as_ref()
                    .is_some_and(|path| path.is_absolute())
        } else if tab.blank {
            tab.markdown.as_os_str().is_empty() && tab.attachment.is_none()
        } else {
            tab.markdown.is_absolute()
        }) || tab
            .attachment
            .as_ref()
            .is_some_and(|path| !path.is_absolute())
            || tab
                .case
                .as_ref()
                .is_some_and(|id| id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()))
            || !tab.scroll_y.is_finite()
            || tab.preview_zoom.is_some_and(|z| !z.is_finite())
            || tab
                .preview_split
                .is_some_and(|v| !v.is_finite() || !(0.2..=0.8).contains(&v))
    }) {
        return Err(io::Error::other(
            "Invalid session paths or reading positions",
        ));
    }
    Ok(session)
}

pub fn save(path: &Path, session: &Session) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("Missing session directory"))?;
    std::fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer(&mut file, session)?;
    file.flush()?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|error| error.error)?;
    Ok(())
}

/// Used at file-open/save boundaries, never during rendering or tab switching.
/// `dunce` keeps Windows paths in their ordinary `C:\…` form; std's canonical
/// `\\?\C:\…` form would show in tooltips and saved sessions.
pub fn identity(path: &Path) -> PathBuf {
    dunce::canonicalize(path).unwrap_or_else(|_| {
        let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        absolute
            .parent()
            .and_then(|parent| dunce::canonicalize(parent).ok())
            .zip(absolute.file_name())
            .map(|(parent, name)| parent.join(name))
            .unwrap_or(absolute)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn manifest_roundtrips_without_document_text_and_replaces_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        assert_eq!(load(&path).unwrap(), Session::default());
        let mut session = Session::default();
        session.tabs.push(TabRecord {
            document: None,
            markdown: dir.path().join("note.md"),
            source_only: false,
            blank: false,
            blank_disposable: false,
            name: None,
            attachment: Some(dir.path().join("source.docx")),
            preview_visible: false,
            caret: 8,
            scroll_y: -120.,
            preview_page: 4,
            preview_zoom: Some(1.25),
            preview_fit: PreviewFit::Manual,
            preview_split: Some(0.6),
            case: Some("ab".repeat(16)),
        });
        save(&path, &session).unwrap();
        assert_eq!(load(&path).unwrap(), session);
        // A case is referenced by id only; anything else is rejected.
        session.tabs[0].case = Some("../secret".into());
        save(&path, &session).unwrap();
        assert!(load(&path).is_err());
        session.tabs[0].case = Some("ab".repeat(16));
        session.tabs.clear();
        save(&path, &session).unwrap();
        assert_eq!(load(&path).unwrap(), session);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
    #[test]
    fn cached_case_summary_ids_are_validated_on_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        let mut session = Session {
            cases: vec![crate::cases::CaseSummary {
                id: "aB0123456789cdef".repeat(2),
                name: "Example case".into(),
                documents: 3,
                color: None,
            }],
            ..Session::default()
        };
        save(&path, &session).unwrap();
        assert_eq!(load(&path).unwrap(), session);

        for invalid in [
            "a".repeat(31),
            "a".repeat(33),
            format!("{}g", "a".repeat(31)),
            format!("{}é", "a".repeat(30)),
            "../secret".into(),
        ] {
            session.cases[0].id = invalid.clone();
            save(&path, &session).unwrap();
            let error = load(&path).unwrap_err();
            assert_eq!(
                error.to_string(),
                "Invalid case id in session",
                "{invalid:?}"
            );
        }
    }
    #[test]
    fn a_vault_tab_round_trips_by_id_alone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        let id = VaultId::parse(&"c0".repeat(16)).unwrap();
        let mut tab = TabRecord {
            name: Some("Contract.md".into()),
            caret: 5,
            ..TabRecord::default()
        };
        tab.set_document(DocumentId::Vault(id.clone()));
        let session = Session {
            tabs: vec![tab],
            ..Session::default()
        };
        save(&path, &session).unwrap();
        let json = std::fs::read_to_string(&path).unwrap();
        assert!(json.contains(id.as_str()) && !json.contains("Contract"));
        let loaded = load(&path).unwrap();
        assert_eq!(loaded, session);
        assert_eq!(loaded.version, 2);
        assert_eq!(loaded.tabs[0].document(), Some(DocumentId::Vault(id)));
        assert!(loaded.tabs[0].same_document(&session.tabs[0]));
    }

    #[test]
    fn a_vault_tab_with_a_bad_id_or_a_path_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        let tab = |document: &str, markdown: &str| {
            format!(
                r#"{{"version":2,"tabs":[{{"document":"{document}","markdown":{markdown:?}}}],"active":0,"sidebar_visible":true}}"#
            )
        };
        let good = "ab".repeat(16);
        std::fs::write(&path, tab(&good, "")).unwrap();
        assert!(load(&path).is_ok());
        let elsewhere = dir.path().join("note.md").display().to_string();
        for json in [
            tab(&"AB".repeat(16), ""),
            tab("../../secret", ""),
            tab(&good[1..], ""),
            tab(&good, &elsewhere),
        ] {
            std::fs::write(&path, &json).unwrap();
            assert!(load(&path).is_err(), "{json}");
        }
    }

    #[test]
    fn a_version_1_session_keeps_its_layout_but_not_its_file_tabs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        let session = Session {
            version: 1,
            tabs: vec![
                TabRecord {
                    markdown: dir.path().join("note.md"),
                    ..TabRecord::default()
                },
                TabRecord {
                    source_only: true,
                    attachment: Some(dir.path().join("scan.pdf")),
                    ..TabRecord::default()
                },
                TabRecord {
                    blank: true,
                    name: Some("Untitled 2.md".into()),
                    ..TabRecord::default()
                },
            ],
            active: 1,
            theme_choice: ThemeChoice::Dark,
            ..Session::default()
        };
        save(&path, &session).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.version, 2);
        assert_eq!(loaded.tabs, vec![session.tabs[2].clone()]);
        assert_eq!(loaded.active, 0);
        assert_eq!(loaded.theme_choice, ThemeChoice::Dark);
    }

    #[test]
    fn corrupt_or_future_manifest_is_not_silently_accepted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        std::fs::write(&path, b"{broken").unwrap();
        assert!(load(&path).is_err());
        let session = Session {
            version: 99,
            ..Session::default()
        };
        save(&path, &session).unwrap();
        assert!(load(&path).is_err());
    }
}
