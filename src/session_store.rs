//! Small, versioned, paths-only session manifest. No document content is stored.
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

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TabRecord {
    pub markdown: PathBuf,
    pub source_only: bool,
    pub attachment: Option<PathBuf>,
    pub preview_visible: bool,
    pub caret: usize,
    pub scroll_y: f32,
    pub preview_page: usize,
    pub preview_zoom: Option<f32>,
    pub preview_fit: PreviewFit,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub version: u32,
    pub tabs: Vec<TabRecord>,
    pub active: usize,
    pub sidebar_visible: bool,
}
impl Default for Session {
    fn default() -> Self {
        Self {
            version: 1,
            tabs: Vec::new(),
            active: 0,
            sidebar_visible: true,
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
    let session: Session = serde_json::from_slice(&bytes)?;
    if session.version != 1 {
        return Err(io::Error::other("Unsupported session version"));
    }
    if session.tabs.iter().any(|tab| {
        !(if tab.source_only {
            tab.markdown.as_os_str().is_empty()
                && tab
                    .attachment
                    .as_ref()
                    .is_some_and(|path| path.is_absolute())
        } else {
            tab.markdown.is_absolute()
        }) || tab
            .attachment
            .as_ref()
            .is_some_and(|path| !path.is_absolute())
            || !tab.scroll_y.is_finite()
            || tab.preview_zoom.is_some_and(|z| !z.is_finite())
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
pub fn identity(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| {
        let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        absolute
            .parent()
            .and_then(|parent| parent.canonicalize().ok())
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
            markdown: dir.path().join("note.md"),
            source_only: false,
            attachment: Some(dir.path().join("source.docx")),
            preview_visible: false,
            caret: 8,
            scroll_y: -120.,
            preview_page: 4,
            preview_zoom: Some(1.25),
            preview_fit: PreviewFit::Manual,
        });
        save(&path, &session).unwrap();
        assert_eq!(load(&path).unwrap(), session);
        session.tabs.clear();
        save(&path, &session).unwrap();
        assert_eq!(load(&path).unwrap(), session);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
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
