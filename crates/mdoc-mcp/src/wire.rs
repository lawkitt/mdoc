//! What the shim and mdoc say over the pipe: one request line, one answer
//! line, JSON. mdoc answers through its connector service only
//! (`mdoc_connector::serve`); every sentence the model reads is the shim's
//! (`mcp.rs`), so no free text crosses but checked document text, labels
//! and headings.
use serde::{Deserialize, Serialize};

/// The wire's version; mdoc refuses any other ([`Refusal::Version`]).
pub const VERSION: u32 = 1;

/// The longest line either side reads.
pub const MAX_LINE: u64 = 16 << 20;

/// One call, from the AI app `app` (its MCP `clientInfo.name`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    pub v: u32,
    pub app: String,
    #[serde(flatten)]
    pub call: Call,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "call", rename_all = "snake_case")]
pub enum Call {
    ListDocuments,
    GetDocument {
        doc_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        part: Option<usize>,
    },
    Deliver {
        kind: Kind,
        #[serde(default)]
        title: Option<String>,
        read: Vec<String>,
        markdown: String,
    },
}

/// Where a delivery lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Reply,
    Document,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Answer {
    Ok(Done),
    Refused(Refusal),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Done {
    Documents {
        documents: Vec<Listed>,
    },
    Text {
        doc_id: String,
        label: String,
        text: String,
    },
    Outline {
        doc_id: String,
        label: String,
        note: String,
        chars: usize,
        words: usize,
        parts: Vec<PartRow>,
    },
    Part {
        doc_id: String,
        label: String,
        part: usize,
        of: usize,
        headings: Vec<Heading>,
        text: String,
    },
    NeedsReview {
        doc_id: String,
        label: String,
        why: Why,
    },
    Delivered {
        reply: String,
        #[serde(default)]
        document: Option<String>,
        restored: usize,
        unresolved: Vec<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Listed {
    pub doc_id: String,
    pub label: String,
    pub chars: usize,
    pub words: usize,
    /// `None`: Ready.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub needs_review: Option<Why>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartRow {
    pub part: usize,
    pub chars: usize,
    pub headings: Vec<Heading>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Heading {
    pub level: u8,
    pub text: String,
}

/// Why a document is not Ready now.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "why", rename_all = "snake_case")]
pub enum Why {
    NotReviewed,
    ChangedSinceReview,
    NamesNotReplaced { count: usize },
}

/// A call mdoc refused, by code alone: the shim words it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum Refusal {
    NothingShared,
    NotShared,
    NoSuchPart {
        of: usize,
    },
    ShareChanged,
    NotRead,
    TitleNeeded,
    Empty,
    /// The Vault failed; its message stays in mdoc.
    Unavailable,
    BadRequest,
    Version,
}
