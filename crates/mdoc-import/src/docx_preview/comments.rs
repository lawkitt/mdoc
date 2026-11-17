//! Comment extraction runs in the conversion worker, before the renderer sees
//! a temporary copy. The copy keeps the renderer's own comment pane off (it
//! bakes comments into the page and shrinks the body) and marks where each
//! comment sits: every commented run of the body is wrapped in a link to
//! `mdoc-comment:<k>`, and those links' `/Link` rectangles in the rendered PDF
//! become the comments' anchors (`anchors`). They are never clickable links.
//!
//! The same copy carries Word's hidden text (`w:vanish`): it is counted
//! always, and drawn in place with a dotted underline and muted ink only
//! when the Source pane shows hidden text.
use super::{PreviewError, replace_parts, word, xml_text};
use roxmltree::Node;
use std::{
    collections::HashMap,
    io::{Cursor, Read},
};
use zip::ZipArchive;

/// The scheme of the marker links. A marker's URI is `mdoc-comment:<k>`, or
/// `mdoc-comment:<k>;<target>` when the run sat inside a real hyperlink, whose
/// target the PDF preview keeps clickable (gpui-pdf's `page_links`).
pub const COMMENT_SCHEME: &str = "mdoc-comment:";

const BODY: &str = "word/document.xml";
const BODY_RELS: &str = "word/_rels/document.xml.rels";

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct DocxComment {
    pub id: String,
    pub author: String,
    pub date: String,
    pub text: String,
    pub quote: String,
    /// The `id` of the comment this one replies to, if any.
    #[serde(default)]
    pub parent: Option<String>,
    /// The thread was marked done in Word.
    #[serde(default)]
    pub resolved: bool,
}

/// Where a comment sits on one page of the rendered preview. A comment whose
/// range crosses a page break has one anchor per page; a comment with none (a
/// footnote, a header, no range or reference) has no place on the page.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct CommentAnchor {
    /// The comment's `DocxComment::id`.
    pub id: String,
    /// 0-based page index.
    pub page: usize,
    /// One rectangle per line the range covers on this page, in reading order.
    pub rects: Vec<AnchorRect>,
    /// A point comment (no range, or a range with no text): its one
    /// rectangle is a 1 pt, zero-width spot on the baseline where the comment
    /// was inserted, to mark rather than tint.
    pub point: bool,
}

/// A rectangle in normalized page coordinates: 0..1 of the page, top-left
/// origin, matching the rendered page image (as gpui-pdf's links).
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct AnchorRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// A `/Link` annotation of the rendered PDF whose URI starts with
/// [`COMMENT_SCHEME`], as the app's PDF reader found it.
#[derive(Clone, Debug, PartialEq)]
pub struct LinkMark {
    pub page: usize,
    pub rect: AnchorRect,
    /// The URI after the scheme.
    pub target: String,
}

/// What marker `k` stands for: the comments open over its runs (indexes into
/// the comment list), and whether it marks a point comment.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub(super) struct Marker {
    comments: Vec<usize>,
    point: bool,
}

pub(super) struct Prepared {
    /// The render copy, or `None` when it would equal the document.
    pub render: Option<Vec<u8>>,
    pub comments: Vec<DocxComment>,
    pub markers: Vec<Marker>,
    /// Passages of hidden text: runs of `w:vanish` text, adjacent ones in a
    /// paragraph counted once.
    pub hidden: usize,
}

fn attr<'a>(node: Node<'a, '_>, name: &str) -> &'a str {
    node.attribute((node.tag_name().namespace().unwrap_or_default(), name))
        .unwrap_or_default()
}

fn text(node: Node<'_, '_>) -> String {
    let mut out = String::new();
    for n in node.descendants() {
        if word(n, "p") && !out.is_empty() {
            out.push('\n');
        }
        if word(n, "t") {
            out.push_str(n.text().unwrap_or_default());
        }
        if word(n, "tab") {
            out.push('\t');
        }
        if word(n, "br") || word(n, "cr") {
            out.push('\n');
        }
    }
    out.trim().to_owned()
}

/// Word 2012's comment extensions: replies and the done state.
const W15: &str = "http://schemas.microsoft.com/office/word/2012/wordml";
/// Word 2010's paragraph IDs, which the extensions point at.
const W14: &str = "http://schemas.microsoft.com/office/word/2010/wordml";

pub(super) fn prepare(bytes: &[u8], show_hidden: bool) -> Result<Prepared, PreviewError> {
    let invalid = |e: zip::result::ZipError| PreviewError::Package(e.to_string());
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(invalid)?;
    let mut comments = Vec::new();
    let mut by_id = HashMap::new();
    // The paragraph ID of each comment's last paragraph → its index.
    let mut by_paragraph = HashMap::new();
    let mut extended = None;
    let mut xml_parts = Vec::new();
    let mut body_rels = None;
    // Input has already passed the package expansion/entry limits.
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(invalid)?;
        let name = match entry.name() {
            Ok(name) => name.to_string(),
            Err(_) => return Err(PreviewError::Package("unreadable entry name".into())),
        };
        if !(name.ends_with(".xml") || name == BODY_RELS) {
            continue;
        }
        let mut raw = Vec::new();
        entry.read_to_end(&mut raw)?;
        let source = xml_text(&raw)?.into_owned();
        if name == BODY_RELS {
            body_rels = Some((i, source));
            continue;
        }
        let doc = roxmltree::Document::parse(&source)
            .map_err(|e| PreviewError::Package(e.to_string()))?;
        let root = doc.root_element();
        if root.tag_name().namespace() == Some(W15) && root.tag_name().name() == "commentsEx" {
            extended = Some(source);
            continue;
        }
        if word(root, "comments") {
            for node in root.children().filter(|n| word(*n, "comment")) {
                let id = attr(node, "id").to_owned();
                if id.is_empty() || by_id.insert(id.clone(), comments.len()).is_some() {
                    return Err(PreviewError::Package(
                        "missing or duplicate comment ID".into(),
                    ));
                }
                if let Some(paragraph) = node
                    .descendants()
                    .rfind(|n| word(*n, "p"))
                    .and_then(|p| p.attribute((W14, "paraId")))
                {
                    by_paragraph.insert(paragraph.to_ascii_uppercase(), comments.len());
                }
                comments.push(DocxComment {
                    id,
                    author: attr(node, "author").to_owned(),
                    date: attr(node, "date").to_owned(),
                    text: text(node),
                    quote: String::new(),
                    parent: None,
                    resolved: false,
                });
            }
        }
        xml_parts.push((i, name, source));
    }
    if let Some(extended) = extended {
        threads(&extended, &by_paragraph, &mut comments);
    }
    let mut replacements = HashMap::new();
    let mut markers = Markers {
        prefix: body_rels
            .as_ref()
            .map(|(_, rels)| relationship_prefix(rels))
            .unwrap_or_default(),
        ..Markers::default()
    };
    let mut hidden = 0;
    for (index, name, source) in xml_parts {
        let doc = roxmltree::Document::parse(&source)
            .map_err(|e| PreviewError::Package(e.to_string()))?;
        if word(doc.root_element(), "comments") {
            if !comments.is_empty() {
                replacements.insert(
                    index,
                    format!(
                        "<w:comments xmlns:w=\"{}\"/>",
                        doc.root_element().tag_name().namespace().unwrap()
                    ),
                );
            }
            continue;
        }
        let mut edits = Vec::new();
        hidden += hidden_text(&doc, show_hidden.then_some(&mut edits));
        if !comments.is_empty() {
            // Only the body's links reach the PDF as annotations the reader
            // can place (docxide reads the body and its relationships by
            // these names).
            let marks = name == BODY && body_rels.is_some();
            let links = if marks {
                relationships(&body_rels.as_ref().unwrap().1)
            } else {
                HashMap::new()
            };
            quote_and_mark(
                &doc,
                &by_id,
                &mut comments,
                &links,
                marks.then_some(&mut markers),
                &mut edits,
            );
        }
        if !edits.is_empty() {
            replacements.insert(index, apply(&source, edits));
        }
    }
    if !markers.list.is_empty()
        && let Some((index, rels)) = body_rels
    {
        replacements.insert(index, markers.add_relationships(&rels)?);
    }
    Ok(Prepared {
        render: if replacements.is_empty() {
            None
        } else {
            Some(replace_parts(&mut archive, &replacements)?)
        },
        comments,
        markers: markers.list,
        hidden,
    })
}

/// Replies and resolved threads from `commentsExtended.xml`: each entry
/// names a comment by its last paragraph's ID, the paragraph of the comment
/// it replies to, and whether its thread is done. A reply shares its
/// thread's state.
// Replaced by the fork's extraction (ROADMAP Comments item 2), which reports
// replies and resolved state with the comments.
fn threads(extended: &str, by_paragraph: &HashMap<String, usize>, comments: &mut [DocxComment]) {
    let Ok(doc) = roxmltree::Document::parse(extended) else {
        return;
    };
    let index = |node: Node<'_, '_>, name: &str| {
        node.attribute((W15, name))
            .and_then(|id| by_paragraph.get(&id.to_ascii_uppercase()))
            .copied()
    };
    let mut parents = Vec::new();
    for entry in doc
        .root_element()
        .children()
        .filter(|n| n.tag_name().namespace() == Some(W15) && n.tag_name().name() == "commentEx")
    {
        let Some(i) = index(entry, "paraId") else {
            continue;
        };
        comments[i].resolved = matches!(entry.attribute((W15, "done")), Some("1" | "true" | "on"));
        if let Some(parent) = index(entry, "paraIdParent").filter(|&p| p != i) {
            parents.push((i, parent));
        }
    }
    for &(i, parent) in &parents {
        comments[i].parent = Some(comments[parent].id.clone());
    }
    // A reply's thread is its root's, whatever the reply itself says; a
    // cycle stops at the comment that closes it.
    for &(i, _) in &parents {
        let mut root = i;
        for _ in 0..parents.len() {
            match parents.iter().find(|(c, _)| *c == root) {
                Some(&(_, parent)) if parent != i => root = parent,
                _ => break,
            }
        }
        comments[i].resolved = comments[root].resolved;
    }
}

/// A run property that is on: present, unless its value turns it off.
fn on(node: Node<'_, '_>) -> bool {
    !matches!(attr(node, "val"), "0" | "false" | "off")
}

/// Counts the passages of hidden text in a part and, with `edits`, lifts
/// it: a run's `w:vanish` becomes a dotted underline in muted ink (its own
/// underline and colour give way) with an explicit `vanish` off, so a
/// character style's `vanish` cannot hide it again, and a hidden paragraph
/// mark shows.
fn hidden_text(doc: &roxmltree::Document<'_>, mut edits: Option<&mut Vec<Edit>>) -> usize {
    let w = doc
        .root_element()
        .tag_name()
        .namespace()
        .unwrap_or_default();
    let mut count = 0;
    let mut in_passage = false;
    for node in doc.descendants() {
        if word(node, "p") {
            in_passage = false;
        }
        if word(node, "r") && draws_text(node) {
            let hidden = node
                .children()
                .find(|c| word(*c, "rPr"))
                .and_then(|p| p.children().find(|c| word(*c, "vanish")))
                .is_some_and(on);
            if hidden && !in_passage {
                count += 1;
            }
            in_passage = hidden;
        }
        let Some(edits) = edits.as_deref_mut() else {
            continue;
        };
        if !(word(node, "vanish") && on(node)) {
            continue;
        }
        let Some(properties) = node.parent().filter(|p| word(*p, "rPr")) else {
            continue;
        };
        let owner = properties.parent();
        if owner.is_some_and(|o| word(o, "r")) {
            for old in properties
                .children()
                .filter(|c| word(*c, "u") || word(*c, "color"))
            {
                edits.push(Edit {
                    start: old.range().start,
                    end: old.range().end,
                    rank: 1,
                    text: String::new(),
                });
            }
            edits.push(Edit {
                start: node.range().start,
                end: node.range().end,
                rank: 1,
                text: format!(
                    "<mdocw:u xmlns:mdocw=\"{w}\" mdocw:val=\"dotted\"/><mdocw:color xmlns:mdocw=\"{w}\" mdocw:val=\"{HIDDEN_INK}\"/><mdocw:vanish xmlns:mdocw=\"{w}\" mdocw:val=\"0\"/>"
                ),
            });
        } else if owner.is_some_and(|o| word(o, "pPr")) {
            edits.push(Edit {
                start: node.range().start,
                end: node.range().end,
                rank: 1,
                text: String::new(),
            });
        }
    }
    count
}

/// The muted ink of hidden text shown: a grey that still reads at 4.5:1 on
/// white paper and stays muted under the themed page tone.
const HIDDEN_INK: &str = "6F6F6F";

/// An edit to a part's text: replace `start..end` with `text`. `rank` orders
/// edits at one offset: closing a link before what follows it, opening one
/// after what precedes it.
struct Edit {
    start: usize,
    end: usize,
    rank: u8,
    text: String,
}

fn apply(source: &str, mut edits: Vec<Edit>) -> String {
    edits.sort_by_key(|e| (e.start, e.rank));
    let mut out = String::with_capacity(source.len() + edits.len() * 64);
    let mut cursor = 0;
    for edit in edits {
        if edit.start >= cursor {
            out.push_str(&source[cursor..edit.start]);
            cursor = edit.start;
        }
        out.push_str(&edit.text);
        cursor = cursor.max(edit.end);
    }
    out.push_str(&source[cursor..]);
    out
}

#[derive(Default)]
struct Markers {
    /// The relationship IDs' prefix, which no existing ID starts with.
    prefix: String,
    list: Vec<Marker>,
    /// Marker and the hyperlink target it carries → relationship index.
    links: Vec<(usize, Option<String>)>,
}

impl Markers {
    /// The relationship index for a run under `comments` inside a link to `target`.
    fn relationship(&mut self, marker: Marker, target: Option<String>) -> usize {
        let k = match self.list.iter().position(|m| *m == marker) {
            Some(k) => k,
            None => {
                self.list.push(marker);
                self.list.len() - 1
            }
        };
        let link = (k, target);
        match self.links.iter().position(|l| *l == link) {
            Some(i) => i,
            None => {
                self.links.push(link);
                self.links.len() - 1
            }
        }
    }

    /// The body's relationships with one external hyperlink per marker link.
    fn add_relationships(&self, rels: &str) -> Result<String, PreviewError> {
        let close = rels
            .rfind("</")
            .ok_or_else(|| PreviewError::Package("invalid relationships part".into()))?;
        let prefix = &self.prefix;
        let mut added = String::new();
        for (i, (k, target)) in self.links.iter().enumerate() {
            let uri = match target {
                Some(target) => format!("{COMMENT_SCHEME}{k};{target}"),
                None => format!("{COMMENT_SCHEME}{k}"),
            };
            added.push_str(&format!(
                "<Relationship xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\" Id=\"{prefix}{i}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink\" Target=\"{}\" TargetMode=\"External\"/>",
                escape_attribute(&uri)
            ));
        }
        let mut out = rels.to_owned();
        out.insert_str(close, &added);
        Ok(out)
    }
}

/// A relationship ID prefix no existing ID starts with.
fn relationship_prefix(rels: &str) -> String {
    let ids: Vec<String> = relationships_with_ids(rels).into_keys().collect();
    let mut prefix = String::from("mdocComment");
    while ids.iter().any(|id| id.starts_with(&prefix)) {
        prefix.push('_');
    }
    prefix
}

/// Every relationship's ID with its target (or nothing), as parsed.
fn relationships_with_ids(rels: &str) -> HashMap<String, Option<String>> {
    let Ok(doc) = roxmltree::Document::parse(rels) else {
        return HashMap::new();
    };
    doc.root_element()
        .children()
        .filter(|n| n.tag_name().name() == "Relationship")
        .filter_map(|n| {
            Some((
                n.attribute("Id")?.to_owned(),
                n.attribute("Target").map(str::to_owned),
            ))
        })
        .collect()
}

/// The body's relationships: ID → target.
fn relationships(rels: &str) -> HashMap<String, String> {
    relationships_with_ids(rels)
        .into_iter()
        .filter_map(|(id, target)| Some((id, target?)))
        .collect()
}

fn escape_attribute(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

const RELATIONSHIPS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const STRICT_RELATIONSHIPS: &str = "http://purl.oclc.org/ooxml/officeDocument/relationships";

/// Whether a run draws something with width: text, a tab or a symbol. A
/// break or an empty text has no rectangle to anchor to.
fn draws_text(run: Node<'_, '_>) -> bool {
    run.children().any(|c| {
        (word(c, "t") && c.text().is_some_and(|t| !t.is_empty()))
            || ["tab", "sym", "noBreakHyphen"]
                .iter()
                .any(|name| word(c, name))
    })
}

/// A node whose parent may hold a link around it.
fn wrappable(node: Node<'_, '_>) -> bool {
    node.parent().is_some_and(|p| {
        ["p", "hyperlink", "smartTag", "customXml", "sdtContent"]
            .iter()
            .any(|name| word(p, name))
    })
}

/// What a marker link wraps for a run: the run, or the whole simple field it
/// belongs to (a fldSimple's runs are the field itself, and its first run must
/// stay its direct child).
fn wrap_target<'a, 'input>(run: Node<'a, 'input>) -> Option<Node<'a, 'input>> {
    if wrappable(run) {
        return Some(run);
    }
    run.parent()
        .filter(|p| word(*p, "fldSimple") && wrappable(*p))
}

/// The target of the hyperlink a run sits in, as the renderer reads it: the
/// innermost link with a target (its relationship's, or `#bookmark` for an
/// in-document link) wins.
fn enclosing_link(run: Node<'_, '_>, links: &HashMap<String, String>) -> Option<String> {
    run.ancestors()
        .skip(1)
        .filter(|n| word(*n, "hyperlink"))
        .find_map(|link| {
            let rid = link
                .attribute((RELATIONSHIPS, "id"))
                .or_else(|| link.attribute((STRICT_RELATIONSHIPS, "id")));
            match rid {
                Some(rid) => links.get(rid).cloned(),
                None => link
                    .attribute((link.tag_name().namespace()?, "anchor"))
                    .map(|a| format!("#{a}")),
            }
        })
}

/// Fills in each comment's quote and collects the edits of the render copy:
/// comment markup goes, and with `markers` each commented run is wrapped in a
/// marker link and each point comment gets a marker where it was inserted.
fn quote_and_mark(
    doc: &roxmltree::Document<'_>,
    by_id: &HashMap<String, usize>,
    comments: &mut [DocxComment],
    links: &HashMap<String, String>,
    markers: Option<&mut Markers>,
    edits: &mut Vec<Edit>,
) {
    let w = doc
        .root_element()
        .tag_name()
        .namespace()
        .unwrap_or_default();
    let r = if w.starts_with("http://purl.oclc.org/") {
        STRICT_RELATIONSHIPS
    } else {
        RELATIONSHIPS
    };
    // Our own prefixes, declared on each inserted element: the run inside keeps
    // whatever prefixes the document gave it.
    let open = |rel: usize, id_prefix: &str| {
        format!(
            "<mdocw:hyperlink xmlns:mdocw=\"{w}\" xmlns:mdocr=\"{r}\" mdocr:id=\"{id_prefix}{rel}\">"
        )
    };
    let close = "</mdocw:hyperlink>";
    let mut active: Vec<usize> = Vec::new();
    // Ranged comments that drew something; the rest are point comments.
    let mut ranged = vec![false; comments.len()];
    let mut pending_runs: Vec<(Node<'_, '_>, Vec<usize>)> = Vec::new();
    for node in doc.descendants() {
        if word(node, "commentRangeStart") {
            if let Some(&i) = by_id.get(attr(node, "id"))
                && !active.contains(&i)
            {
                active.push(i);
            }
        } else if word(node, "commentRangeEnd") {
            if let Some(&i) = by_id.get(attr(node, "id")) {
                active.retain(|a| *a != i);
            }
        } else if word(node, "t") {
            for &i in &active {
                comments[i].quote.push_str(node.text().unwrap_or_default());
            }
        } else if word(node, "p") || word(node, "br") || word(node, "tab") {
            for &i in &active {
                if !comments[i].quote.is_empty() {
                    comments[i].quote.push(' ');
                }
            }
        }
        if word(node, "r")
            && !active.is_empty()
            && draws_text(node)
            && let Some(target) = wrap_target(node)
        {
            for &i in &active {
                ranged[i] = true;
            }
            // A simple field's runs share one link: the comments over any of them.
            match pending_runs.last_mut() {
                Some((last, open_comments)) if *last == target => {
                    open_comments.extend(active.iter().copied());
                }
                _ => pending_runs.push((target, active.clone())),
            }
        }
        if word(node, "commentRangeStart")
            || word(node, "commentRangeEnd")
            || word(node, "commentReference")
        {
            edits.push(Edit {
                start: node.range().start,
                end: node.range().end,
                rank: 1,
                text: String::new(),
            });
        }
    }
    // Point comments have no selected range: show their paragraph as context.
    let mut points = Vec::new();
    for node in doc.descendants().filter(|n| word(*n, "commentReference")) {
        if let Some(&i) = by_id.get(attr(node, "id"))
            && !ranged[i]
        {
            if comments[i].quote.is_empty()
                && let Some(paragraph) = node.ancestors().find(|n| word(*n, "p"))
            {
                comments[i].quote = text(paragraph);
            }
            points.push((node, i));
        }
    }
    let Some(markers) = markers else {
        return;
    };
    let id_prefix = markers.prefix.clone();
    let id_prefix = id_prefix.as_str();
    for (run, mut open_comments) in pending_runs {
        open_comments.sort_unstable();
        open_comments.dedup();
        let rel = markers.relationship(
            Marker {
                comments: open_comments,
                point: false,
            },
            enclosing_link(run, links),
        );
        edits.push(Edit {
            start: run.range().start,
            end: run.range().start,
            rank: 2,
            text: open(rel, id_prefix),
        });
        edits.push(Edit {
            start: run.range().end,
            end: run.range().end,
            rank: 0,
            text: close.to_owned(),
        });
    }
    for (reference, i) in points {
        // The reference sits in a run of its own (or at a run's end); the
        // marker follows that run, as a zero-width text the line can carry.
        let Some(run) = reference.ancestors().find(|n| word(*n, "r")) else {
            continue;
        };
        if !wrappable(run) {
            continue;
        }
        let rel = markers.relationship(
            Marker {
                comments: vec![i],
                point: true,
            },
            enclosing_link(run, links),
        );
        // 1 pt, so the marker never raises its line, even where the font
        // lacks the character and another face draws it.
        edits.push(Edit {
            start: run.range().end,
            end: run.range().end,
            rank: 2,
            text: format!(
                "{}<mdocw:r><mdocw:rPr><mdocw:sz mdocw:val=\"2\"/><mdocw:szCs mdocw:val=\"2\"/></mdocw:rPr><mdocw:t>{POINT}</mdocw:t></mdocw:r>{close}",
                open(rel, id_prefix)
            ),
        });
    }
}

/// The point marker's text: zero width, no break.
const POINT: char = '\u{200B}';

/// The anchors the marks read from the rendered PDF give each comment, in
/// document order, with a range's rectangles on one line merged.
pub fn anchors(
    marks: &[LinkMark],
    markers: &[Marker],
    comments: &[DocxComment],
) -> Vec<CommentAnchor> {
    let mut out: Vec<CommentAnchor> = Vec::new();
    for mark in marks {
        let k = mark.target.split(';').next().unwrap_or_default();
        let Some(marker) = k.parse::<usize>().ok().and_then(|k| markers.get(k)) else {
            continue;
        };
        for &i in &marker.comments {
            let Some(comment) = comments.get(i) else {
                continue;
            };
            let anchor = match out
                .iter_mut()
                .find(|a| a.id == comment.id && a.page == mark.page)
            {
                Some(anchor) => anchor,
                None => {
                    out.push(CommentAnchor {
                        id: comment.id.clone(),
                        page: mark.page,
                        rects: Vec::new(),
                        point: marker.point,
                    });
                    out.last_mut().unwrap()
                }
            };
            add_rect(&mut anchor.rects, mark.rect);
        }
    }
    for anchor in &mut out {
        anchor.rects.sort_by(line_order);
        if anchor.point {
            // A zero-width space has no width to keep.
            for rect in &mut anchor.rects {
                rect.w = 0.0;
            }
        }
    }
    out.sort_by(|a, b| {
        a.page
            .cmp(&b.page)
            .then_with(|| line_order(&a.rects[0], &b.rects[0]))
    });
    out
}

/// Reading order: by line (tops within half a line height), then left to right.
fn line_order(a: &AnchorRect, b: &AnchorRect) -> std::cmp::Ordering {
    if same_line(a, b) {
        a.x.total_cmp(&b.x)
    } else {
        a.y.total_cmp(&b.y)
    }
}

fn same_line(a: &AnchorRect, b: &AnchorRect) -> bool {
    (a.y - b.y).abs() < a.h.min(b.h) / 2.0
}

/// Adds `rect`, merged into a rectangle on the same line that it touches.
fn add_rect(rects: &mut Vec<AnchorRect>, rect: AnchorRect) {
    // Within about a space of a 10 pt line on a Letter page.
    const GAP: f32 = 0.006;
    if let Some(line) = rects
        .iter_mut()
        .find(|r| same_line(r, &rect) && rect.x <= r.x + r.w + GAP && r.x <= rect.x + rect.w + GAP)
    {
        let x0 = line.x.min(rect.x);
        let y0 = line.y.min(rect.y);
        let x1 = (line.x + line.w).max(rect.x + rect.w);
        let y1 = (line.y + line.h).max(rect.y + rect.h);
        *line = AnchorRect {
            x: x0,
            y: y0,
            w: x1 - x0,
            h: y1 - y0,
        };
    } else {
        rects.push(rect);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body_of(copy: Vec<u8>) -> String {
        let mut zip = ZipArchive::new(Cursor::new(copy)).unwrap();
        let mut body = String::new();
        zip.by_name(BODY)
            .unwrap()
            .read_to_string(&mut body)
            .unwrap();
        body
    }

    #[test]
    fn rendering_copy_removes_comment_markup_without_changing_source_text() {
        let bytes = include_bytes!("../../../../tests/fixtures/docx-preview/comments.docx");
        let prepared = prepare(bytes, false).unwrap();
        assert_eq!(prepared.comments.len(), 6);
        let copy = prepared.render.unwrap();
        let body = body_of(copy.clone());
        let doc = roxmltree::Document::parse(&body).unwrap();
        assert!(!doc.descendants().any(|n| matches!(
            n.tag_name().name(),
            "commentRangeStart" | "commentRangeEnd" | "commentReference"
        )));
        assert_eq!(
            text(doc.root_element()).replace('\u{200B}', ""),
            "First quoted paragraph.\nSecond quoted paragraph.\nPoint comment context.\nA settled clause. Visible text with a hidden drafting note."
        );
        let mut zip = ZipArchive::new(Cursor::new(copy)).unwrap();
        let mut xml = String::new();
        zip.by_name("word/comments.xml")
            .unwrap()
            .read_to_string(&mut xml)
            .unwrap();
        let doc = roxmltree::Document::parse(&xml).unwrap();
        assert!(!doc.descendants().any(|n| word(n, "comment")));
    }

    #[test]
    fn no_comments_keeps_original_render_input() {
        let prepared = prepare(
            include_bytes!("../../../../tests/fixtures/docx-preview/coverage.docx"),
            true,
        )
        .unwrap();
        assert!(prepared.render.is_none());
        assert!(prepared.comments.is_empty());
    }

    fn part(copy: &[u8], name: &str) -> String {
        let mut zip = ZipArchive::new(Cursor::new(copy)).unwrap();
        let mut xml = String::new();
        zip.by_name(name).unwrap().read_to_string(&mut xml).unwrap();
        xml
    }

    /// Each run's text with the target of the marker link around it, if any.
    fn marked_runs(copy: &[u8]) -> Vec<(String, Option<String>)> {
        let rels = relationships(&part(copy, BODY_RELS));
        let body = part(copy, BODY);
        let doc = roxmltree::Document::parse(&body).unwrap();
        doc.descendants()
            .filter(|n| word(*n, "r"))
            .map(|run| {
                let text = run
                    .children()
                    .filter(|c| word(*c, "t"))
                    .filter_map(|c| c.text())
                    .collect();
                let target = run
                    .parent()
                    .filter(|p| word(*p, "hyperlink"))
                    .and_then(|p| p.attribute((RELATIONSHIPS, "id")))
                    .and_then(|id| rels.get(id))
                    .filter(|t| t.starts_with(COMMENT_SCHEME))
                    .cloned();
                (text, target)
            })
            .collect()
    }

    /// The ids of the comments a marker target stands for.
    fn ids(prepared: &Prepared, target: &str) -> Vec<String> {
        let k: usize = target[COMMENT_SCHEME.len()..]
            .split(';')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        prepared.markers[k]
            .comments
            .iter()
            .map(|&i| prepared.comments[i].id.clone())
            .collect()
    }

    #[test]
    fn render_copy_marks_every_commented_run() {
        let bytes = include_bytes!("../../../../tests/fixtures/docx-preview/comment-anchors.docx");
        let prepared = prepare(bytes, false).unwrap();
        let copy = prepared.render.clone().unwrap();
        let runs = marked_runs(&copy);
        let marker = |text: &str| {
            runs.iter()
                .find(|(t, _)| t == text)
                .unwrap_or_else(|| panic!("no run {text:?}"))
                .1
                .clone()
        };
        let marked = |text: &str| ids(&prepared, &marker(text).expect(text));
        // Several lines, two paragraphs, a cell, Russian, across a page break.
        assert_eq!(marked(&runs[1].0), ["0"]);
        assert_eq!(
            marked("First paragraph of a range over two paragraphs."),
            ["1"]
        );
        assert_eq!(marked("Second paragraph of that range."), ["1"]);
        assert_eq!(marked("Commented cell"), ["7"]);
        assert_eq!(marked("Срок оплаты — тридцать дней."), ["8"]);
        assert_eq!(marked("Before the page break."), ["12"]);
        assert_eq!(marked("After the page break."), ["12"]);
        // Overlapping comments: the shared run stands for both.
        assert_eq!(marked("Alpha beta "), ["2"]);
        assert_eq!(marked("gamma"), ["2", "3"]);
        assert_eq!(marked(" delta"), ["3"]);
        assert_eq!(marker(" epsilon."), None);
        // A run inside a real link keeps its target after the marker.
        assert!(
            marker("public register")
                .unwrap()
                .ends_with(";https://example.com/register")
        );
        assert!(
            marker("register")
                .unwrap()
                .ends_with(";https://example.com/register")
        );
        assert_eq!(marked("register"), ["11"]);
        assert!(marker("clause 5").unwrap().ends_with(";#clause5"));
        // Point comments and an empty range get a zero-width marker of their own.
        let points: Vec<_> = runs
            .iter()
            .filter(|(t, _)| t == "\u{200B}")
            .map(|(_, target)| ids(&prepared, target.as_ref().unwrap()))
            .collect();
        assert_eq!(points, [["5"], ["6"]]);
        let point = |id: &str| {
            prepared
                .markers
                .iter()
                .find(|m| m.comments.iter().any(|&i| prepared.comments[i].id == id))
                .unwrap()
                .point
        };
        assert!(point("5") && point("6") && !point("0"));
        // A comment with no range and no reference has no marker.
        let nowhere = prepared.comments.iter().position(|c| c.id == "10").unwrap();
        assert!(
            !prepared
                .markers
                .iter()
                .any(|m| m.comments.contains(&nowhere))
        );
        // No comment markup is left, and only the marker runs were added.
        let body = part(&copy, BODY);
        let doc = roxmltree::Document::parse(&body).unwrap();
        assert!(
            !doc.descendants()
                .any(|n| n.tag_name().name().starts_with("comment"))
        );
        let source = part(bytes, BODY);
        let original = roxmltree::Document::parse(&source).unwrap();
        assert_eq!(
            text(doc.root_element()).replace('\u{200B}', ""),
            text(original.root_element())
        );
        // Each marker link is an external relationship the source never had.
        let rels = part(&copy, BODY_RELS);
        assert!(rels.contains(r#"Target="mdoc-comment:0" TargetMode="External""#));
        assert!(!part(bytes, BODY_RELS).contains(COMMENT_SCHEME));
    }

    #[test]
    fn marker_ids_never_reuse_an_existing_relationship_id() {
        let rels = |ids: &str| format!("<Relationships>{ids}</Relationships>");
        assert_eq!(
            relationship_prefix(&rels(r#"<Relationship Id="rId1"/>"#)),
            "mdocComment"
        );
        // Single quotes and character references are the same IDs.
        assert_eq!(
            relationship_prefix(&rels(
                r#"<Relationship Id='mdocComment0'/><Relationship Id="&#109;docComment_1"/>"#
            )),
            "mdocComment__"
        );
    }

    /// A package whose body is `body`, with comments 1 to 4 and a link `ext`.
    fn package_with(body: &str) -> Vec<u8> {
        let w = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
        let r = RELATIONSHIPS;
        let document = format!(
            r#"<w:document xmlns:w="{w}" xmlns:r="{r}"><w:body>{body}</w:body></w:document>"#
        );
        let comments = format!(
            r#"<w:comments xmlns:w="{w}">{}</w:comments>"#,
            (1..=4)
                .map(|i| format!(
                    r#"<w:comment w:id="{i}"><w:p><w:r><w:t>c{i}</w:t></w:r></w:p></w:comment>"#
                ))
                .collect::<String>()
        );
        let rels = format!(
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="ext" Type="{r}/hyperlink" Target="https://example.com/a" TargetMode="External"/></Relationships>"#
        );
        super::super::tests::package(&[
            (BODY, document.as_bytes()),
            ("word/comments.xml", comments.as_bytes()),
            (BODY_RELS, rels.as_bytes()),
        ])
    }

    #[test]
    fn fields_nested_links_and_empty_ranges_still_get_their_place() {
        let reference = |i: u32| format!(r#"<w:r><w:commentReference w:id="{i}"/></w:r>"#);
        let range = |i: u32, inner: &str| {
            format!(
                r#"<w:commentRangeStart w:id="{i}"/>{inner}<w:commentRangeEnd w:id="{i}"/>{}"#,
                reference(i)
            )
        };
        let body = [
            // 1: over a simple field, whose run must stay its direct child.
            range(
                1,
                r#"<w:fldSimple w:instr=" DATE "><w:r><w:t>18.11.2026</w:t></w:r></w:fldSimple>"#,
            ),
            // 2: in a targetless link inside a real one, which docxide follows.
            format!(
                r#"<w:hyperlink r:id="ext"><w:hyperlink>{}</w:hyperlink></w:hyperlink>"#,
                range(2, "<w:r><w:t>inner</w:t></w:r>")
            ),
            // 3 and 4: ranges holding only an empty text and only a break.
            range(3, "<w:r><w:t/></w:r>"),
            range(4, "<w:r><w:br/></w:r>"),
        ]
        .map(|p| format!("<w:p>{p}</w:p>"))
        .concat();
        let prepared = prepare(&package_with(&body), false).unwrap();
        let copy = prepared.render.clone().unwrap();
        let body = part(&copy, BODY);
        let doc = roxmltree::Document::parse(&body).unwrap();
        // The field sits whole inside its marker link.
        let field = doc.descendants().find(|n| word(*n, "fldSimple")).unwrap();
        let link = field.parent().unwrap();
        assert!(word(link, "hyperlink"));
        let rels = relationships(&part(&copy, BODY_RELS));
        let target =
            |node: Node<'_, '_>| rels[node.attribute((RELATIONSHIPS, "id")).unwrap()].clone();
        assert_eq!(ids(&prepared, &target(link)), ["1"]);
        // The inner run keeps the outer link's target after its marker.
        let inner = marked_runs(&copy)
            .into_iter()
            .find(|(t, _)| t == "inner")
            .unwrap()
            .1
            .unwrap();
        assert!(inner.ends_with(";https://example.com/a"), "{inner}");
        // Ranges with nothing to draw become point comments.
        let point = |id: &str| {
            prepared
                .markers
                .iter()
                .find(|m| m.comments.iter().any(|&i| prepared.comments[i].id == id))
                .map(|m| m.point)
        };
        assert_eq!(point("3"), Some(true));
        assert_eq!(point("4"), Some(true));
        assert_eq!(point("1"), Some(false));
    }

    fn rect(x: f32, y: f32, w: f32) -> AnchorRect {
        AnchorRect { x, y, w, h: 0.014 }
    }

    fn mark(page: usize, rect: AnchorRect, target: &str) -> LinkMark {
        LinkMark {
            page,
            rect,
            target: target.into(),
        }
    }

    #[test]
    fn anchors_merge_a_line_and_follow_the_pages() {
        let comment = |id: &str| DocxComment {
            id: id.into(),
            author: String::new(),
            date: String::new(),
            text: String::new(),
            quote: String::new(),
            parent: None,
            resolved: false,
        };
        let comments = [comment("a"), comment("b"), comment("p")];
        let markers = [
            Marker {
                comments: vec![0],
                point: false,
            },
            Marker {
                comments: vec![0, 1],
                point: false,
            },
            Marker {
                comments: vec![2],
                point: true,
            },
        ];
        let marks = [
            mark(1, rect(0.1, 0.1, 0.2), "0"),
            mark(0, rect(0.1, 0.5, 0.2), "0"),
            // Touches the run before it on the same line: one rectangle.
            mark(0, rect(0.302, 0.5, 0.1), "1;https://example.com"),
            mark(0, rect(0.1, 0.52, 0.3), "1"),
            mark(0, rect(0.4, 0.3, 0.0), "2"),
            // Not a marker this render made.
            mark(0, rect(0.1, 0.1, 0.1), "9"),
            mark(0, rect(0.1, 0.1, 0.1), "x"),
        ];
        let anchors = anchors(&marks, &markers, &comments);
        let got: Vec<_> = anchors
            .iter()
            .map(|a| (a.id.as_str(), a.page, a.rects.len(), a.point))
            .collect();
        assert_eq!(
            got,
            [
                ("p", 0, 1, true),
                ("a", 0, 2, false),
                ("b", 0, 2, false),
                ("a", 1, 1, false)
            ]
        );
        let a = &anchors[1].rects;
        assert!((a[0].x - 0.1).abs() < 1e-6 && (a[0].w - 0.302).abs() < 1e-6);
        assert!((a[1].y - 0.52).abs() < 1e-6);
        assert_eq!(anchors[0].rects[0].w, 0.0);
    }

    #[test]
    fn replies_and_resolved_threads_come_from_comments_extended() {
        let bytes = include_bytes!("../../../../tests/fixtures/docx-preview/comments.docx");
        let prepared = prepare(bytes, false).unwrap();
        let comment = |id: &str| prepared.comments.iter().find(|c| c.id == id).unwrap();
        assert_eq!(comment("5").parent.as_deref(), Some("0"));
        assert!(comment("4").resolved);
        for id in ["0", "1", "2", "3", "5"] {
            assert!(!comment(id).resolved, "{id}");
        }
        assert!(
            ["0", "1", "2", "3", "4"]
                .iter()
                .all(|id| comment(id).parent.is_none())
        );
    }

    #[test]
    fn a_reply_shares_its_threads_state_and_a_cycle_ends() {
        let comment = |id: &str| DocxComment {
            id: id.into(),
            author: String::new(),
            date: String::new(),
            text: String::new(),
            quote: String::new(),
            parent: None,
            resolved: false,
        };
        let mut comments = [comment("a"), comment("b"), comment("c"), comment("d")];
        let by_paragraph = [("0A", 0), ("0B", 1), ("0C", 2), ("0D", 3)]
            .into_iter()
            .map(|(p, i)| (p.to_owned(), i))
            .collect();
        threads(
            &format!(
                r#"<w15:commentsEx xmlns:w15="{W15}"><w15:commentEx w15:paraId="0a" w15:done="1"/><w15:commentEx w15:paraId="0B" w15:paraIdParent="0A" w15:done="0"/><w15:commentEx w15:paraId="0C" w15:paraIdParent="0D"/><w15:commentEx w15:paraId="0D" w15:paraIdParent="0C"/><w15:commentEx w15:paraId="FF" w15:done="1"/></w15:commentsEx>"#
            ),
            &by_paragraph,
            &mut comments,
        );
        assert!(comments[0].resolved && comments[1].resolved);
        assert_eq!(comments[1].parent.as_deref(), Some("a"));
        assert_eq!(comments[2].parent.as_deref(), Some("d"));
        assert!(!comments[2].resolved && !comments[3].resolved);
    }

    #[test]
    fn hidden_text_is_counted_always_and_drawn_only_when_shown() {
        let bytes = include_bytes!("../../../../tests/fixtures/docx-preview/comments.docx");
        let hidden_run = |body: &str| {
            let doc = roxmltree::Document::parse(body).unwrap();
            let run = doc
                .descendants()
                .find(|n| word(*n, "r") && text(*n).contains("hidden drafting note"))
                .unwrap();
            let properties = run.children().find(|c| word(*c, "rPr")).unwrap();
            properties
                .children()
                .filter(|c| c.is_element())
                .map(|c| format!("{}={}", c.tag_name().name(), attr(c, "val")))
                .collect::<Vec<_>>()
        };
        let plain = prepare(bytes, false).unwrap();
        assert_eq!(plain.hidden, 1);
        assert_eq!(hidden_run(&body_of(plain.render.unwrap())), ["vanish="]);
        let shown = prepare(bytes, true).unwrap();
        assert_eq!(shown.hidden, 1);
        assert_eq!(
            hidden_run(&body_of(shown.render.unwrap())),
            ["u=dotted", &format!("color={HIDDEN_INK}"), "vanish=0"]
        );
    }

    #[test]
    fn hidden_passages_merge_adjacent_runs_and_lift_their_marks() {
        let w = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
        let body = format!(
            r#"<w:document xmlns:w="{w}"><w:body><w:p><w:pPr><w:rPr><w:vanish/></w:rPr></w:pPr><w:r><w:rPr><w:vanish/><w:color w:val="FF0000"/><w:u w:val="single"/></w:rPr><w:t>one</w:t></w:r><w:bookmarkStart w:id="1"/><w:r><w:rPr><w:vanish w:val="1"/></w:rPr><w:t>still one</w:t></w:r><w:r><w:t>shown</w:t></w:r><w:r><w:rPr><w:vanish/></w:rPr><w:t>two</w:t></w:r></w:p><w:p><w:r><w:rPr><w:vanish/></w:rPr><w:t>three</w:t></w:r><w:r><w:rPr><w:vanish w:val="false"/></w:rPr><w:t>not hidden</w:t></w:r></w:p></w:body></w:document>"#
        );
        let package = super::super::tests::package(&[(BODY, body.as_bytes())]);
        let plain = prepare(&package, false).unwrap();
        assert_eq!(plain.hidden, 3);
        assert!(plain.render.is_none(), "nothing to change");
        let shown = body_of(prepare(&package, true).unwrap().render.unwrap());
        let doc = roxmltree::Document::parse(&shown).unwrap();
        // Each hidden run's vanish is now explicitly off; the run's own
        // colour and underline gave way.
        let vanish: Vec<_> = doc
            .descendants()
            .filter(|n| word(*n, "vanish"))
            .map(|n| attr(n, "val"))
            .collect();
        assert_eq!(vanish, ["0", "0", "0", "0", "false"]);
        assert!(!shown.contains("FF0000") && !shown.contains("single"));
        assert_eq!(
            doc.descendants().filter(|n| word(*n, "u")).count(),
            4,
            "each hidden run is underlined"
        );
        assert_eq!(
            text(doc.root_element()),
            "onestill oneshowntwo\nthreenot hidden"
        );
    }
}
