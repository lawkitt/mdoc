//! Comment extraction runs in the conversion worker, before the renderer sees
//! a temporary copy with its unreliable comment-margin rendering disabled.
use super::{PreviewError, xml_text};
use roxmltree::Node;
use std::{
    collections::HashMap,
    io::{Cursor, Read, Write},
};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct DocxComment {
    pub id: String,
    pub author: String,
    pub date: String,
    pub text: String,
    pub quote: String,
}

fn word(node: Node<'_, '_>, name: &str) -> bool {
    node.tag_name().name() == name
        && matches!(
            node.tag_name().namespace(),
            Some(
                "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
                    | "http://purl.oclc.org/ooxml/wordprocessingml/main"
            )
        )
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

pub(super) fn prepare(bytes: &[u8]) -> Result<(Option<Vec<u8>>, Vec<DocxComment>), PreviewError> {
    let invalid = |e: zip::result::ZipError| PreviewError::Package(e.to_string());
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(invalid)?;
    let mut comments = Vec::new();
    let mut by_id = HashMap::new();
    let mut xml_parts = Vec::new();
    // Input has already passed the package expansion/entry limits.
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(invalid)?;
        if !entry.name().ends_with(".xml") {
            continue;
        }
        let mut raw = Vec::new();
        entry.read_to_end(&mut raw)?;
        let source = xml_text(&raw)?.into_owned();
        let doc = roxmltree::Document::parse(&source)
            .map_err(|e| PreviewError::Package(e.to_string()))?;
        if word(doc.root_element(), "comments") {
            for node in doc
                .root_element()
                .children()
                .filter(|n| word(*n, "comment"))
            {
                let id = attr(node, "id").to_owned();
                if id.is_empty() || by_id.insert(id.clone(), comments.len()).is_some() {
                    return Err(PreviewError::Package(
                        "missing or duplicate comment ID".into(),
                    ));
                }
                comments.push(DocxComment {
                    id,
                    author: attr(node, "author").to_owned(),
                    date: attr(node, "date").to_owned(),
                    text: text(node),
                    quote: String::new(),
                });
            }
        }
        xml_parts.push((i, source));
    }
    if comments.is_empty() {
        return Ok((None, comments));
    }
    let mut replacements = HashMap::new();
    for (index, source) in xml_parts {
        let doc = roxmltree::Document::parse(&source)
            .map_err(|e| PreviewError::Package(e.to_string()))?;
        if word(doc.root_element(), "comments") {
            replacements.insert(
                index,
                format!(
                    "<w:comments xmlns:w=\"{}\"/>",
                    doc.root_element().tag_name().namespace().unwrap()
                ),
            );
            continue;
        }
        let mut active = Vec::new();
        let mut removals = Vec::new();
        for node in doc.descendants() {
            if word(node, "commentRangeStart") {
                if let Some(&i) = by_id.get(attr(node, "id")) {
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
            if word(node, "commentRangeStart")
                || word(node, "commentRangeEnd")
                || word(node, "commentReference")
            {
                removals.push(node.range());
            }
        }
        // Point comments have no selected range: show their paragraph as context.
        for node in doc.descendants().filter(|n| word(*n, "commentReference")) {
            if let Some(&i) = by_id.get(attr(node, "id"))
                && comments[i].quote.is_empty()
                && let Some(paragraph) = node.ancestors().find(|n| word(*n, "p"))
            {
                comments[i].quote = text(paragraph);
            }
        }
        if !removals.is_empty() {
            let mut modified = source.clone();
            for range in removals.into_iter().rev() {
                modified.replace_range(range, "");
            }
            replacements.insert(index, modified);
        }
    }
    let mut output = ZipWriter::new(Cursor::new(Vec::new()));
    for i in 0..archive.len() {
        let entry = archive.by_index(i).map_err(invalid)?;
        if let Some(xml) = replacements.get(&i) {
            output
                .start_file(
                    entry.name(),
                    SimpleFileOptions::default()
                        .compression_method(zip::CompressionMethod::Deflated),
                )
                .map_err(invalid)?;
            output.write_all(xml.as_bytes())?;
        } else {
            output.raw_copy_file(entry).map_err(invalid)?;
        }
    }
    Ok((
        Some(output.finish().map_err(invalid)?.into_inner()),
        comments,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rendering_copy_removes_comment_markup_without_changing_source_text() {
        let bytes = include_bytes!("../tests/fixtures/docx-preview/comments.docx");
        let (copy, comments) = prepare(bytes).unwrap();
        assert_eq!(comments.len(), 4);
        let mut zip = ZipArchive::new(Cursor::new(copy.unwrap())).unwrap();
        let mut body = String::new();
        zip.by_name("word/document.xml")
            .unwrap()
            .read_to_string(&mut body)
            .unwrap();
        let doc = roxmltree::Document::parse(&body).unwrap();
        assert!(!doc.descendants().any(|n| matches!(
            n.tag_name().name(),
            "commentRangeStart" | "commentRangeEnd" | "commentReference"
        )));
        assert_eq!(
            text(doc.root_element()),
            "First quoted paragraph.\nSecond quoted paragraph.\nPoint comment context."
        );
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
        let (copy, comments) = prepare(include_bytes!(
            "../tests/fixtures/docx-preview/coverage.docx"
        ))
        .unwrap();
        assert!(copy.is_none());
        assert!(comments.is_empty());
    }
}
