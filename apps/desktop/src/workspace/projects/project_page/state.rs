//! The Project page's state: its segments, where a request to show it
//! lands, and the words its rows are built from (colours, kinds, counts,
//! dates). No GPUI.

use crate::workspace::projects::text;
use mdoc_vault::{Colour, SourceKind};

/// A swatch's name, as the page reads it out.
pub(crate) fn colour_name(colour: Colour) -> &'static str {
    match colour {
        Colour::Red => text::RED,
        Colour::Orange => text::ORANGE,
        Colour::Amber => text::AMBER,
        Colour::Green => text::GREEN,
        Colour::Teal => text::TEAL,
        Colour::Blue => text::BLUE,
        Colour::Violet => text::VIOLET,
        Colour::Pink => text::PINK,
    }
}

/// One of the page's three views of its Project.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Segment {
    Documents,
    Replies,
    Names,
    /// The Project's AI activity: only with "AI connector (preview)" on.
    Activity,
}

impl Segment {
    pub(crate) const ALL: [Segment; 4] = [
        Segment::Documents,
        Segment::Replies,
        Segment::Names,
        Segment::Activity,
    ];

    pub(super) fn label(self) -> &'static str {
        match self {
            Segment::Documents => text::DOCUMENTS,
            Segment::Replies => text::REPLIES,
            Segment::Names => text::NAMES,
            Segment::Activity => text::AI_ACTIVITY,
        }
    }

    pub(super) fn control(self) -> &'static str {
        match self {
            Segment::Documents => "project-segment-documents",
            Segment::Replies => "project-segment-replies",
            Segment::Names => "project-segment-names",
            Segment::Activity => "project-segment-activity",
        }
    }
}

/// Where a request to show the page lands.
#[derive(Clone, Copy)]
pub(in crate::workspace) enum Place {
    /// Documents, focus on the colour.
    Header,
    Rename,
    /// The swatches open.
    Colour,
    /// Names, at Categories to detect.
    Categories,
    /// The page, then the delete question.
    Delete,
}

/// A document's file family, as its row shows it.
pub(in crate::workspace) fn kind_label(kind: SourceKind) -> &'static str {
    match kind {
        SourceKind::Pdf => text::PDF,
        SourceKind::Word => text::DOC,
        SourceKind::Spreadsheet => text::XLS,
        SourceKind::Presentation => text::PPT,
        SourceKind::Ebook => text::BOOK,
        SourceKind::Image => text::IMG,
        SourceKind::Scan => text::SCAN,
        SourceKind::Markdown => text::MD,
        SourceKind::Text => text::TXT,
    }
}

/// "4 documents · 2 replies · 11 names · created 3 Nov".
pub(super) fn project_counts(
    documents: usize,
    replies: usize,
    names: usize,
    created: u64,
) -> String {
    text::project_counts(documents, replies, names, &created_date(created))
}

/// Gregorian civil date from Unix days; independent of locale and OS access.
pub(in crate::workspace) fn created_date(seconds: u64) -> String {
    let days = (seconds / 86400).min(i32::MAX as u64) as i64 + 719468;
    let era = days / 146097;
    let doe = days - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    text::day_month(day, (month - 1) as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_counts_line_says_each_count_in_words() {
        assert_eq!(
            project_counts(1, 0, 2, 1791763200),
            "1 document · 0 replies · 2 names · created 12 Oct"
        );
        assert_eq!(
            project_counts(3, 1, 1, 0),
            "3 documents · 1 reply · 1 name · created 1 Jan"
        );
    }
}
