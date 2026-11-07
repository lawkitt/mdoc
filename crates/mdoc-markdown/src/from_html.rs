//! Markdown from the HTML an app's Copy puts on the clipboard (ADR 0105).
//!
//! The ChatGPT iOS app copies a reply as HTML, RTF and a flattened plain
//! text: no Markdown, and a table's cells end up separated by tabs. This
//! turns the HTML back into the Markdown the rest of mdoc reads.
//!
//! Two promises: no text is lost for structure we don't know (an unknown tag
//! keeps its text), and an alias stays byte for byte, so Restore finds it:
//! `_` is never escaped.

use std::cell::RefCell;

use crate::tables;
use html5ever::tendril::StrTendril;
use html5ever::tokenizer::{
    BufferQueue, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts,
};

/// The Markdown for `html`, or `None` when it holds no text.
pub fn from_html(html: &str) -> Option<String> {
    convert(html).0
}

/// What a copy holding `html` and `plain` pastes as. The HTML only when it
/// has what the plain text lost: plain text that is Markdown already is the
/// source (an AI app's Markdown Copy, a Markdown editor), and HTML that is
/// only paragraphs adds nothing and would escape Markdown in its text.
pub fn pasted_markdown(html: &str, plain: Option<&str>) -> Option<String> {
    let plain = plain.filter(|plain| !plain.trim().is_empty());
    if let Some(plain) = plain.filter(|plain| is_markdown(plain)) {
        return Some(plain.to_string());
    }
    let (markdown, structured) = convert(html);
    match (markdown, plain) {
        (Some(markdown), _) if structured => Some(markdown),
        (_, Some(plain)) => Some(plain.to_string()),
        (markdown, None) => markdown,
    }
}

/// Whether plain text shows what only Markdown writes: a table's separator,
/// a heading or a bold pair. A list doesn't count, as `* ` is how the ChatGPT
/// app's flattened plain text writes one, and neither does a lone `**`, as in
/// `x**2`.
fn is_markdown(plain: &str) -> bool {
    plain.lines().any(|line| {
        let hashes = line.chars().take_while(|c| *c == '#').count();
        (1..=6).contains(&hashes) && line[hashes..].starts_with(' ')
            || line.contains('-') && line.contains('|') && tables::is_table_separator(line)
            || has_bold_pair(line, "**")
            || has_bold_pair(line, "__")
    })
}

/// `marker` in a pair with no space just inside it, and not against a word
/// on its outer sides: for `**` a word in ASCII, as `x**2`, since Chinese has
/// no spaces to set bold off with; for `__` any word, which it can't open in.
fn has_bold_pair(line: &str, marker: &str) -> bool {
    let is_word = |c: Option<char>| {
        c.is_some_and(|c| {
            if marker == "__" {
                c.is_alphanumeric()
            } else {
                c.is_ascii_alphanumeric()
            }
        })
    };
    let mut from = 0;
    while let Some(found) = line[from..].find(marker) {
        let start = from + found;
        let inner = start + marker.len();
        if !is_word(line[..start].chars().next_back())
            && line[inner..]
                .chars()
                .next()
                .is_some_and(|c| !c.is_whitespace())
            && let Some(close) = line[inner..].find(marker)
        {
            let end = inner + close;
            if close > 0
                && line[..end]
                    .chars()
                    .next_back()
                    .is_some_and(|c| !c.is_whitespace())
                && !is_word(line[end + marker.len()..].chars().next())
            {
                return true;
            }
        }
        from = inner;
    }
    false
}

/// The Markdown, and whether the HTML held more than paragraphs.
fn convert(html: &str) -> (Option<String>, bool) {
    let queue = BufferQueue::default();
    queue.push_back(StrTendril::from_slice(html));
    let tokenizer = Tokenizer::new(
        Sink(RefCell::new(Converter::new())),
        TokenizerOpts::default(),
    );
    let _ = tokenizer.feed(&queue);
    tokenizer.end();
    let converter = tokenizer.sink.0.into_inner();
    let structured = converter.structured;
    let markdown = converter.finish();
    (
        (!markdown.trim().is_empty()).then_some(markdown),
        structured,
    )
}

struct Sink(RefCell<Converter>);

impl TokenSink for Sink {
    type Handle = ();

    fn process_token(&self, token: Token, _line: u64) -> TokenSinkResult<()> {
        let mut converter = self.0.borrow_mut();
        match token {
            Token::TagToken(tag) => match tag.kind {
                TagKind::StartTag => {
                    let attribute = |name: &str| {
                        tag.attrs
                            .iter()
                            .find(|a| &*a.name.local == name)
                            .map(|a| a.value.to_string())
                    };
                    converter.flush_text();
                    converter.open(&tag.name, attribute("href"), attribute("alt"));
                    if tag.self_closing {
                        converter.close(&tag.name);
                    }
                }
                TagKind::EndTag => {
                    converter.flush_text();
                    converter.close(&tag.name);
                }
            },
            // Split at each character reference: gathered until the next tag.
            Token::CharacterTokens(text) => converter.pending.push_str(&text),
            Token::NullCharacterToken
            | Token::CommentToken(_)
            | Token::DoctypeToken(_)
            | Token::EOFToken
            | Token::ParseError(_) => {}
        }
        TokenSinkResult::Continue
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Kind {
    Root,
    /// `p`, `div` and the like: their text becomes paragraphs.
    Block(String),
    Heading(usize),
    Quote,
    List {
        ordered: bool,
    },
    Item,
    Table,
    Row,
    Cell {
        header: bool,
    },
    Pre,
    Strong,
    Em,
    Strike,
    Code,
    Link(Option<String>),
    /// `head`, `style`, `script`: nothing in them is text of the reply.
    Skip(String),
}

impl Kind {
    fn is_inline(&self) -> bool {
        matches!(
            self,
            Kind::Strong | Kind::Em | Kind::Strike | Kind::Code | Kind::Link(_)
        )
    }

    fn matches(&self, tag: &str) -> bool {
        match self {
            Kind::Root => false,
            Kind::Block(name) | Kind::Skip(name) => name == tag,
            Kind::Heading(level) => {
                tag.len() == 2 && tag.starts_with('h') && tag[1..] == level.to_string()
            }
            Kind::Quote => tag == "blockquote",
            Kind::List { ordered } => tag == if *ordered { "ol" } else { "ul" },
            Kind::Item => tag == "li",
            Kind::Table => tag == "table",
            Kind::Row => tag == "tr",
            Kind::Cell { header } => tag == if *header { "th" } else { "td" },
            Kind::Pre => tag == "pre",
            Kind::Strong => matches!(tag, "b" | "strong"),
            Kind::Em => matches!(tag, "i" | "em"),
            Kind::Strike => matches!(tag, "s" | "strike" | "del"),
            Kind::Code => matches!(tag, "code" | "kbd" | "samp" | "tt"),
            Kind::Link(_) => tag == "a",
        }
    }
}

/// One finished block of Markdown.
#[derive(Debug)]
struct Done {
    text: String,
    /// A list follows the item's text on the next line, without a gap.
    list: bool,
}

#[derive(Debug)]
struct Frame {
    kind: Kind,
    /// Text not yet part of a block.
    inline: String,
    blocks: Vec<Done>,
    /// A row's cells.
    cells: Vec<String>,
    /// A table's rows.
    rows: Vec<Vec<String>>,
    /// The marks that end `inline`, innermost first.
    marks: Vec<Marks>,
    /// An inline frame opened right after a word.
    after_word: bool,
}

/// The byte ranges of one inline mark's opening and closing in `inline`.
#[derive(Debug)]
struct Marks {
    open: std::ops::Range<usize>,
    close: std::ops::Range<usize>,
}

/// What an alias is made of (`valid_replacement` in mdoc-pii), and words:
/// marks between these would split one.
fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-'
}

impl Frame {
    fn new(kind: Kind) -> Self {
        Frame {
            kind,
            inline: String::new(),
            blocks: Vec::new(),
            cells: Vec::new(),
            rows: Vec::new(),
            marks: Vec::new(),
            after_word: false,
        }
    }

    /// Marks that would end in the middle of a word go, or `<b>ORG</b>_3`
    /// would hide the alias ORG_3 from Restore.
    fn unsplit(&mut self, next: char) {
        let marks = std::mem::take(&mut self.marks);
        let (Some(innermost), Some(outermost)) = (marks.first(), marks.last()) else {
            return;
        };
        let content_end = self.inline[..innermost.close.start].chars().next_back();
        if outermost.close.end == self.inline.len()
            && is_word(next)
            && content_end.is_some_and(is_word)
        {
            let mut ranges: Vec<_> = marks
                .into_iter()
                .flat_map(|mark| [mark.open, mark.close])
                .collect();
            ranges.sort_by_key(|range| std::cmp::Reverse(range.start));
            for range in ranges {
                self.inline.replace_range(range, "");
            }
        }
    }

    /// The text so far becomes a paragraph.
    fn flush(&mut self) {
        let text = self.inline.trim();
        if !text.is_empty() {
            let text = escape_line_starts(text);
            self.blocks.push(Done { text, list: false });
        }
        self.inline.clear();
        self.marks.clear();
    }

    /// The frame's blocks, with a blank line between them; in a list item, a
    /// nested list follows on the next line.
    fn joined(&self) -> String {
        let mut out = String::new();
        for (index, block) in self.blocks.iter().enumerate() {
            if index > 0 {
                out.push_str(if block.list && self.kind == Kind::Item {
                    "\n"
                } else {
                    "\n\n"
                });
            }
            out.push_str(&block.text);
        }
        out
    }
}

struct Converter {
    stack: Vec<Frame>,
    /// Text not yet read: the tokenizer's pieces of it.
    pending: String,
    /// The HTML has a heading, list, table, quote, code or inline mark.
    structured: bool,
}

impl Converter {
    fn new() -> Self {
        Converter {
            stack: vec![Frame::new(Kind::Root)],
            pending: String::new(),
            structured: false,
        }
    }

    fn flush_text(&mut self) {
        let text = std::mem::take(&mut self.pending);
        self.text(&text);
    }

    fn top(&mut self) -> &mut Frame {
        self.stack.last_mut().expect("the root stays")
    }

    fn inside(&self, wanted: impl Fn(&Kind) -> bool) -> bool {
        self.stack.iter().any(|frame| wanted(&frame.kind))
    }

    fn skipping(&self) -> bool {
        self.inside(|kind| matches!(kind, Kind::Skip(_)))
    }

    fn open(&mut self, tag: &str, href: Option<String>, alt: Option<String>) {
        if self.skipping() {
            return;
        }
        match tag {
            "head" | "style" | "script" | "title" | "template" | "noscript" => {
                self.push(Kind::Skip(tag.to_string()));
            }
            "p" | "div" | "section" | "article" | "main" | "header" | "footer" | "aside"
            | "nav" | "figure" | "figcaption" | "center" | "address" | "dl" | "dt" | "dd"
            | "details" | "summary" | "caption" => {
                self.close_paragraph();
                self.push(Kind::Block(tag.to_string()));
            }
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                self.close_paragraph();
                self.push(Kind::Heading(usize::from(tag.as_bytes()[1] - b'0')));
            }
            "blockquote" => {
                self.close_paragraph();
                self.push(Kind::Quote);
            }
            "ul" | "ol" => {
                self.close_paragraph();
                self.push(Kind::List {
                    ordered: tag == "ol",
                });
            }
            "li" => {
                self.close_to(|kind| matches!(kind, Kind::List { .. }));
                if matches!(self.top().kind, Kind::List { .. }) {
                    self.push(Kind::Item);
                } else {
                    self.push(Kind::Block(tag.to_string()));
                }
            }
            "table" => {
                self.close_paragraph();
                self.push(Kind::Table);
            }
            "tr" => {
                if self.inside(|kind| *kind == Kind::Table) {
                    self.close_to(|kind| *kind == Kind::Table);
                    self.push(Kind::Row);
                } else {
                    self.push(Kind::Block(tag.to_string()));
                }
            }
            "td" | "th" => {
                if self.inside(|kind| *kind == Kind::Table) {
                    self.close_to(|kind| matches!(kind, Kind::Row | Kind::Table));
                    if self.top().kind == Kind::Table {
                        self.push(Kind::Row);
                    }
                    self.push(Kind::Cell {
                        header: tag == "th",
                    });
                } else {
                    self.push(Kind::Block(tag.to_string()));
                }
            }
            "pre" => {
                self.close_paragraph();
                self.push(Kind::Pre);
            }
            "b" | "strong" => self.push(Kind::Strong),
            "i" | "em" => self.push(Kind::Em),
            "s" | "strike" | "del" => self.push(Kind::Strike),
            "code" | "kbd" | "samp" | "tt" => {
                // A `pre` fences its text already.
                if !self.inside(|kind| *kind == Kind::Pre) {
                    self.push(Kind::Code);
                }
            }
            "a" => self.push(Kind::Link(href)),
            "br" => {
                let pre = self.inside(|kind| *kind == Kind::Pre);
                let top = self.top();
                top.inline.truncate(top.inline.trim_end().len());
                top.inline.push_str(if pre { "\n" } else { "  \n" });
            }
            "hr" => {
                self.close_paragraph();
                let top = self.top();
                top.flush();
                top.blocks.push(Done {
                    text: "---".to_string(),
                    list: false,
                });
            }
            "img" => {
                if let Some(alt) = alt {
                    self.text(&alt);
                }
            }
            // span, font, u, html, body and anything unknown: the text stays.
            _ => {}
        }
    }

    fn push(&mut self, kind: Kind) {
        self.structured |= !matches!(kind, Kind::Block(_) | Kind::Skip(_));
        if !kind.is_inline() {
            self.top().flush();
        }
        let top = self.top();
        let after_word = match top.inline.chars().next_back() {
            Some(last) => is_word(last),
            None => top.after_word,
        };
        let mut frame = Frame::new(kind);
        frame.after_word = frame.kind.is_inline() && after_word;
        self.stack.push(frame);
    }

    /// An open `p` ends where a block starts, as in HTML.
    fn close_paragraph(&mut self) {
        let index = self.stack.iter().rposition(|frame| {
            !frame.kind.is_inline() && !matches!(frame.kind, Kind::Block(_))
                || matches!(&frame.kind, Kind::Block(name) if name == "p")
        });
        if let Some(index) = index
            && matches!(&self.stack[index].kind, Kind::Block(name) if name == "p")
        {
            while self.stack.len() > index {
                self.pop();
            }
        }
    }

    /// Closes frames until the top is one `stop` accepts, or the root.
    fn close_to(&mut self, stop: impl Fn(&Kind) -> bool) {
        if !self.inside(&stop) {
            return;
        }
        while self.stack.len() > 1 && !stop(&self.top().kind) {
            self.pop();
        }
    }

    fn close(&mut self, tag: &str) {
        if let Some(Kind::Skip(name)) = self
            .stack
            .iter()
            .find(|frame| matches!(frame.kind, Kind::Skip(_)))
            .map(|frame| frame.kind.clone())
        {
            if name == tag {
                self.close_matching(tag);
            }
            return;
        }
        if tag == "br" {
            return;
        }
        self.close_matching(tag);
    }

    /// Closes the nearest open `tag`; a stray closing tag closes nothing, and
    /// none reaches past the cell or list item it was written in.
    fn close_matching(&mut self, tag: &str) {
        let structural = matches!(tag, "table" | "tr" | "td" | "th" | "ul" | "ol" | "li");
        let mut found = None;
        for (index, frame) in self.stack.iter().enumerate().rev() {
            if frame.kind.matches(tag) {
                found = Some(index);
                break;
            }
            if !structural && matches!(frame.kind, Kind::Cell { .. } | Kind::Item | Kind::Quote) {
                break;
            }
        }
        if let Some(index) = found {
            while self.stack.len() > index {
                self.pop();
            }
        }
    }

    fn text(&mut self, text: &str) {
        if self.skipping() {
            return;
        }
        let raw = self.inside(|kind| matches!(kind, Kind::Pre | Kind::Code));
        if self.inside(|kind| *kind == Kind::Pre) {
            self.top().inline.push_str(text);
            return;
        }
        let at_block_start = {
            let top = self.top();
            !top.kind.is_inline() && top.inline.is_empty()
        };
        let top = self.top();
        let mut chars = text.chars().peekable();
        let mut last_space = at_block_start || top.inline.ends_with([' ', '\n']);
        while let Some(c) = chars.next() {
            if c.is_ascii_whitespace() {
                if !last_space {
                    top.inline.push(' ');
                    last_space = true;
                }
                continue;
            }
            last_space = false;
            let c = if c == '\u{a0}' { ' ' } else { c };
            top.unsplit(c);
            if c == '_' && !raw {
                // Inside a word it is no emphasis, and an alias holds runs of
                // it. At a mark's edge the neighbour is still unknown: it may
                // be an alias split by marks, so it stays.
                let run = 1 + chars.clone().take_while(|next| *next == '_').count();
                let word = |c: char| c.is_alphanumeric() || c == '-';
                let starts_loose = match top.inline.chars().next_back() {
                    Some(before) => !word(before),
                    None => !top.kind.is_inline(),
                };
                let ends_loose = chars.clone().nth(run - 1).is_some_and(|next| !word(next));
                for _ in 1..run {
                    chars.next();
                }
                for _ in 0..run {
                    if starts_loose || ends_loose {
                        top.inline.push('\\');
                    }
                    top.inline.push('_');
                }
                continue;
            }
            if !raw {
                let doubled = matches!(c, '~' | '=') && chars.peek() == Some(&c);
                let entity = c == '&' && {
                    let rest: String = chars.clone().take(32).collect();
                    let name = rest.split(';').next().unwrap_or("");
                    rest.contains(';')
                        && !name.is_empty()
                        && name.chars().all(|n| n.is_ascii_alphanumeric() || n == '#')
                };
                if matches!(c, '\\' | '*' | '`' | '<' | '[' | ']') || entity || doubled {
                    top.inline.push('\\');
                }
            }
            top.inline.push(c);
        }
    }

    fn pop(&mut self) {
        let mut frame = self.stack.pop().expect("the root stays");
        let quiet_bold = self.inside(|kind| {
            matches!(
                kind,
                Kind::Heading(_) | Kind::Cell { header: true } | Kind::Strong
            )
        });
        let quiet_em = self.inside(|kind| *kind == Kind::Em);
        let code = self.inside(|kind| *kind == Kind::Code);
        // A block inside a mark or link (`<a><p>…</p></a>`): the blocks go up
        // as they are; there is no Markdown for the mark around them.
        if frame.kind.is_inline() && !frame.blocks.is_empty() {
            frame.flush();
            let parent = self.top();
            parent.flush();
            parent.blocks.append(&mut frame.blocks);
            return;
        }
        let marks = std::mem::take(&mut frame.marks);
        let after_word = frame.after_word;
        match frame.kind.clone() {
            Kind::Root => unreachable!("the root is never popped"),
            Kind::Skip(_) => {}
            Kind::Strong => self.wrap(frame.inline, marks, after_word, "**", "**", quiet_bold),
            Kind::Em => self.wrap(frame.inline, marks, after_word, "*", "*", quiet_em),
            Kind::Strike => self.wrap(frame.inline, marks, after_word, "~~", "~~", false),
            Kind::Code => {
                let fence = "`".repeat(longest_run(&frame.inline, '`') + 1);
                // A space keeps a backtick at the code's edge out of the fence.
                let trimmed = frame.inline.trim();
                let pad = if trimmed.starts_with('`') || trimmed.ends_with('`') {
                    " "
                } else {
                    ""
                };
                self.wrap(
                    frame.inline.clone(),
                    marks,
                    after_word,
                    &format!("{fence}{pad}"),
                    &format!("{pad}{fence}"),
                    code,
                );
            }
            Kind::Link(href) => match href.filter(|href| safe_link(href)) {
                Some(href) => {
                    let close = format!("]({})", href.replace(' ', "%20").replace(')', "%29"));
                    self.wrap(frame.inline, marks, after_word, "[", &close, false);
                }
                None => self.wrap(frame.inline, marks, after_word, "", "", true),
            },
            Kind::Block(_) => {
                frame.flush();
                let parent = self.top();
                parent.flush();
                parent.blocks.append(&mut frame.blocks);
            }
            Kind::Heading(level) => {
                // Not `flush`: "1. Parties" under a heading is no list.
                let text = format!("{} {}", frame.joined(), frame.inline);
                let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
                self.add_block(
                    (!text.is_empty()).then(|| format!("{} {text}", "#".repeat(level))),
                    false,
                );
            }
            Kind::Quote => {
                frame.flush();
                let text = frame.joined();
                let quoted = text
                    .lines()
                    .map(|line| {
                        if line.is_empty() {
                            ">".to_string()
                        } else {
                            format!("> {line}")
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                self.add_block((!text.is_empty()).then_some(quoted), false);
            }
            Kind::Item => {
                frame.flush();
                let item = frame.joined();
                self.top().blocks.push(Done {
                    text: item,
                    list: false,
                });
            }
            Kind::List { ordered } => {
                frame.flush();
                let mut lines = Vec::new();
                for (index, item) in frame.blocks.iter().enumerate() {
                    let marker = if ordered {
                        format!("{}. ", index + 1)
                    } else {
                        "- ".to_string()
                    };
                    let indent = " ".repeat(marker.len());
                    for (line_index, line) in item.text.lines().enumerate() {
                        lines.push(if line_index == 0 {
                            format!("{marker}{line}")
                        } else if line.is_empty() {
                            String::new()
                        } else {
                            format!("{indent}{line}")
                        });
                    }
                    if item.text.is_empty() {
                        lines.push(marker.trim_end().to_string());
                    }
                }
                self.add_block((!lines.is_empty()).then(|| lines.join("\n")), true);
            }
            Kind::Table => {
                frame.flush();
                let columns = frame.rows.iter().map(Vec::len).max().unwrap_or(0);
                let mut parent_blocks = std::mem::take(&mut frame.blocks);
                self.top().flush();
                self.top().blocks.append(&mut parent_blocks);
                if columns > 0 {
                    let row = |cells: &[String]| {
                        let mut line = String::from("|");
                        for column in 0..columns {
                            line.push(' ');
                            line.push_str(cells.get(column).map_or("", String::as_str));
                            line.push_str(" |");
                        }
                        line
                    };
                    let mut lines = vec![row(&frame.rows[0])];
                    lines.push(format!("|{}", " --- |".repeat(columns)));
                    lines.extend(frame.rows[1..].iter().map(|cells| row(cells)));
                    self.add_block(Some(lines.join("\n")), false);
                }
            }
            Kind::Row => {
                frame.flush();
                let cells = std::mem::take(&mut frame.cells);
                let parent = self.top();
                // Text between cells stays, ahead of the table.
                parent.blocks.append(&mut frame.blocks);
                if !cells.is_empty() {
                    parent.rows.push(cells);
                }
            }
            Kind::Cell { .. } => {
                frame.flush();
                let text = frame
                    .blocks
                    .iter()
                    .flat_map(|block| block.text.lines())
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ");
                self.top().cells.push(escape_pipes(&text));
            }
            Kind::Pre => {
                let text = format!("{}\n{}", frame.joined(), frame.inline);
                let text = text.trim_matches('\n');
                let fence = "`".repeat(longest_run(text, '`').max(2) + 1);
                self.add_block(
                    (!text.is_empty()).then(|| format!("{fence}\n{text}\n{fence}")),
                    false,
                );
            }
        }
    }

    fn add_block(&mut self, text: Option<String>, list: bool) {
        let top = self.top();
        top.flush();
        if let Some(text) = text {
            top.blocks.push(Done { text, list });
        }
    }

    /// Adds inline text with its markers, any space at its ends outside them.
    /// None inside a word: `ORG_<b>3</b>` keeps the alias whole. `inner` are
    /// the marks ending the text, which the outer ones end together with.
    fn wrap(
        &mut self,
        text: String,
        inner: Vec<Marks>,
        after_word: bool,
        open: &str,
        close: &str,
        quiet: bool,
    ) {
        let trimmed = text.trim();
        let lead = text.len() - text.trim_start().len();
        let trail = text.len() - text.trim_end().len();
        let parent = self.top();
        if lead > 0 && !parent.inline.is_empty() && !parent.inline.ends_with([' ', '\n']) {
            parent.inline.push(' ');
        }
        let Some(first) = trimmed.chars().next() else {
            return;
        };
        if lead == 0 {
            parent.unsplit(first);
        } else {
            parent.marks.clear();
        }
        let inside_word = lead == 0
            && is_word(first)
            && (after_word || parent.inline.chars().next_back().is_some_and(is_word));
        let plain = quiet || inside_word;
        let open_at = parent.inline.len();
        if !plain {
            parent.inline.push_str(open);
        }
        let text_at = parent.inline.len();
        parent.inline.push_str(trimmed);
        let close_at = parent.inline.len();
        if !plain {
            parent.inline.push_str(close);
        }
        let shift = |range: std::ops::Range<usize>| {
            range.start - lead + text_at..range.end - lead + text_at
        };
        let mut marks: Vec<Marks> = inner
            .into_iter()
            .map(|mark| Marks {
                open: shift(mark.open),
                close: shift(mark.close),
            })
            .collect();
        if !plain {
            marks.push(Marks {
                open: open_at..text_at,
                close: close_at..close_at + close.len(),
            });
        }
        parent.marks = marks;
        if trail > 0 {
            parent.inline.push(' ');
            parent.marks.clear();
        }
    }

    fn finish(mut self) -> String {
        self.flush_text();
        while self.stack.len() > 1 {
            self.pop();
        }
        let root = self.top();
        root.flush();
        root.joined()
    }
}

fn longest_run(text: &str, wanted: char) -> usize {
    let (mut longest, mut run) = (0, 0);
    for c in text.chars() {
        run = if c == wanted { run + 1 } else { 0 };
        longest = longest.max(run);
    }
    longest
}

/// A link keeps its target only if opening it can't run anything.
fn safe_link(href: &str) -> bool {
    let lower = href.trim().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mailto:")
}

/// A `|` ends a cell unless escaped, in code too (GFM).
fn escape_pipes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut escaped = false;
    for c in text.chars() {
        if c == '|' && !escaped {
            out.push('\\');
        }
        escaped = c == '\\' && !escaped;
        out.push(c);
    }
    out
}

/// A paragraph that starts like a heading, quote or list stays a paragraph:
/// "1. Fees" as text in the HTML must not become a list.
fn escape_line_starts(text: &str) -> String {
    text.split('\n')
        .map(|line| {
            let digits = line.chars().take_while(char::is_ascii_digit).count();
            let rest = &line[digits..];
            if digits > 0 && (rest.starts_with(". ") || rest.starts_with(") ")) {
                format!("{}\\{rest}", &line[..digits])
            } else if line.starts_with("# ")
                || line.starts_with("## ")
                || line.starts_with("### ")
                || line.starts_with("#### ")
                || line.starts_with("##### ")
                || line.starts_with("###### ")
                || line.starts_with('>')
                || line.starts_with("- ")
                || line.starts_with("+ ")
                || line.starts_with("---")
                || line.starts_with('|')
            {
                format!("\\{line}")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
#[path = "from_html_tests.rs"]
mod tests;
