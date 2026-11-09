//! The phones' editing toolbar and formatting keyboard (ADR 0109): what
//! their buttons show at the caret, and the commands desktop has no
//! counterpart for: a format toggled at a caret, the list and task buttons,
//! links, a rule and a table skeleton.
//!
//! Like [`crate::commands`], each call takes the source and a UTF-8
//! selection (its `end` is the caret) and returns the edits; desktop's
//! editor does not call this module.

use std::ops::Range;

use mdoc_markdown::lines::{list_prefix, task_prefix};
use mdoc_markdown::syntax::links;

use crate::commands::InlineFormat;
use crate::markdown::fmt_marker_pairs;
use crate::text::{line_bounds, row_col};
use crate::turn_into::TurnKind;
use crate::{BlockScan, Edit, EditResult, TAB_INDENT, format};

/// What the toolbar shows for the selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaretState {
    /// The inline formats the caret is inside (or the selection lies
    /// within), which their buttons show on.
    pub formats: Vec<InlineFormat>,
    /// Inline formats apply here: not in code or math.
    pub can_format: bool,
    /// The caret's block kind, which "Turn into" shows selected.
    pub block: TurnKind,
    /// The caret's line is a bulleted or numbered item (not a task).
    pub list: bool,
    /// The caret's line is a task: whether it is checked.
    pub task: Option<bool>,
    /// Indent nests the caret's list item under the item above it.
    pub can_indent: bool,
    /// The caret's line has an indent to take away.
    pub can_outdent: bool,
    /// The caret is in a table the view draws.
    pub in_table: bool,
    /// The link holding or touching the caret.
    pub link: Option<LinkAt>,
}

/// A link at the caret: an inline `[text](url)` or a bare URL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkAt {
    /// The whole link in the source.
    pub range: Range<usize>,
    pub text: String,
    pub url: String,
    /// An `http` or `https` URL, the only kind the phone opens.
    pub web: bool,
}

fn sorted(selection: &Range<usize>) -> Range<usize> {
    selection.start.min(selection.end)..selection.start.max(selection.end)
}

/// The formats a formatting marker opens: by its text, so a heading's bold
/// body is not taken for bold.
fn marker_formats(marker: &str) -> &'static [InlineFormat] {
    use InlineFormat::*;
    let tag = marker.trim_start_matches('<').trim_end_matches('>');
    let tag = tag
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    match marker {
        "**" | "__" => &[Bold],
        "*" | "_" => &[Italic],
        "***" | "___" => &[Bold, Italic],
        "~~" | "~" => &[Strikethrough],
        "==" => &[Highlight],
        m if m.bytes().all(|b| b == b'`') => &[Code],
        m if m.starts_with('<') => match tag.as_str() {
            "b" | "strong" => &[Bold],
            "i" | "em" => &[Italic],
            "s" | "del" | "strike" => &[Strikethrough],
            "u" | "ins" => &[Underline],
            "mark" => &[Highlight],
            "code" => &[Code],
            _ => &[],
        },
        _ => &[],
    }
}

/// A formatting construct on one line, in source offsets.
struct Pair {
    open: Range<usize>,
    close: Range<usize>,
    formats: &'static [InlineFormat],
}

/// The caret line's formatting constructs that hold `selection`: a caret
/// between the markers (just inside counts), or a selection within the
/// whole construct. Innermost last.
fn pairs_holding(source: &str, selection: &Range<usize>) -> Vec<Pair> {
    let (start, end) = line_bounds(source, selection.start);
    if selection.end > end {
        return Vec::new();
    }
    let line = &source[start..end];
    let mut pairs: Vec<Pair> = fmt_marker_pairs(line)
        .into_iter()
        .map(|(o, c)| Pair {
            formats: marker_formats(&line[o.clone()]),
            open: o.start + start..o.end + start,
            close: c.start + start..c.end + start,
        })
        .filter(|p| {
            if selection.is_empty() {
                p.open.end <= selection.start && selection.start <= p.close.start
            } else {
                p.open.start <= selection.start
                    && selection.end <= p.close.end
                    && selection.start < p.close.start
                    && selection.end > p.open.end
            }
        })
        .collect();
    pairs.sort_by_key(|p| p.open.start);
    pairs
}

/// The link holding or touching the caret.
pub fn link_at(source: &str, caret: usize) -> Option<LinkAt> {
    let (start, end) = line_bounds(source, caret);
    let line = &source[start..end];
    let col = caret - start;
    let (range, url) = links(line)
        .into_iter()
        .find(|(r, _)| r.start <= col && col <= r.end)?;
    let text = if line[range.clone()].starts_with('[') {
        let close = line[range.start + 1..].find(']')? + range.start + 1;
        line[range.start + 1..close].replace("\\\\", "\\")
    } else {
        url.clone()
    };
    let scheme = url.split(':').next().unwrap_or("").to_ascii_lowercase();
    let web = url.contains("://") && (scheme == "http" || scheme == "https");
    Some(LinkAt {
        range: range.start + start..range.end + start,
        text,
        url,
        web,
    })
}

/// Whether the line above `line_start` (skipping blank lines) is a list
/// item a nested item can sit under.
fn item_above(source: &str, line_start: usize) -> bool {
    let mut lines = source[..line_start].lines().rev();
    lines
        .find(|l| !l.trim().is_empty())
        .is_some_and(|l| list_prefix(l).is_some())
}

/// What the toolbar shows for `selection`.
pub fn caret_state(source: &str, selection: Range<usize>, rendered: bool) -> CaretState {
    let sel = sorted(&selection);
    let caret = selection.end.min(source.len());
    let scan = BlockScan::new(source);
    let blocks = scan.view();
    let row = row_col(source, caret).0;
    let (line_start, line_end) = line_bounds(source, caret);
    let line = &source[line_start..line_end];
    let in_table = blocks.row_in_table(row) && !blocks.row_is_literal(source, row);
    let literal = blocks.row_is_literal(source, row);
    let mut formats: Vec<InlineFormat> = Vec::new();
    for pair in pairs_holding(source, &sel) {
        for format in pair.formats {
            if !formats.contains(format) {
                formats.push(*format);
            }
        }
    }
    let task = task_prefix(line).map(|(_, _, checked)| checked);
    let item = list_prefix(line).is_some();
    let table_cell = rendered && in_table;
    CaretState {
        formats,
        can_format: !literal,
        block: crate::commands::block_kind(source, selection),
        list: item && task.is_none(),
        task,
        can_indent: item && !table_cell && item_above(source, line_start),
        can_outdent: !table_cell && format::outdent(source, caret, TAB_INDENT).is_some(),
        in_table,
        link: link_at(source, caret),
    }
}

/// `offset` after `edits` (sorted, in the offsets before them): an offset
/// inside a replaced range lands at the end of its replacement.
fn mapped(offset: usize, edits: &[Edit]) -> usize {
    let mut delta: isize = 0;
    for edit in edits {
        if offset < edit.range.start {
            break;
        }
        if offset < edit.range.end {
            return (edit.range.start as isize + delta) as usize + edit.text.len();
        }
        delta += edit.text.len() as isize - edit.range.len() as isize;
    }
    (offset as isize + delta) as usize
}

/// The markers `format` leaves when it comes off a construct that opens
/// with `marker` (`***` loses `**` to Italic, `*` to Bold).
fn remaining(marker: &str, format: InlineFormat) -> String {
    match (marker, format) {
        ("***", InlineFormat::Bold) => "*".into(),
        ("___", InlineFormat::Bold) => "_".into(),
        ("***", InlineFormat::Italic) => "**".into(),
        ("___", InlineFormat::Italic) => "__".into(),
        _ => String::new(),
    }
}

/// Toggle `format` the phone's way. Inside a construct of that format (the
/// caret, or a selection within it) its markers come off; otherwise a
/// selection toggles as desktop's does, a caret inside a word wraps the
/// word, and a caret elsewhere gets an empty pair to type into. `None` on
/// a blank selection, and in code or math, where markers are literal text
/// (the toolbar's button is off there, and ⌘B does nothing).
pub fn toggle_format(
    source: &str,
    selection: Range<usize>,
    format: InlineFormat,
) -> Option<EditResult> {
    let sel = sorted(&selection);
    let sel = sel.start.min(source.len())..sel.end.min(source.len());
    let scan = BlockScan::new(source);
    let (first, last) = (row_col(source, sel.start).0, row_col(source, sel.end).0);
    if (first..=last).any(|row| scan.view().row_is_literal(source, row)) {
        return None;
    }
    if let Some(pair) = pairs_holding(source, &sel)
        .into_iter()
        .rev()
        .find(|p| p.formats.contains(&format))
    {
        let open = &source[pair.open.clone()];
        let close = &source[pair.close.clone()];
        let edits = vec![
            Edit {
                range: pair.open.clone(),
                text: remaining(open, format),
            },
            Edit {
                range: pair.close.clone(),
                text: remaining(close, format),
            },
        ];
        // A selection that took the markers in keeps only the text.
        let start = mapped(sel.start.max(pair.open.end), &edits);
        let end = mapped(sel.end.min(pair.close.start), &edits);
        return Some(EditResult {
            edits,
            selection: if sel.is_empty() {
                start..start
            } else {
                start..end
            },
        });
    }
    let (open, close) = format.markers();
    if !sel.is_empty() {
        let (range, text, selection) = format::toggle_wrap_pair(source, sel, open, close)?;
        return Some(EditResult {
            edits: vec![Edit { range, text }],
            selection,
        });
    }
    let caret = sel.start;
    let word_char = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '\'');
    let inside_word =
        word_char(source[..caret].chars().next_back()) && word_char(source[caret..].chars().next());
    if inside_word && let Some(word) = crate::words::word_range_at(source, caret) {
        let text = format!("{open}{}{close}", &source[word.clone()]);
        return Some(EditResult::one(word, text, caret + open.len()));
    }
    Some(EditResult::one(
        caret..caret,
        format!("{open}{close}"),
        caret + open.len(),
    ))
}

/// The list button: a bulleted or numbered item turns back into text,
/// anything else into a bulleted item.
pub fn toggle_list(source: &str, selection: Range<usize>) -> Option<EditResult> {
    let kind = match crate::commands::block_kind(source, selection.clone()) {
        TurnKind::Bullet | TurnKind::Numbered => TurnKind::Text,
        _ => TurnKind::Bullet,
    };
    crate::commands::turn_into(source, selection, kind)
}

/// The task button: a task's box checks or unchecks; any other line turns
/// into a task.
pub fn task_button(source: &str, selection: Range<usize>) -> Option<EditResult> {
    let caret = selection.end.min(source.len());
    let (start, end) = line_bounds(source, caret);
    if task_prefix(&source[start..end]).is_some() {
        return crate::commands::toggle_task(source, selection);
    }
    crate::commands::turn_into(source, selection, TurnKind::Todo)
}

/// A link's URL as Markdown writes it: no spaces or parentheses to end it
/// early.
fn link_target(url: &str) -> String {
    url.trim()
        .replace(' ', "%20")
        .replace('(', "%28")
        .replace(')', "%29")
}

/// A link's text as Markdown writes it, so the shared scanner reads it
/// back whole: one line, no brackets (its first `]` ends a label, escaped
/// or not), backslashes doubled.
fn link_label(text: &str) -> String {
    text.replace(['\n', '\r'], " ")
        .replace(['[', ']'], "")
        .replace('\\', "\\\\")
}

/// The link sheet's Done: `[text](url)` over the link at the caret, or
/// over the selection (the caret after it). An empty URL takes the link at
/// the caret away, keeping its text; an empty text shows the URL. `None`
/// with no URL and no link.
pub fn set_link(
    source: &str,
    selection: Range<usize>,
    text: &str,
    url: &str,
) -> Option<EditResult> {
    let sel = sorted(&selection);
    let link = link_at(source, selection.end.min(source.len()));
    let range = link.as_ref().map_or(sel, |l| l.range.clone());
    let target = link_target(url);
    let replacement = if target.is_empty() {
        link.as_ref()?;
        // The text as typed: no longer a label.
        let text = text.replace(['\n', '\r'], " ");
        if text.is_empty() {
            link.map(|l| l.text).unwrap_or_default()
        } else {
            text
        }
    } else if link_label(text).trim().is_empty() {
        format!("[{}]({target})", link_label(&target))
    } else {
        format!("[{}]({target})", link_label(text))
    };
    let caret = range.start + replacement.len();
    Some(EditResult::one(range, replacement, caret))
}

/// Inserts `block` as a block of its own after the caret's line (in place
/// of it when blank), blank lines around it, the caret on a new line below
/// it or `caret_in` bytes into it.
fn insert_block(
    source: &str,
    selection: Range<usize>,
    block: &str,
    caret_in: Option<usize>,
) -> EditResult {
    let caret = selection.end.min(source.len());
    let (start, end) = line_bounds(source, caret);
    let (range, before) = if source[start..end].trim().is_empty() {
        let above = start > 0
            && !source[..start - 1]
                .rsplit('\n')
                .next()
                .unwrap_or("")
                .trim()
                .is_empty();
        (start..end, if above { "\n" } else { "" })
    } else {
        (end..end, "\n\n")
    };
    let text = format!("{before}{block}\n\n");
    let to = match caret_in {
        Some(offset) => range.start + before.len() + offset,
        None => range.start + text.len(),
    };
    EditResult::one(range, text, to)
}

/// The formatting keyboard's Rule: a thematic break of its own.
pub fn insert_rule(source: &str, selection: Range<usize>) -> EditResult {
    insert_block(source, selection, "---", None)
}

/// The formatting keyboard's Table: a two-column table, a header row and a
/// body row, empty. With Show Markdown on the caret goes into its first
/// cell; live preview edits cells in a later slice, so the caret goes below
/// it there.
pub fn insert_table(source: &str, selection: Range<usize>, rendered: bool) -> EditResult {
    let block = "|  |  |\n| --- | --- |\n|  |  |";
    insert_block(source, selection, block, (!rendered).then_some(2))
}

#[cfg(test)]
mod tests {
    use super::*;
    use InlineFormat::*;

    fn caret(c: usize) -> Range<usize> {
        c..c
    }

    fn run(source: &str, result: Option<EditResult>) -> (String, Range<usize>) {
        let r = result.expect("a change");
        (r.apply(source), r.selection)
    }

    fn at(source: &str, needle: &str) -> usize {
        source.find(needle).unwrap()
    }

    #[test]
    fn formats_show_inside_their_markers_only() {
        let s = "a **bold *both* x** `code` ==hi== ~~x~~ <u>u</u> end";
        let state = |c: usize| caret_state(s, caret(c), true).formats;
        assert_eq!(state(at(s, "bold")), [Bold]);
        // Just inside the opener and just before the closer count; outside
        // the markers does not.
        assert_eq!(state(at(s, "**bold")), Vec::<InlineFormat>::new());
        assert_eq!(state(at(s, "both") + 4), [Bold, Italic]);
        assert_eq!(state(at(s, "code")), [Code]);
        assert_eq!(state(at(s, "hi")), [Highlight]);
        assert_eq!(state(at(s, "x~~")), [Strikethrough]);
        assert_eq!(state(at(s, "u</u>")), [Underline]);
        assert!(state(at(s, " end")).is_empty());
        // A heading's body is bold to look at, not bold text.
        assert!(caret_state("# Title", caret(4), true).formats.is_empty());
        // A selection within a construct.
        let b = at(s, "bold");
        assert_eq!(caret_state(s, b..b + 4, true).formats, [Bold]);
        assert!(caret_state(s, 0..b + 4, true).formats.is_empty());
    }

    #[test]
    fn the_caret_state_names_lines_links_and_tables() {
        let s = "- one\n- [x] two\n    - three\n> q\n\n| a |\n| - |\n| b |\n\n```\ncode\n```";
        let state = |needle: &str| caret_state(s, caret(at(s, needle)), true);
        let one = state("one");
        assert!(one.list && one.task.is_none() && !one.can_indent && !one.can_outdent);
        assert_eq!(one.block, TurnKind::Bullet);
        let two = state("two");
        assert_eq!(two.task, Some(true));
        assert!(!two.list && two.can_indent);
        assert!(state("three").can_outdent);
        assert!(!state("q").can_indent);
        assert!(state("b |").in_table && !state("q").in_table);
        assert!(!state("code").can_format && state("q").can_format);
        let l = "see [the site](https://example.com) or ftp://x.org now";
        let link = caret_state(l, caret(at(l, "site")), true).link.unwrap();
        assert_eq!(
            (link.text.as_str(), link.url.as_str(), link.web),
            ("the site", "https://example.com", true)
        );
        assert_eq!(&l[link.range.clone()], "[the site](https://example.com)");
        // Touching its end counts, as the reveal rule does.
        assert!(link_at(l, link.range.end).is_some());
        assert!(link_at(l, at(l, "now")).is_none());
        let bare = link_at("go https://a.org/x now", 5).unwrap();
        assert_eq!((bare.text.as_str(), bare.web), ("https://a.org/x", true));
        assert!(!link_at("[x](javascript:alert(1))", 1).unwrap().web);
        assert!(!link_at("[x](mailto:a@b.c)", 1).unwrap().web);
    }

    #[test]
    fn a_format_comes_off_at_the_caret() {
        let s = "a **bold** b";
        assert_eq!(
            run(s, toggle_format(s, caret(at(s, "ld")), Bold)),
            ("a bold b".into(), caret(4))
        );
        // A selection inside the construct unwraps it too, keeping the text selected.
        let o = at(s, "old");
        assert_eq!(
            run(s, toggle_format(s, o..o + 3, Bold)),
            ("a bold b".into(), 3..6)
        );
        // Bold off bold-italic leaves the italic.
        let s = "***bi***";
        assert_eq!(run(s, toggle_format(s, caret(4), Bold)).0, "*bi*");
        assert_eq!(run(s, toggle_format(s, caret(4), Italic)).0, "**bi**");
        // Nested: italic comes off inside bold.
        let s = "**a *b* c**";
        assert_eq!(
            run(s, toggle_format(s, caret(at(s, "b*")), Italic)).0,
            "**a b c**"
        );
        let s = "x <mark>y</mark> `z`";
        assert_eq!(
            run(s, toggle_format(s, caret(at(s, "y<")), Highlight)).0,
            "x y `z`"
        );
        assert_eq!(
            run(s, toggle_format(s, caret(at(s, "z")), Code)).0,
            "x <mark>y</mark> z"
        );
    }

    #[test]
    fn a_format_goes_on_at_the_caret() {
        // Inside a word: the word.
        let s = "the tenant pays";
        assert_eq!(
            run(s, toggle_format(s, caret(6), Bold)),
            ("the **tenant** pays".into(), caret(8))
        );
        // Between words or at a word's edge: an empty pair to type into.
        assert_eq!(
            run(s, toggle_format(s, caret(3), Italic)),
            ("the** tenant pays".into(), caret(4))
        );
        assert_eq!(
            run("", toggle_format("", caret(0), Strikethrough)),
            ("~~~~".into(), caret(2))
        );
        // A selection: desktop's rule.
        assert_eq!(
            run(s, toggle_format(s, 4..10, Highlight)),
            ("the ==tenant== pays".into(), 6..12)
        );
        assert_eq!(toggle_format("a  b", 1..3, Bold), None);
        // In code and math markers are literal: nothing toggles.
        let s = "a\n```\namount\n```\n$$\nx\n$$";
        assert_eq!(toggle_format(s, caret(at(s, "amount") + 2), Bold), None);
        assert_eq!(toggle_format(s, caret(at(s, "x\n$$")), Italic), None);
        assert_eq!(toggle_format(s, 0..at(s, "amount") + 2, Bold), None);
        // Cyrillic words wrap whole.
        let s = "один два";
        assert_eq!(run(s, toggle_format(s, caret(2), Bold)).0, "**один** два");
    }

    #[test]
    fn list_and_task_buttons() {
        assert_eq!(
            run("a", toggle_list("a", caret(1))),
            ("- a".into(), caret(3))
        );
        assert_eq!(run("- a", toggle_list("- a", caret(3))).0, "a");
        assert_eq!(run("1. a", toggle_list("1. a", caret(4))).0, "a");
        assert_eq!(run("a", task_button("a", caret(1))).0, "- [ ] a");
        assert_eq!(run("- a", task_button("- a", caret(3))).0, "- [ ] a");
        assert_eq!(
            run("- [ ] a", task_button("- [ ] a", caret(7))).0,
            "- [x] a"
        );
        assert_eq!(
            run("- [x] a", task_button("- [x] a", caret(7))).0,
            "- [ ] a"
        );
    }

    #[test]
    fn links_are_set_edited_and_removed() {
        let s = "see the site now";
        let r = at(s, "the site");
        assert_eq!(
            run(s, set_link(s, r..r + 8, "the site", "https://example.com")),
            ("see [the site](https://example.com) now".into(), caret(35))
        );
        let s = "see [old](https://a.org) now";
        let c = caret(at(s, "old"));
        assert_eq!(
            run(s, set_link(s, c.clone(), "new", "https://b.org/a b")).0,
            "see [new](https://b.org/a%20b) now"
        );
        // No URL: the link goes, its text stays.
        assert_eq!(run(s, set_link(s, c.clone(), "old", "")).0, "see old now");
        assert_eq!(run(s, set_link(s, c, "", " ")).0, "see old now");
        assert_eq!(set_link("plain", caret(2), "x", ""), None);
        // No text: the URL shows.
        assert_eq!(
            run("", set_link("", caret(0), "", "https://a.org")).0,
            "[https://a.org](https://a.org)"
        );
        // Brackets would end the label early; backslashes are doubled, and
        // the label reads back as typed.
        for (typed, label, read) in [
            ("Section 1]", "Section 1", "Section 1"),
            ("[draft] terms", "draft terms", "draft terms"),
            ("a\\b\\", "a\\\\b\\\\", "a\\b\\"),
        ] {
            let (out, _) = run("", set_link("", caret(0), typed, "https://a.org"));
            assert_eq!(out, format!("[{label}](https://a.org)"), "{typed:?}");
            let link = link_at(&out, 1).expect("the scanner reads it as a link");
            assert_eq!(
                (link.text.as_str(), link.url.as_str()),
                (read, "https://a.org")
            );
        }
    }

    #[test]
    fn a_rule_and_a_table_are_blocks_of_their_own() {
        // After a line of text: blank lines around it, the caret below.
        assert_eq!(
            run("abc", Some(insert_rule("abc", caret(1)))),
            ("abc\n\n---\n\n".into(), caret(10))
        );
        // `---` right under text would be a heading underline: never.
        let (out, sel) = run("abc\nnext", Some(insert_rule("abc\nnext", caret(3))));
        assert_eq!(out, "abc\n\n---\n\n\nnext");
        assert_eq!(sel, caret(10));
        // On a blank line: in its place.
        assert_eq!(
            run("a\n\n", Some(insert_rule("a\n\n", caret(3)))).0,
            "a\n\n---\n\n"
        );
        assert_eq!(run("", Some(insert_rule("", caret(0)))).0, "---\n\n");
        let table = "|  |  |\n| --- | --- |\n|  |  |";
        let (out, sel) = run("abc", Some(insert_table("abc", caret(3), false)));
        assert_eq!(out, format!("abc\n\n{table}\n\n"));
        assert_eq!(sel, caret(7));
        let (out, sel) = run("abc", Some(insert_table("abc", caret(3), true)));
        assert_eq!(sel, caret(out.len()));
        assert_eq!(mdoc_markdown::tables::table_regions(&out).len(), 1);
    }
}
