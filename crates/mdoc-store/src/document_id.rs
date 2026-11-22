//! What names a document across the desktop's live views, its session and
//! its lookups. Today a document is a Markdown file; the Vault adds its own
//! opaque id as a second kind, which the session stores instead of a path
//! (ADR 0125; #1091, decision 26).
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum DocumentId {
    /// A Markdown file on disk, by the path normalized when it was opened or
    /// saved ([`crate::session_store::identity`]).
    File(PathBuf),
}

impl DocumentId {
    /// The file this document is, when it is one.
    pub fn file(&self) -> Option<&Path> {
        match self {
            Self::File(path) => Some(path),
        }
    }

    /// The file this document is, when it is one, taking it.
    pub fn into_file(self) -> Option<PathBuf> {
        match self {
            Self::File(path) => Some(path),
        }
    }
}
