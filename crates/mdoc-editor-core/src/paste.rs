//! Paste normalisation: what pasted plain text becomes before it is typed in.

/// Where a paste lands.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PasteTarget {
    /// A plain-text field: the paste is kept exactly (line endings aside).
    pub plain: bool,
    /// The caret is in a table cell.
    pub table_cell: bool,
    /// The editor shows Markdown styling (not raw source).
    pub markdown: bool,
}

/// The text a paste of `text` inserts at `target`.
pub fn normalize_paste(text: String, target: PasteTarget) -> String {
    // Normalize foreign line endings — a Windows/browser copy
    // carries \r\n (or bare \r), and a literal \r in the buffer
    // garbles rendering + desyncs the \n-based column math.
    let mut text = if text.contains('\r') {
        text.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        text
    };
    // Inside a table cell, a raw paste of newlines/pipes would
    // break the `| … |` row markup (Enter is guarded the same
    // way): flatten newlines and escape pipes so the paste stays
    // one cell's content.
    if target.plain {
        // A plain-text field keeps the paste exactly.
    } else if target.table_cell && text.contains(['\n', '|']) {
        // Unescape-then-escape so text already carrying `\|`
        // doesn't double up into `\\|` (an escaped backslash
        // followed by a live separator).
        text = text
            .trim_end_matches('\n')
            .replace('\n', " ")
            .replace("\\|", "|")
            .replace('|', "\\|");
    } else if target.markdown && text.contains("$$") {
        // Words-attached `$$` fences / words-mixed pairs in pasted
        // text split onto their own lines (issue #54) — same
        // normalization typing gets.
        if let std::borrow::Cow::Owned(n) = mdoc_markdown::syntax::normalize_math_fences(&text) {
            text = n;
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_endings_cells_and_math_fences() {
        let md = PasteTarget {
            markdown: true,
            ..PasteTarget::default()
        };
        assert_eq!(normalize_paste("a\r\nb\rc".into(), md), "a\nb\nc");
        let cell = PasteTarget {
            table_cell: true,
            ..md
        };
        assert_eq!(normalize_paste("a|b\\|c\nd\n".into(), cell), "a\\|b\\|c d");
        let plain = PasteTarget {
            plain: true,
            table_cell: true,
            markdown: true,
        };
        assert_eq!(normalize_paste("a|b\n".into(), plain), "a|b\n");
        assert_eq!(
            normalize_paste("x $$a$$ y".into(), md),
            mdoc_markdown::syntax::normalize_math_fences("x $$a$$ y")
        );
    }
}
