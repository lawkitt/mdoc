//! The Vault: mdoc's one encrypted store (ADR 0125, #1091 Decisions 10–21).
//! A folder of sealed object files under an injected 256-bit key, with a
//! manifest that only caches their summaries:
//!
//! ```text
//! <vault root>/
//!   .lock                    advisory lock, held only for a write
//!   manifest.enc             cache of summaries
//!   user-lists.enc           user-wide Always and Never lists
//!   projects/<id>.enc        Project record
//!   docs/<id>/doc.enc        Document record
//! ```
//!
//! Originals (`docs/<id>/original.enc`) and replies (`replies/<id>.enc`)
//! join in later items, each with its own [`ObjectKind`]. The caller
//! supplies the key; this crate never asks the OS for one.
//!
//! No AI text is stored: [`Vault::text_for_ai`] computes it from a
//! Document's Markdown on every outbound read, after checking it against
//! its Project's names as they are then ([`Readiness`]).
#![forbid(unsafe_code)]

mod manifest;
mod mapping;
mod records;
mod seal;
mod vault;

pub use mapping::{Identity, Mapping, NotName};
pub use records::{
    Colour, Document, DocumentOrigin, DocumentSummary, NewDocument, NewProject, Project,
    ProjectSummary, SourceKind, UserLists,
};
pub use vault::{
    Fault, FaultHook, ManifestCheck, MoveUndo, Moved, ProjectDocuments, Readiness, Saved, Stage,
    Vault, WriteStep,
};

use std::io;

/// What a sealed file holds; its byte is authenticated with the id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ObjectKind {
    Manifest,
    Project,
    Document,
    UserLists,
}

impl ObjectKind {
    /// The header byte. Reserved for later items: 4 AI Reply, 6 an
    /// original's wrapped key.
    pub(crate) fn byte(self) -> u8 {
        match self {
            Self::Manifest => 1,
            Self::Project => 2,
            Self::Document => 3,
            Self::UserLists => 5,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Manifest => "manifest",
            Self::Project => "Project",
            Self::Document => "Document",
            Self::UserLists => "user-wide lists",
        }
    }
}

impl std::fmt::Display for ObjectKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("Could not read or write the Vault: {0}")]
    Io(#[from] io::Error),
    /// Nothing in the Vault opens with this key, and it holds objects.
    #[error("The Vault was sealed with a different key.")]
    WrongKey,
    #[error("Invalid id.")]
    InvalidId,
    #[error("No {kind} with id {id}.")]
    NotFound { kind: ObjectKind, id: String },
    #[error("The {kind} {id} is damaged or was saved with a different key.")]
    Damaged { kind: ObjectKind, id: String },
    /// A move into the Project, or into Unfiled, the document is in.
    #[error("The document is there already.")]
    AlreadyThere,
    /// A move could not line the document's names up with its new
    /// mapping; nothing was written.
    #[error("The document's names could not be lined up: {0}")]
    Names(String),
    /// Copy for AI, Share, the shelf or MCP asked for a Document marked
    /// "exclude from AI".
    #[error("The document is excluded from AI.")]
    ExcludedFromAi,
    /// An outbound read of a Document that is not Ready now.
    #[error("The document is not ready for AI ({0:?}).")]
    NotReady(Readiness),
    /// A fault hook stopped a write; the handle writes nothing more.
    #[error("The Vault handle stopped at an injected fault.")]
    Crashed,
}

/// A Vault id: 32 lowercase hex digits, so two ids never name one file on
/// a case-insensitive file system.
pub(crate) fn is_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// A new random id: 16 random bytes in lowercase hex.
pub(crate) fn new_id() -> io::Result<String> {
    use aes_gcm::aead::Generate;
    let bytes =
        <[u8; 16]>::try_generate().map_err(|_| io::Error::other("Could not generate an id"))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
