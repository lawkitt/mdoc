//! Hidden Office content (recognition grill 2026-11-01, round 3): what a Word
//! document holds that its Markdown must not carry. The pinned AnyDoc leaves
//! out comments, deleted and moved-from tracked changes and document
//! properties, with no option to keep them; `import`'s tests pin that on
//! `handmade-hidden-content.docx`. This module counts the comments left
//! out, for the import note.
use std::{
    io::{Read, Seek},
    path::Path,
};

/// The most of each XML part read to find its root element: Word's root tags
/// carry a few kilobytes of namespace declarations.
const HEAD: u64 = 64 * 1024;
/// The most of a comments part read for counting; past it the count is a
/// lower bound.
const PART_LIMIT: u64 = 64 * 1024 * 1024;

/// The comments in a Word package's comments part, or 0 when `path` is not
/// a Word package or cannot be read. Best effort: the conversion has already
/// succeeded, so a part it cannot read is skipped rather than failing it.
pub fn docx_comment_count(path: &Path) -> usize {
    std::fs::File::open(path)
        .ok()
        .map_or(0, package_comment_count)
}

fn package_comment_count(reader: impl Read + Seek) -> usize {
    let Ok(mut archive) = zip::ZipArchive::new(reader) else {
        return 0;
    };
    let mut count = 0;
    for index in 0..archive.len() {
        let Ok(mut entry) = archive.by_index(index) else {
            continue;
        };
        if !entry.name().is_ok_and(|name| name.ends_with(".xml")) {
            continue;
        }
        let mut bytes = Vec::new();
        if (&mut entry).take(HEAD).read_to_end(&mut bytes).is_err()
            || count_comments(&decode(&bytes)).is_none()
            || (&mut entry)
                .take(PART_LIMIT - HEAD)
                .read_to_end(&mut bytes)
                .is_err()
        {
            continue;
        }
        count += count_comments(&decode(&bytes)).unwrap_or(0);
    }
    count
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

/// The `comment` elements in `xml` when its root is WordprocessingML's
/// `comments` (Transitional or Strict); `None` for any other part, such as a
/// spreadsheet's comments or the document itself.
fn count_comments(xml: &str) -> Option<usize> {
    let mut tags = start_tags(xml);
    let root = tags.next()?;
    (local_name(root) == "comments" && root.contains("wordprocessingml"))
        .then(|| tags.filter(|tag| local_name(tag) == "comment").count())
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

/// The import note's line for comments left out, if any.
pub(crate) fn comments_note(count: usize) -> Option<String> {
    match count {
        0 => None,
        1 => Some("1 comment not imported.".into()),
        n => Some(format!("{n} comments not imported.")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};

    const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

    #[test]
    fn counts_word_comments_only() {
        let part = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<!-- <w:comment w:id="9"> in an XML comment is not one -->
<w:comments xmlns:w="{W}"><w:comment w:id="0"><w:p><w:r><w:t>a</w:t></w:r></w:p></w:comment>
<w:comment w:id="1"/><x:comment xmlns:x="{W}" w:id="2"></x:comment><![CDATA[<w:comment>]]></w:comments>"#
        );
        assert_eq!(count_comments(&part), Some(3));
        let strict = r#"<comments xmlns="http://purl.oclc.org/ooxml/wordprocessingml/main"><comment id="0"/></comments>"#;
        assert_eq!(count_comments(strict), Some(1));
        // A spreadsheet's notes, the document and an empty part are not
        // Word comments parts.
        let sheet = r#"<comments xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><commentList><comment ref="A1"/></commentList></comments>"#;
        assert_eq!(count_comments(sheet), None);
        assert_eq!(
            count_comments(&format!(r#"<w:document xmlns:w="{W}"/>"#)),
            None
        );
        assert_eq!(count_comments(""), None);
        // An extended comments part names no `comment` elements of its own.
        assert_eq!(
            count_comments(r#"<w15:commentsEx xmlns:w15="x"><w15:commentEx/></w15:commentsEx>"#),
            None
        );
    }

    #[test]
    fn reads_utf16_parts_and_packages() {
        let part = format!(r#"<w:comments xmlns:w="{W}"><w:comment w:id="0"/></w:comments>"#);
        let mut le = vec![0xFF, 0xFE];
        le.extend(part.encode_utf16().flat_map(u16::to_le_bytes));
        assert_eq!(count_comments(&decode(&le)), Some(1));
        let mut be = vec![0xFE, 0xFF];
        be.extend(part.encode_utf16().flat_map(u16::to_be_bytes));
        assert_eq!(count_comments(&decode(&be)), Some(1));

        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, data) in [
            (
                "word/document.xml",
                format!(r#"<w:document xmlns:w="{W}"/>"#),
            ),
            ("word/comments.xml", part.clone()),
            ("word/comments.rels", part),
        ] {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(data.as_bytes()).unwrap();
        }
        let bytes = zip.finish().unwrap().into_inner();
        assert_eq!(package_comment_count(Cursor::new(bytes)), 1);
        assert_eq!(package_comment_count(Cursor::new(b"not a zip".to_vec())), 0);
    }

    #[test]
    fn counts_the_public_comments_fixture() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/docx-preview/comments.docx");
        assert_eq!(docx_comment_count(&fixture), 4);
        assert_eq!(docx_comment_count(Path::new("missing.docx")), 0);
    }

    #[test]
    fn note_names_the_count() {
        assert_eq!(comments_note(0), None);
        assert_eq!(comments_note(1).unwrap(), "1 comment not imported.");
        assert_eq!(comments_note(12).unwrap(), "12 comments not imported.");
    }
}
