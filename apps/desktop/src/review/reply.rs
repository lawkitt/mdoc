//! Restore in place: the Share panel's "AI reply" box restores a
//! pasted reply at once, with Restore's matching rules, and the
//! view switcher's read-only Reply segment shows the result, following the
//! mapping live. A reply in the box is saved to the Vault once it settles
//! (the workspace saves it); edits while the box holds it replace that
//! saved reply, and clearing the box keeps it.
mod actions;
mod state;
use super::restore::UNRESOLVED_ID;
use mdoc_pii::restore::RestoredText;
pub(crate) use state::{ReplyBox, ReplyEffect};

/// A saved reply as Share lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SavedReplyRow {
    pub id: String,
    pub title: String,
    /// Where it came from and when.
    pub detail: String,
}

/// A restored reply's marks: restored values in the original's gold,
/// unresolved tokens red, as the restore tab marked them.
pub(crate) fn reply_marks(
    restored: &RestoredText,
    theme: crate::ui::style::Theme,
) -> Vec<mdoc_editor::SourceAnnotation> {
    let gold = theme.proposed();
    let red = crate::ui::style::markdown_style(theme).alert_caution;
    let mark = |id, range, accent: gpui::Hsla| mdoc_editor::SourceAnnotation {
        id,
        range,
        color: gpui::Hsla { a: 0.16, ..accent },
        active_color: gpui::Hsla { a: 0.34, ..accent },
        border: gpui::Hsla { a: 0.5, ..accent },
        text_color: Some(accent),
        underline: None,
        dotted: false,
    };
    (restored.restored.iter().enumerate())
        .map(|(i, (range, _))| mark(i as u64, range.clone(), gold))
        .chain(
            (restored.unresolved.iter())
                .map(|range| mark(UNRESOLVED_ID | range.start as u64, range.clone(), red)),
        )
        .collect()
}
