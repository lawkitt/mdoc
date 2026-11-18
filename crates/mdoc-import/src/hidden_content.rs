//! Hidden Office content (recognition grill 2026-11-01, round 3): what a Word
//! document holds that its Markdown must not carry. The pinned AnyDoc leaves
//! out comments, deleted and moved-from tracked changes and document
//! properties, with no option to keep them; `import`'s tests pin that on
//! `handmade-hidden-content.docx`. This module counts the comments left
//! out of Word and OpenDocument text documents, for the import note.
use std::{
    collections::HashMap,
    io::{ErrorKind, Read, Seek},
    path::Path,
};
use zip::ZipArchive;

/// The most of a comments part read to find its root element: Word's root
/// tags carry a few kilobytes of namespace declarations.
const HEAD: u64 = 64 * 1024;
/// The most of a relationships part read.
const RELS_LIMIT: u64 = 4 * 1024 * 1024;
/// The most comment data scanned per document; past it the count is a lower
/// bound ("At least N").
const BUDGET: u64 = 256 * 1024 * 1024;
/// WordprocessingML, Transitional and Strict.
const WORD_NAMESPACES: [&str; 2] = [
    "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
    "http://purl.oclc.org/ooxml/wordprocessingml/main",
];
/// OpenDocument's office vocabulary, which `annotation` belongs to.
const OFFICE_NAMESPACE: [&str; 1] = ["urn:oasis:names:tc:opendocument:xmlns:office:1.0"];

/// Where a format keeps its comments: the root element of the part that
/// holds them, the namespaces that root may be bound to, and the element of
/// one comment, in the root's namespace.
struct Vocabulary {
    root: &'static str,
    namespaces: &'static [&'static str],
    comment: &'static str,
}

const WORD: Vocabulary = Vocabulary {
    root: "comments",
    namespaces: &WORD_NAMESPACES,
    comment: "comment",
};

/// ODF text documents keep comments in the body (`office:annotation`; a
/// range's end is `office:annotation-end`, which does not count).
const ODT: Vocabulary = Vocabulary {
    root: "document-content",
    namespaces: &OFFICE_NAMESPACE,
    comment: "annotation",
};

/// The comments a Word or OpenDocument text document holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Comments {
    pub count: usize,
    /// The scan stopped at its limit, so there may be more.
    pub at_least: bool,
}

impl Comments {
    /// The import note's line for comments left out, if any, as the apps
    /// show it until they show the left-out line.
    pub fn note(self) -> Option<String> {
        self.phrase("not imported")
    }

    /// The left-out line's count of comments that no rendering shows.
    pub fn not_shown(self) -> Option<String> {
        self.phrase("not shown")
    }

    fn phrase(self, what: &str) -> Option<String> {
        let at_least = if self.at_least { "At least " } else { "" };
        match self.count {
            0 => None,
            1 => Some(format!("{at_least}1 comment {what}.")),
            n => Some(format!("{at_least}{n} comments {what}.")),
        }
    }

    /// Both counts together.
    pub fn and(self, other: Self) -> Self {
        Self {
            count: self.count + other.count,
            at_least: self.at_least || other.at_least,
        }
    }
}

/// The comments in the parts a Word package's main document names as its
/// comments, or none when `path` is not a Word package or cannot be read.
/// Best effort: the conversion has already succeeded, so a part it cannot
/// read is skipped rather than failing it.
pub fn docx_comments(path: &Path) -> Comments {
    std::fs::File::open(path)
        .ok()
        .map_or_else(Comments::default, package_comments)
}

fn package_comments<R: Read + Seek>(reader: R) -> Comments {
    let Ok(mut archive) = ZipArchive::new(reader) else {
        return Comments::default();
    };
    // OPC part names are case-insensitive.
    let names: HashMap<String, usize> = (0..archive.len())
        .filter_map(|index| {
            let name = archive.name_for_index(index)?.ok()?;
            Some((name.to_ascii_lowercase(), index))
        })
        .collect();
    let Some(main) = targets(&mut archive, &names, "", "/officeDocument")
        .into_iter()
        .next()
    else {
        return Comments::default();
    };
    let mut budget = BUDGET;
    let mut total = Comments::default();
    for part in targets(&mut archive, &names, &main, "/comments") {
        let Some(&index) = names.get(&part.to_ascii_lowercase()) else {
            continue;
        };
        let Ok(entry) = archive.by_index(index) else {
            continue;
        };
        let found = count_part(entry, &mut budget, &WORD);
        total.count += found.count;
        total.at_least |= found.at_least;
    }
    total
}

/// The comments in an OpenDocument text's body (`content.xml`), or none
/// when `path` is not an OpenDocument text package (by its `mimetype`
/// entry, so a spreadsheet's cell notes don't count) or cannot be read. Best
/// effort, like [`docx_comments`]. AnyDoc leaves them out of the Markdown.
pub fn odt_comments(path: &Path) -> Comments {
    std::fs::File::open(path)
        .ok()
        .map_or_else(Comments::default, odf_package_comments)
}

/// ODF text, text template and master document.
const ODT_MIMETYPE: &str = "application/vnd.oasis.opendocument.text";

fn odf_package_comments<R: Read + Seek>(reader: R) -> Comments {
    let Ok(mut archive) = ZipArchive::new(reader) else {
        return Comments::default();
    };
    let mut mimetype = Vec::new();
    let read = archive
        .by_name("mimetype")
        .map(|entry| entry.take(256).read_to_end(&mut mimetype));
    if !matches!(read, Ok(Ok(_))) || !mimetype.starts_with(ODT_MIMETYPE.as_bytes()) {
        return Comments::default();
    }
    let Ok(entry) = archive.by_name("content.xml") else {
        return Comments::default();
    };
    count_part(entry, &mut BUDGET.clone(), &ODT)
}

/// The internal targets, as part names, of `source`'s relationships whose
/// type ends in `kind` (`""` is the package itself).
fn targets<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    names: &HashMap<String, usize>,
    source: &str,
    kind: &str,
) -> Vec<String> {
    let (dir, file) = source.rsplit_once('/').unwrap_or(("", source));
    let rels = match (source.is_empty(), dir.is_empty()) {
        (true, _) => "_rels/.rels".to_owned(),
        (false, true) => format!("_rels/{file}.rels"),
        (false, false) => format!("{dir}/_rels/{file}.rels"),
    };
    let Some(&index) = names.get(&rels.to_ascii_lowercase()) else {
        return Vec::new();
    };
    let mut bytes = Vec::new();
    let read = archive
        .by_index(index)
        .map(|entry| entry.take(RELS_LIMIT).read_to_end(&mut bytes));
    if !matches!(read, Ok(Ok(_))) {
        return Vec::new();
    }
    let xml = decode(&bytes);
    start_tags(&xml)
        .filter(|tag| local_name(tag) == "Relationship")
        .filter(|tag| attribute(tag, "Type").is_some_and(|kind_uri| kind_uri.ends_with(kind)))
        .filter(|tag| attribute(tag, "TargetMode") != Some("External"))
        .filter_map(|tag| resolve(dir, attribute(tag, "Target")?))
        .collect()
}

/// A relationship target as a part name: absolute (`/word/x.xml`) or
/// relative to the source's folder, with `.` and `..` resolved.
fn resolve(dir: &str, target: &str) -> Option<String> {
    let joined = match target.strip_prefix('/') {
        Some(absolute) => absolute.to_owned(),
        None if dir.is_empty() => target.to_owned(),
        None => format!("{dir}/{target}"),
    };
    let mut segments = Vec::new();
    for segment in joined.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            other => segments.push(other),
        }
    }
    (!segments.is_empty()).then(|| segments.join("/"))
}

/// The comments in one part, read to its end within `budget`. A part whose
/// root is not `vocabulary`'s counts none. The count is of comment start
/// tags in the root's namespace prefix, which is how Word and OpenDocument
/// editors write them; text inside an XML comment that looks like one would
/// count too, which they never write.
fn count_part(mut part: impl Read, budget: &mut u64, vocabulary: &Vocabulary) -> Comments {
    let mut head = Vec::new();
    if (&mut part).take(HEAD).read_to_end(&mut head).is_err() {
        return Comments::default();
    }
    let text = decode(&head);
    let Some(prefix) = root_prefix(&text, vocabulary) else {
        return Comments::default();
    };
    let comment = vocabulary.comment;
    let element = if prefix.is_empty() {
        format!("<{comment}")
    } else {
        format!("<{prefix}:{comment}")
    };
    let (encode, step): (fn(&str) -> Vec<u8>, usize) = match head.as_slice() {
        [0xFF, 0xFE, ..] => (|s| s.encode_utf16().flat_map(u16::to_le_bytes).collect(), 2),
        [0xFE, 0xFF, ..] => (|s| s.encode_utf16().flat_map(u16::to_be_bytes).collect(), 2),
        _ => (|s| s.as_bytes().to_vec(), 1),
    };
    let mut counter = Counter {
        // The element name ends at whitespace, `/` or `>`.
        patterns: [' ', '\t', '\r', '\n', '/', '>']
            .iter()
            .map(|end| encode(&format!("{element}{end}")))
            .collect(),
        step,
        carry: Vec::new(),
        offset: 0,
        count: 0,
    };
    *budget = budget.saturating_sub(head.len() as u64);
    counter.feed(&head);
    let mut chunk = vec![0; 64 * 1024];
    loop {
        let room = chunk
            .len()
            .min(usize::try_from(*budget).unwrap_or(usize::MAX));
        if room == 0 {
            // At the limit: complete only if nothing is left.
            let at_least = !matches!(part.read(&mut [0; 1]), Ok(0));
            return Comments {
                count: counter.count,
                at_least,
            };
        }
        match part.read(&mut chunk[..room]) {
            Ok(0) => break,
            Ok(read) => {
                *budget -= read as u64;
                counter.feed(&chunk[..read]);
            }
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(_) => {
                return Comments {
                    count: counter.count,
                    at_least: true,
                };
            }
        }
    }
    Comments {
        count: counter.count,
        at_least: false,
    }
}

/// Counts equal-length byte patterns in a stream fed in chunks, at offsets
/// that are multiples of `step` (2 for UTF-16, so a match never straddles
/// two code units).
struct Counter {
    patterns: Vec<Vec<u8>>,
    step: usize,
    /// Bytes not yet checked as a match start, from `offset` in the stream.
    carry: Vec<u8>,
    offset: usize,
    count: usize,
}

impl Counter {
    fn feed(&mut self, bytes: &[u8]) {
        self.carry.extend_from_slice(bytes);
        let len = self.patterns[0].len();
        let mut at = (self.step - self.offset % self.step) % self.step;
        while at + len <= self.carry.len() {
            let window = &self.carry[at..at + len];
            if self.patterns.iter().any(|pattern| pattern == window) {
                self.count += 1;
            }
            at += self.step;
        }
        let checked = at.min(self.carry.len());
        self.carry.drain(..checked);
        self.offset += checked;
    }
}

/// The namespace prefix of `xml`'s root when it is `vocabulary`'s root
/// (`""` for a default namespace), checked by the namespace the prefix is
/// bound to.
fn root_prefix<'a>(xml: &'a str, vocabulary: &Vocabulary) -> Option<&'a str> {
    let root = start_tags(xml).next()?;
    if local_name(root) != vocabulary.root {
        return None;
    }
    let name = root.split(|c: char| c.is_whitespace() || c == '/').next()?;
    let prefix = name.rsplit_once(':').map_or("", |(prefix, _)| prefix);
    let declaration = if prefix.is_empty() {
        "xmlns".to_owned()
    } else {
        format!("xmlns:{prefix}")
    };
    attribute(root, &declaration)
        .filter(|namespace| vocabulary.namespaces.contains(namespace))
        .map(|_| prefix)
}

/// XML text from a part's bytes: UTF-16 when it starts with a byte-order
/// mark, otherwise UTF-8 (lossy, which keeps every ASCII tag intact).
fn decode(bytes: &[u8]) -> String {
    let utf16 = |unit: fn([u8; 2]) -> u16| {
        let units: Vec<u16> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| unit(*pair))
            .collect();
        String::from_utf16_lossy(&units)
    };
    match bytes {
        [0xFF, 0xFE, ..] => utf16(u16::from_le_bytes),
        [0xFE, 0xFF, ..] => utf16(u16::from_be_bytes),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// The text of each start tag in `xml` (between `<` and `>`), skipping end
/// tags, declarations, processing instructions, comments and CDATA. An
/// unclosed tag ends the iteration.
fn start_tags(xml: &str) -> impl Iterator<Item = &str> {
    let mut rest = xml;
    std::iter::from_fn(move || {
        loop {
            let open = rest.find('<')?;
            rest = &rest[open + 1..];
            let skip_to = |end: &str| rest.find(end).map(|at| at + end.len());
            let skipped = if rest.starts_with("!--") {
                skip_to("-->")?
            } else if rest.starts_with("![CDATA[") {
                skip_to("]]>")?
            } else if rest.starts_with(['!', '?', '/']) {
                skip_to(">")?
            } else {
                let close = rest.find('>')?;
                let tag = &rest[..close];
                rest = &rest[close + 1..];
                return Some(tag);
            };
            rest = &rest[skipped..];
        }
    })
}

/// A start tag's element name without its namespace prefix.
fn local_name(tag: &str) -> &str {
    let name = tag
        .split(|c: char| c.is_whitespace() || c == '/')
        .next()
        .unwrap_or_default();
    name.rsplit(':').next().unwrap_or(name)
}

/// The value of attribute `name` in a start tag's text.
fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let mut from = 0;
    while let Some(found) = tag[from..].find(name) {
        let start = from + found;
        from = start + name.len();
        if !tag[..start].ends_with(char::is_whitespace) {
            continue;
        }
        let Some(rest) = tag[from..].trim_start().strip_prefix('=') else {
            continue;
        };
        let rest = rest.trim_start();
        let quote = rest.chars().next().filter(|c| matches!(c, '"' | '\''))?;
        let value = &rest[1..];
        return value.find(quote).map(|end| &value[..end]);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};

    const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

    fn count(xml: &str) -> Comments {
        count_part(
            Cursor::new(xml.as_bytes().to_vec()),
            &mut BUDGET.clone(),
            &WORD,
        )
    }

    fn found(count: usize) -> Comments {
        Comments {
            count,
            at_least: false,
        }
    }

    #[test]
    fn counts_comments_in_the_word_namespace_only() {
        let part = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:comments xmlns:w="{W}"><w:comment w:id="0"><w:p><w:r><w:t>a</w:t></w:r></w:p></w:comment>
<w:comment w:id="1"/><w:comment
 w:id="2"></w:comment><w:commentsEx/></w:comments>"#
        );
        assert_eq!(count(&part), found(3));
        let strict = r#"<comments xmlns="http://purl.oclc.org/ooxml/wordprocessingml/main"><comment id="0"/></comments>"#;
        assert_eq!(count(strict), found(1));
        // A spreadsheet's notes, another vocabulary's `comments` that merely
        // declares Word's namespace, the document and an empty part.
        let sheet = r#"<comments xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><commentList><comment ref="A1"/></commentList></comments>"#;
        assert_eq!(count(sheet), found(0));
        let custom = format!(r#"<comments xmlns="urn:app" xmlns:w="{W}"><comment/></comments>"#);
        assert_eq!(count(&custom), found(0));
        assert_eq!(count(&format!(r#"<w:document xmlns:w="{W}"/>"#)), found(0));
        assert_eq!(count(""), found(0));
    }

    #[test]
    fn counts_to_the_end_across_chunks_in_utf8_and_utf16() {
        // About 400 KiB: past the head and many read chunks.
        let comments = 10_000;
        let part = format!(
            r#"<w:comments xmlns:w="{W}">{}</w:comments>"#,
            (0..comments)
                .map(|id| format!(r#"<w:comment w:id="{id}"><w:p/></w:comment>"#))
                .collect::<String>()
        );
        assert_eq!(count(&part), found(comments));
        for (bom, unit) in [
            ([0xFF, 0xFE], u16::to_le_bytes as fn(u16) -> [u8; 2]),
            ([0xFE, 0xFF], u16::to_be_bytes),
        ] {
            let mut bytes = bom.to_vec();
            bytes.extend(part.encode_utf16().flat_map(unit));
            let counted = count_part(Cursor::new(bytes), &mut BUDGET.clone(), &WORD);
            assert_eq!(counted, found(comments));
        }
    }

    #[test]
    fn a_count_cut_short_says_at_least() {
        let part = format!(
            r#"<w:comments xmlns:w="{W}">{}</w:comments>"#,
            r#"<w:comment w:id="0"/>"#.repeat(5_000)
        );
        let size = part.len() as u64;
        let mut budget = size - 1_000;
        let short = count_part(Cursor::new(part.clone().into_bytes()), &mut budget, &WORD);
        assert!(short.at_least && short.count < 5_000, "{short:?}");
        assert_eq!(
            short.note().unwrap(),
            format!("At least {} comments not imported.", short.count)
        );
        // Exactly enough budget is a complete count.
        assert_eq!(
            count_part(Cursor::new(part.into_bytes()), &mut size.clone(), &WORD),
            found(5_000)
        );
    }

    fn zipped(parts: &[(&str, String)]) -> Cursor<Vec<u8>> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, data) in parts {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(data.as_bytes()).unwrap();
        }
        Cursor::new(zip.finish().unwrap().into_inner())
    }

    fn package(parts: &[(&str, String)]) -> Comments {
        package_comments(zipped(parts))
    }

    /// ODT comments: each `annotation` in the office namespace, replies
    /// included, never a range's `annotation-end`; another root, another
    /// vocabulary or another part counts none.
    #[test]
    fn counts_opendocument_annotations() {
        const OFFICE: &str = "urn:oasis:names:tc:opendocument:xmlns:office:1.0";
        let body = |root: &str, prefix: &str, namespace: &str| {
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<{prefix}:{root} xmlns:{prefix}="{namespace}" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"><{prefix}:body><{prefix}:text>
<text:p>Lamb<{prefix}:annotation {prefix}:name="a"><text:p>first</text:p></{prefix}:annotation>da<{prefix}:annotation-end {prefix}:name="a"/></text:p>
<text:p><{prefix}:annotation><text:p>reply</text:p></{prefix}:annotation><{prefix}:annotation/></text:p>
</{prefix}:text></{prefix}:body></{prefix}:{root}>"#
            )
        };
        let text = ("mimetype", ODT_MIMETYPE.to_owned());
        let content =
            |xml: String| odf_package_comments(zipped(&[text.clone(), ("content.xml", xml)]));
        assert_eq!(
            content(body("document-content", "office", OFFICE)),
            found(3)
        );
        assert_eq!(content(body("document-content", "o", OFFICE)), found(3));
        assert_eq!(
            content(body("document-content", "office", "urn:app")),
            found(0)
        );
        assert_eq!(content(body("document-styles", "office", OFFICE)), found(0));
        let styles = body("document-content", "office", OFFICE);
        assert_eq!(
            odf_package_comments(zipped(&[text.clone(), ("styles.xml", styles)])),
            found(0)
        );
        // A spreadsheet's cell notes, or a package with no mimetype, are
        // not an ODT's comments.
        let full = body("document-content", "office", OFFICE);
        let sheet = (
            "mimetype",
            "application/vnd.oasis.opendocument.spreadsheet".to_owned(),
        );
        assert_eq!(
            odf_package_comments(zipped(&[sheet, ("content.xml", full.clone())])),
            found(0)
        );
        assert_eq!(
            odf_package_comments(zipped(&[("content.xml", full)])),
            found(0)
        );
        assert_eq!(
            found(2).and(Comments {
                count: 1,
                at_least: true
            }),
            Comments {
                count: 3,
                at_least: true
            }
        );
        assert_eq!(
            odf_package_comments(Cursor::new(b"not a zip".to_vec())),
            found(0)
        );
        assert_eq!(odt_comments(Path::new("missing.odt")), found(0));
    }

    fn rels(items: &[(&str, &str)]) -> String {
        let items: String = items
            .iter()
            .map(|(kind, target)| {
                format!(r#"<Relationship Id="r{kind}" Type="{R}/{kind}" Target="{target}"/>"#)
            })
            .collect();
        format!(
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{items}</Relationships>"#
        )
    }

    #[test]
    fn finds_the_comments_part_through_the_main_documents_relationships() {
        let comments = |n: usize| {
            format!(
                r#"<w:comments xmlns:w="{W}">{}</w:comments>"#,
                r#"<w:comment w:id="0"/>"#.repeat(n)
            )
        };
        let document = format!(r#"<w:document xmlns:w="{W}"/>"#);
        // The target's case differs from the entry's; a glossary's comments
        // and an unreferenced part are not the document's.
        assert_eq!(
            package(&[
                ("_rels/.rels", rels(&[("officeDocument", "/Word/Main.xml")])),
                ("word/main.xml", document.clone()),
                (
                    "word/_rels/main.xml.rels",
                    rels(&[("comments", "Notes/C.XML")])
                ),
                ("word/notes/c.xml", comments(2)),
                ("word/glossary/comments.xml", comments(5)),
                ("word/comments.xml", comments(7)),
            ]),
            found(2)
        );
        // `..` targets resolve against the main part's folder.
        assert_eq!(
            package(&[
                (
                    "_rels/.rels",
                    rels(&[("officeDocument", "word/document.xml")])
                ),
                ("word/document.xml", document),
                (
                    "word/_rels/document.xml.rels",
                    rels(&[("comments", "../extra/c.xml")])
                ),
                ("extra/c.xml", comments(1)),
            ]),
            found(1)
        );
        assert_eq!(package(&[("word/comments.xml", comments(3))]), found(0));
        assert_eq!(
            package_comments(Cursor::new(b"not a zip".to_vec())),
            found(0)
        );
    }

    #[test]
    fn counts_the_public_comments_fixture() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/docx-preview/comments.docx");
        assert_eq!(docx_comments(&fixture), found(6));
        assert_eq!(docx_comments(Path::new("missing.docx")), found(0));
    }

    #[test]
    fn note_names_the_count() {
        assert_eq!(found(0).note(), None);
        assert_eq!(found(1).note().unwrap(), "1 comment not imported.");
        assert_eq!(found(12).note().unwrap(), "12 comments not imported.");
        let at_least = Comments {
            count: 1,
            at_least: true,
        };
        assert_eq!(at_least.note().unwrap(), "At least 1 comment not imported.");
        assert_eq!(found(3).not_shown().unwrap(), "3 comments not shown.");
        assert_eq!(found(0).not_shown(), None);
    }
}
