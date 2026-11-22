//! Project and Document records (#1091 Decision 16), stored as JSON.
//! Later items add their fields with `#[serde(default)]`: identities and
//! the Always and Never lists (item 4), the review state (item 2), the
//! source hash and the original's metadata (item 3).
use serde::{Deserialize, Deserializer, Serialize, Serializer};

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
}

/// What a new Project starts with.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NewProject {
    pub name: String,
    pub colour: Option<Colour>,
    pub pinned: bool,
}

/// A Document: its Markdown and what the Vault keeps beside it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Scanned for names, so a scan that found nothing reopens as scanned.
    #[serde(default)]
    pub scanned: bool,
    /// The SHA-256, in hex, of the text last saved while Ready.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ready_fingerprint: Option<String>,
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

/// How a document began in mdoc.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DocumentOrigin {
    /// Imported, pasted or scanned, or a new document the user renamed.
    #[default]
    Ordinary,
    /// New document: named after its first line until the user renames it.
    New,
    /// Try a sample's document.
    Sample,
}

/// What a Document was made from: the iOS app's kinds, Markdown, and any
/// other format by its lowercase file extension. Stored as one lowercase
/// string (`"pdf"`, `"other:xlsx"`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SourceKind {
    Pdf,
    Docx,
    Image,
    Scan,
    /// Plain or pasted text.
    Text,
    Markdown,
    /// Another format, by its lowercase extension.
    Other(String),
}

impl SourceKind {
    pub fn as_str(&self) -> std::borrow::Cow<'_, str> {
        match self {
            Self::Pdf => "pdf".into(),
            Self::Docx => "docx".into(),
            Self::Image => "image".into(),
            Self::Scan => "scan".into(),
            Self::Text => "text".into(),
            Self::Markdown => "markdown".into(),
            Self::Other(extension) => format!("other:{extension}").into(),
        }
    }

    pub fn parse(text: &str) -> Self {
        match text {
            "pdf" => Self::Pdf,
            "docx" => Self::Docx,
            "image" => Self::Image,
            "scan" => Self::Scan,
            "text" => Self::Text,
            "markdown" => Self::Markdown,
            other => Self::Other(
                other
                    .strip_prefix("other:")
                    .unwrap_or(other)
                    .to_ascii_lowercase(),
            ),
        }
    }
}

impl Serialize for SourceKind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.as_str())
    }
}

impl<'de> Deserialize<'de> for SourceKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::parse(&String::deserialize(deserializer)?))
    }
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
}

impl DocumentSummary {
    pub fn of(document: &Document) -> Self {
        Self {
            id: document.id.clone(),
            project: document.project.clone(),
            title: document.title.clone(),
            source_kind: document.source_kind.clone(),
            origin: document.origin,
            exclude_from_ai: document.exclude_from_ai,
            scanned: document.scanned,
            created: document.created,
            updated: document.updated,
        }
    }
}
