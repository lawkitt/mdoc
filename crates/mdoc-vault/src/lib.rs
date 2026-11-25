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
//!   docs/<id>/original.enc   the Document's Source Original, as imported
//!   replies/<id>.enc         AI Reply record
//!   activity/<id>.enc        a Project's AI activity, by the Project's id
//!   staging/.import-*        an import sealed, not yet published
//! ```
//!
//! The caller supplies the key; this crate never asks the OS for one.
//!
//! No AI text is stored: [`Vault::text_for_ai`] computes it from a
//! Document's Markdown on every outbound read, after checking it against
//! its Project's names as they are then ([`Readiness`]).
#![forbid(unsafe_code)]

mod manifest;
mod mapping;
mod original;
mod records;
pub mod recovery;
mod seal;
mod vault;

pub use mapping::{Binding, Identity, Mapping, NotName};
pub use original::{OriginalReader, source_sha256};
pub use records::{
    Colour, Document, DocumentOrigin, DocumentSummary, NewDocument, NewProject, NewReply, Original,
    OriginalInfo, Project, ProjectSummary, Reply, ReplyOwner, ReplySummary, ReviewCounts,
    SharedFrom, SourceKind, UserLists,
};
pub use vault::{
    ACTIVITY_KEPT, AiActivity, AiEvent, AiRead, AiText, Delivered, Exported, Fault, FaultHook,
    ManifestCheck, MoveUndo, Moved, NewDelivery, ORIGINALS, ProjectDocuments, ReadAt, Readiness,
    ReplyHandle, Saved, Stage, StagedImport, Vault, WriteStep,
};

use std::io;

/// What a sealed file holds; its byte is authenticated with the id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ObjectKind {
    Manifest,
    Project,
    Document,
    Reply,
    UserLists,
    /// A Source Original's file key, sealed at the head of `original.enc`.
    Original,
    /// A Project's AI activity (`activity/<project id>.enc`).
    Activity,
}

impl ObjectKind {
    /// The header byte.
    pub(crate) fn byte(self) -> u8 {
        match self {
            Self::Manifest => 1,
            Self::Project => 2,
            Self::Document => 3,
            Self::Reply => 4,
            Self::UserLists => 5,
            Self::Original => 6,
            Self::Activity => 7,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Manifest => "manifest",
            Self::Project => "Project",
            Self::Document => "Document",
            Self::Reply => "AI Reply",
            Self::UserLists => "user-wide lists",
            Self::Original => "original",
            Self::Activity => "AI activity",
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
    /// A viewing copy's name is not a bare file name, or its folder is
    /// inside the Vault's.
    #[error("A copy of an original needs a bare file name in a folder outside the Vault.")]
    InvalidCopyTarget,
    /// Export Project… was pointed at a folder inside the Vault.
    #[error("Documents can't be exported into the Vault's own folder.")]
    ExportInsideVault,
    /// A record sealed larger than the Vault reads back; nothing was written.
    #[error("The {kind} {id} is too large to store.")]
    TooLarge { kind: ObjectKind, id: String },
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
