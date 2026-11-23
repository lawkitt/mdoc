//! Project, Document and AI Reply records (#1091 Decision 16), stored as
//! JSON.
//! Later items add their fields with `#[serde(default)]`.
use crate::mapping::{Binding, Mapping};
use mdoc_pii::{Category, saved::SavedReview};
use mdoc_store::lists::Lists;
use serde::{Deserialize, Serialize};

/// Seconds since the Unix epoch.
pub(crate) fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// A Project: a named workspace whose Documents share one mapping.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    /// `None`: the colour [`Colour::for_id`] picks.
    #[serde(default)]
    pub colour: Option<Colour>,
    #[serde(default)]
    pub pinned: bool,
    /// Seconds since the Unix epoch.
    pub created: u64,
    /// Seconds since the Unix epoch of the last change through the Vault.
    pub updated: u64,
    /// Bumped on every change; a handle never writes one older than the
    /// last it wrote.
    pub revision: u64,
    /// The identities its Documents share.
    #[serde(default)]
    pub mapping: Mapping,
    /// Its Always and Never lists, from its Documents' manual marks and
    /// Keeps; they win over the user-wide ones (`Lists::effective`).
    #[serde(default)]
    pub lists: Lists,
    /// Category tokens scans in its Documents skip.
    #[serde(default)]
    pub skip: Vec<String>,
    /// Dismissed merge hints, as the identities' primary wordings (shorter,
    /// fuller).
    #[serde(default)]
    pub dismissed_hints: Vec<(String, String)>,
}

impl Project {
    /// The categories its scans skip.
    pub fn skipped(&self) -> std::collections::HashSet<Category> {
        Category::ALL
            .into_iter()
            .filter(|c| self.skip.iter().any(|s| s == c.token()))
            .collect()
    }

    /// Irreversibly discard the originals of the identity `alias` names,
    /// and the merge hints dismissed for them; its aliases stay reserved.
    /// `false` when there was nothing to forget.
    pub fn forget_identity(&mut self, alias: &str) -> bool {
        let Some(originals) = self.mapping.forget(alias) else {
            return false;
        };
        self.dismissed_hints
            .retain(|(from, into)| !originals.contains(from) && !originals.contains(into));
        true
    }
}

/// What a new Project starts with.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NewProject {
    pub name: String,
    pub colour: Option<Colour>,
    pub pinned: bool,
}

/// A Document: its Markdown and what the Vault keeps beside it. Not `Eq`:
/// its review holds detection scores.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub id: String,
    /// The Project it belongs to; `None`: Unfiled. Changed only by a move,
    /// never by an update or save.
    pub project: Option<String>,
    pub title: String,
    pub source_kind: SourceKind,
    #[serde(default)]
    pub origin: DocumentOrigin,
    /// Never offered to an AI (Copy for AI, the shelf, MCP).
    #[serde(default)]
    pub exclude_from_ai: bool,
    /// Seconds since the Unix epoch.
    pub created: u64,
    /// Seconds since the Unix epoch of the last change through the Vault.
    pub updated: u64,
    /// Bumped on every change; a handle never writes one older than the
    /// last it wrote.
    pub revision: u64,
    pub markdown: String,
    /// Imported with no text yet: the app converts its Source Original when
    /// it opens. Set only by [`crate::Vault::import_document`] with empty
    /// text; a move keeps it; a save of text clears it (the app clears it
    /// itself for a conversion that made no text), and no later save sets
    /// it again. Read it through [`Document::pending_conversion`].
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unconverted: bool,
    /// Scanned for names, so a scan that found nothing reopens as scanned.
    #[serde(default)]
    pub scanned: bool,
    /// The SHA-256, in hex, of the text last saved while Ready
    /// ([`Document::set_ready`]); readiness itself is checked when text
    /// leaves ([`crate::Vault::readiness`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ready_fingerprint: Option<String>,
    /// Its own identities while Unfiled; empty in a Project, whose mapping
    /// it shares.
    #[serde(default, skip_serializing_if = "Mapping::is_empty")]
    pub mapping: Mapping,
    /// The review as it stood at the last save (#1091 Decision 16), so a
    /// reopened Document needs no rescan; `None` until it has one. Its
    /// ranges are in `markdown` as saved with it: restore it only over that
    /// text (`mdoc_pii::Review::restore` refuses ranges that do not fit).
    /// A move clears it, as its text and mapping change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review: Option<SavedReview>,
    /// The Source Original kept in `original.enc`; `None` for a pasted or
    /// new document. Set only by [`crate::Vault::import_document`]: a save
    /// or update keeps it as stored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original: Option<Original>,
    /// What its review counted at the last save, so lists show it without
    /// opening it; `None` until a save says. A move clears it with the
    /// review.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counts: Option<ReviewCounts>,
}

impl Document {
    /// Whether its Source Original still waits to be converted: no text,
    /// and imported so ([`Document::unconverted`]) or, for a Document
    /// imported before that flag, never written since (revision 1).
    pub fn pending_conversion(&self) -> bool {
        self.original.is_some()
            && self.markdown.is_empty()
            && (self.unconverted || self.revision == 1)
    }
}

/// What the Document record knows of its Source Original; the sealed file
/// holds the source's bytes only (#1091 Decisions 14 and 16).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Original {
    /// The SHA-256, in lowercase hex, of the source's bytes: a repeated
    /// import of the same file finds this Document (#1091 Decision 7).
    pub sha256: String,
    /// The source's length in bytes.
    pub len: u64,
    #[serde(flatten)]
    pub info: OriginalInfo,
}

/// What the importer tells the Vault about a source beside its bytes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OriginalInfo {
    /// The file name it was imported from, without its folder.
    pub file_name: String,
    /// Password-protected: the original stays locked as imported
    /// (#1091 Decision 23).
    #[serde(default)]
    pub password_protected: bool,
    /// The import's warning, as shown when it was imported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub import_warning: Option<String>,
    /// The left-out line: what else the Markdown leaves out, counted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left_out: Option<String>,
}

/// A review's mentions by state, as the caller counted them at a save.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ReviewCounts {
    /// Proposals left.
    pub proposed: u64,
    pub replaced: u64,
    pub kept: u64,
}

/// What a new Document starts with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewDocument {
    /// `None`: Unfiled.
    pub project: Option<String>,
    pub title: String,
    pub source_kind: SourceKind,
    pub origin: DocumentOrigin,
    pub markdown: String,
}

/// How a document began in mdoc; the mobile core's `DocumentOrigin`, in
/// its wire form.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentOrigin {
    /// Imported, pasted or scanned, or a new document the user renamed.
    #[default]
    Ordinary,
    /// New document: named after its first line until the user renames it.
    New,
    /// Try a sample's document.
    Sample,
}

/// The file family a Document was made from, stored as a lowercase string.
/// The exact format comes from the original's file name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    Pdf,
    Word,
    Spreadsheet,
    Presentation,
    Ebook,
    Image,
    Scan,
    Markdown,
    /// Plain text, and pasted and new documents; [`DocumentOrigin`] tells
    /// them apart.
    Text,
}

/// The eight Project swatches, in hue order: the case store's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Colour {
    Red,
    Orange,
    Amber,
    Green,
    Teal,
    Blue,
    Violet,
    Pink,
}

impl Colour {
    pub const ALL: [Self; 8] = [
        Self::Red,
        Self::Orange,
        Self::Amber,
        Self::Green,
        Self::Teal,
        Self::Blue,
        Self::Violet,
        Self::Pink,
    ];

    /// Its hue in degrees (0..360).
    pub fn hue(self) -> u32 {
        match self {
            Self::Red => 0,
            Self::Orange => 30,
            Self::Amber => 45,
            Self::Green => 120,
            Self::Teal => 175,
            Self::Blue => 220,
            Self::Violet => 270,
            Self::Pink => 325,
        }
    }

    /// The swatch nearest a hue taken from the id: a Project's colour until
    /// the user picks one.
    pub fn for_id(id: &str) -> Self {
        let hue = u32::from_str_radix(id.get(..6).unwrap_or(""), 16).unwrap_or(0) % 360;
        Self::ALL
            .into_iter()
            .min_by_key(|colour| {
                let distance = hue.abs_diff(colour.hue());
                distance.min(360 - distance)
            })
            .unwrap_or(Self::Red)
    }
}

/// What the Project tree and search need of a Project, kept in the
/// manifest so listing opens no record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub colour: Option<Colour>,
    pub pinned: bool,
    pub created: u64,
    pub updated: u64,
}

impl ProjectSummary {
    pub fn of(project: &Project) -> Self {
        Self {
            id: project.id.clone(),
            name: project.name.clone(),
            colour: project.colour,
            pinned: project.pinned,
            created: project.created,
            updated: project.updated,
        }
    }
}

/// What the Project tree and search need of a Document; no text.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentSummary {
    pub id: String,
    pub project: Option<String>,
    pub title: String,
    pub source_kind: SourceKind,
    pub origin: DocumentOrigin,
    pub exclude_from_ai: bool,
    pub scanned: bool,
    pub created: u64,
    pub updated: u64,
    /// The original's SHA-256 in hex, for the repeated-import check.
    #[serde(default)]
    pub source_sha256: Option<String>,
    /// [`Document::counts`].
    #[serde(default)]
    pub counts: Option<ReviewCounts>,
}

impl DocumentSummary {
    pub fn of(document: &Document) -> Self {
        Self {
            id: document.id.clone(),
            project: document.project.clone(),
            title: document.title.clone(),
            source_kind: document.source_kind,
            origin: document.origin,
            exclude_from_ai: document.exclude_from_ai,
            scanned: document.scanned,
            created: document.created,
            updated: document.updated,
            source_sha256: (document.original.as_ref()).map(|original| original.sha256.clone()),
            counts: document.counts,
        }
    }
}

/// What an AI Reply belongs to.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplyOwner {
    /// It restores with the Project's mapping.
    Project(String),
    /// It follows the Document: it restores with the Document's own mapping
    /// while Unfiled and with its Project's once moved into one (#1091
    /// Decision 4).
    Document(String),
}

/// Where the text an AI Reply answers was shared from; it decides the
/// reply's [`ReplyOwner`] (`Vault::save_shared_reply`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SharedFrom {
    /// A Project as a whole, from its page.
    Project(String),
    /// A Document: its Project's when it is in one, else Unfiled.
    Document(String),
}

/// An AI Reply (#1091 Decisions 16 and 18): the pseudonymized text as
/// received and what each alias in it meant when it was saved. Written once.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reply {
    pub id: String,
    pub owner: ReplyOwner,
    pub title: String,
    /// The app it came from, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_app: Option<String>,
    /// As received: aliases, never restored text.
    pub markdown: String,
    /// Seconds since the Unix epoch.
    pub created: u64,
    pub bindings: Vec<Binding>,
}

/// What a new AI Reply starts with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewReply {
    pub owner: ReplyOwner,
    pub title: String,
    pub source_app: Option<String>,
    /// As received.
    pub markdown: String,
}

/// What a Project page and Home need of an AI Reply; no text.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplySummary {
    pub id: String,
    pub owner: ReplyOwner,
    pub title: String,
    pub source_app: Option<String>,
    pub created: u64,
}

impl ReplySummary {
    pub fn of(reply: &Reply) -> Self {
        Self {
            id: reply.id.clone(),
            owner: reply.owner.clone(),
            title: reply.title.clone(),
            source_app: reply.source_app.clone(),
            created: reply.created,
        }
    }
}

/// The user-wide Always and Never lists (Settings › Detection), sealed in
/// `user-lists.enc`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserLists {
    /// Bumped on every change; a handle never writes one older than the
    /// last it wrote.
    pub revision: u64,
    pub lists: Lists,
}
