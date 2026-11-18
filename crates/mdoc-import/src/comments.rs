//! Comments a source document carries, as the forks extract them
//! (originals spec, Extraction): AnyDoc reads them from DOCX, ODT and RTF,
//! and pdf-inspector reports a PDF's annotations that carry text. mdoc
//! keeps no comment parser of its own. Comments never enter the Markdown;
//! the left-out line counts them and the Source rail's cards show them,
//! both from the same extraction.

/// One comment, as AnyDoc reads it.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Comment {
    /// Unique in its document: DOCX `w:id`, the render copy's anchors carry
    /// it too.
    pub id: String,
    /// Empty when the document names no one.
    pub author: String,
    /// As written (DOCX and ODT usually ISO 8601); empty when absent.
    pub date: String,
    /// Plain text: paragraphs joined by `\n`, tabs as `\t`.
    pub text: String,
    /// The commented passage's text; a point comment's paragraph.
    pub anchor_text: String,
    /// The `id` of the comment this one replies to, in the same list.
    pub parent: Option<String>,
    /// Marked resolved (done) in the document.
    pub resolved: bool,
}

impl From<anydoc::model::Comment> for Comment {
    fn from(comment: anydoc::model::Comment) -> Self {
        let anydoc::model::Comment {
            id,
            author,
            date,
            text,
            anchor_text,
            parent_id,
            resolved,
            ..
        } = comment;
        Self {
            id,
            author,
            date,
            text,
            anchor_text,
            parent: parent_id,
            resolved,
        }
    }
}
