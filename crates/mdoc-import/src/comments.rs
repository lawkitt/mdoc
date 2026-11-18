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

impl Comment {
    /// The `index`th of a PDF's annotations, as pdf-inspector lists them: its
    /// id is that index, so a reply's `in_reply_to` names its parent; the
    /// passage is the marked text, and the date reads as ISO 8601.
    pub fn from_pdf(index: usize, annotation: &pdf_inspector::PdfAnnotation) -> Self {
        Self {
            id: index.to_string(),
            author: annotation.author.clone().unwrap_or_default(),
            date: annotation
                .date
                .as_deref()
                .map(|date| pdf_date(date).unwrap_or_else(|| date.to_owned()))
                .unwrap_or_default(),
            text: annotation.text.trim_end().to_owned(),
            anchor_text: annotation.marked_text.clone().unwrap_or_default(),
            parent: annotation.in_reply_to.map(|parent| parent.to_string()),
            resolved: annotation.resolved,
        }
    }
}

/// A PDF date (`D:YYYYMMDDHHmmSSOHH'mm'`, everything after the year
/// optional) as ISO 8601; `None` when it is not one.
fn pdf_date(date: &str) -> Option<String> {
    let date = date.strip_prefix("D:").unwrap_or(date);
    // The year, then month, day, hour, minute and second, each optional in
    // turn and two digits.
    let digits = date.bytes().take_while(u8::is_ascii_digit).count();
    if !(4..=14).contains(&digits) || digits % 2 != 0 {
        return None;
    }
    let part = |at: usize, default: &'static str| {
        if at < digits {
            &date[at..at + 2]
        } else {
            default
        }
    };
    let zone = match &date[digits..] {
        z if z.starts_with('Z') => "Z".to_owned(),
        z if z.starts_with(['+', '-']) => {
            let offset: String = z[1..].chars().filter(char::is_ascii_digit).collect();
            match offset.get(..2) {
                Some(hours) => format!("{}{hours}:{}", &z[..1], offset.get(2..4).unwrap_or("00")),
                None => String::new(),
            }
        }
        _ => String::new(),
    };
    Some(format!(
        "{}-{}-{}T{}:{}:{}{zone}",
        &date[..4],
        part(4, "01"),
        part(6, "01"),
        part(8, "00"),
        part(10, "00"),
        part(12, "00"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_inspector::{AnnotationKind, AnnotationRect, PdfAnnotation};

    #[test]
    fn pdf_dates_read_as_iso() {
        assert_eq!(
            pdf_date("D:20261118093000+07'00'").as_deref(),
            Some("2026-11-18T09:30:00+07:00")
        );
        assert_eq!(
            pdf_date("D:20260919").as_deref(),
            Some("2026-09-19T00:00:00")
        );
        assert_eq!(
            pdf_date("D:202609191005Z").as_deref(),
            Some("2026-09-19T10:05:00Z")
        );
        assert_eq!(pdf_date("2026").as_deref(), Some("2026-01-01T00:00:00"));
        assert_eq!(pdf_date("D:2026091"), None);
        assert_eq!(pdf_date("yesterday"), None);
        assert_eq!(pdf_date(""), None);
    }

    #[test]
    fn an_annotation_reads_as_a_comment() {
        let annotation = PdfAnnotation {
            page: 1,
            kind: AnnotationKind::Highlight,
            author: Some("Ann Lee".into()),
            date: Some("D:20260919100000Z".into()),
            text: "Is this cap right?\n".into(),
            rect: AnnotationRect {
                x: 0.,
                y: 0.,
                width: 1.,
                height: 1.,
            },
            quads: Vec::new(),
            in_reply_to: Some(2),
            resolved: true,
            marked_text: Some("the cap".into()),
        };
        assert_eq!(
            Comment::from_pdf(5, &annotation),
            Comment {
                id: "5".into(),
                author: "Ann Lee".into(),
                date: "2026-09-19T10:00:00Z".into(),
                text: "Is this cap right?".into(),
                anchor_text: "the cap".into(),
                parent: Some("2".into()),
                resolved: true,
            }
        );
        let bare = PdfAnnotation {
            author: None,
            date: Some("soon".into()),
            marked_text: None,
            in_reply_to: None,
            ..annotation
        };
        let comment = Comment::from_pdf(0, &bare);
        assert_eq!(
            (
                comment.author.as_str(),
                comment.date.as_str(),
                comment.parent
            ),
            ("", "soon", None)
        );
    }
}
