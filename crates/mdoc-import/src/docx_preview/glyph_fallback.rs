//! Currency signs the substituted fonts lack get a system face that has them.
//!
//! A Word document usually asks for Calibri or Cambria. Where neither is
//! installed the renderer draws the text in Arial, whose macOS copy (like Times
//! New Roman and Courier New there) stops at U+20B5, so ₽ came out as an empty
//! box (#305). The renderer rescues only East Asian characters per glyph, so the
//! worker's render copy moves the newer Currency Symbols into runs of their own
//! in a system face that covers them. Windows' copies of those fonts already
//! carry the block, and the source document is never changed.
use super::{PreviewError, replace_parts, word};
use roxmltree::Node;
use std::{
    collections::HashMap,
    io::{Cursor, Read},
};
use zip::ZipArchive;

/// The face that draws a character, `None` to leave it in its run's font.
pub(super) type Fallback = fn(char) -> Option<&'static str>;

/// This platform's faces for the signs, or `None` where the fonts the renderer
/// substitutes already have them.
pub(super) fn platform_fallback() -> Option<Fallback> {
    cfg!(target_os = "macos").then_some(mac_fallback as Fallback)
}

/// Faces every macOS install has, with all four styles under one family name.
/// PT Sans draws ₽ beside Arial; Rockwell is the one such face with ₸ ₹ ₺ ₼ ₾
/// (Helvetica Neue has them, but the renderer resolves it to its Thin weight).
fn mac_fallback(c: char) -> Option<&'static str> {
    match c {
        '\u{20BD}' => Some("PT Sans"),
        '\u{20B6}'..='\u{20CF}' => Some("Rockwell"),
        _ => None,
    }
}

/// A render copy with the signs `fallback` names moved into its faces, or
/// `None` when the document has none.
pub(super) fn prepare(bytes: &[u8], fallback: Fallback) -> Result<Option<Vec<u8>>, PreviewError> {
    let invalid = |e: zip::result::ZipError| PreviewError::Package(e.to_string());
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(invalid)?;
    let mut replacements = HashMap::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(invalid)?;
        let name = entry.name().map_err(invalid)?;
        if !(name.starts_with("word/") && name.ends_with(".xml")) {
            continue;
        }
        let mut raw = Vec::new();
        entry.read_to_end(&mut raw)?;
        // Word writes its parts in UTF-8; another encoding keeps its runs.
        let Ok(source) = std::str::from_utf8(&raw) else {
            continue;
        };
        if let Some(xml) = rewrite_part(source, fallback)? {
            replacements.insert(i, xml);
        }
    }
    if replacements.is_empty() {
        return Ok(None);
    }
    replace_parts(&mut archive, &replacements).map(Some)
}

fn rewrite_part(source: &str, fallback: Fallback) -> Result<Option<String>, PreviewError> {
    let doc =
        roxmltree::Document::parse(source).map_err(|e| PreviewError::Package(e.to_string()))?;
    let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    for run in doc.descendants().filter(|n| word(*n, "r")) {
        // A text box's runs sit inside a run; the outer rewrite copies them as they are.
        if edits
            .last()
            .is_some_and(|(r, _)| r.contains(&run.range().start))
        {
            continue;
        }
        if let Some(xml) = split_run(source, run, fallback) {
            edits.push((run.range(), xml));
        }
    }
    if edits.is_empty() {
        return Ok(None);
    }
    let mut out = source.to_owned();
    for (range, xml) in edits.into_iter().rev() {
        out.replace_range(range, &xml);
    }
    Ok(Some(out))
}

/// The text of a `w:t` with a sign `fallback` names.
fn rescued_text<'a>(child: Node<'a, '_>, fallback: Fallback) -> Option<&'a str> {
    child
        .text()
        .filter(|t| word(child, "t") && t.chars().any(|c| fallback(c).is_some()))
}

/// The run as a sequence of runs with the same properties, each stretch of
/// signs in its own run whose fonts are their face. `None` when the run has no
/// such sign, or no namespace prefix (the renderer would not read its attributes).
fn split_run(source: &str, run: Node<'_, '_>, fallback: Fallback) -> Option<String> {
    if !run.children().any(|c| rescued_text(c, fallback).is_some()) {
        return None;
    }
    let tag = &source[run.range().start + 1..];
    let qname = &tag[..tag.find(|c: char| c.is_whitespace() || c == '/' || c == '>')?];
    let prefix = &qname[..qname.find(':')? + 1];
    // The start tag keeps the run's own attributes (revision IDs).
    let open = &source[run.range().start..run.first_child()?.range().start];
    let close = format!("</{prefix}r>");
    let rpr = run.children().find(|c| word(*c, "rPr"));
    let plain_rpr = rpr.map_or("", |n| &source[n.range()]);

    let mut out = String::new();
    let mut plain = String::new();
    let mut plain_has_content = false;
    let flush = |out: &mut String, plain: &mut String, has_content: &mut bool| {
        if *has_content {
            out.push_str(open);
            out.push_str(plain_rpr);
            out.push_str(plain);
            out.push_str(&close);
        }
        plain.clear();
        *has_content = false;
    };
    for child in run.children().filter(|c| !word(*c, "rPr")) {
        let Some(text) = rescued_text(child, fallback) else {
            plain.push_str(&source[child.range()]);
            plain_has_content |= child.is_element();
            continue;
        };
        for (font, segment) in segments(text, fallback) {
            let t = format!(
                "<{prefix}t xml:space=\"preserve\">{}</{prefix}t>",
                escape(segment)
            );
            if let Some(font) = font {
                flush(&mut out, &mut plain, &mut plain_has_content);
                out.push_str(open);
                out.push_str(&rescue_properties(source, rpr, prefix, font));
                out.push_str(&t);
                out.push_str(&close);
            } else {
                plain.push_str(&t);
                plain_has_content = true;
            }
        }
    }
    flush(&mut out, &mut plain, &mut plain_has_content);
    Some(out)
}

/// The run properties with every font slot set to `font` and theme fonts
/// dropped (they outrank a named font), the style kept first as the schema orders it.
fn rescue_properties(source: &str, rpr: Option<Node<'_, '_>>, prefix: &str, font: &str) -> String {
    let font = escape(font).replace('"', "&quot;");
    let fonts = format!(
        "<{prefix}rFonts {prefix}ascii=\"{font}\" {prefix}hAnsi=\"{font}\" {prefix}eastAsia=\"{font}\" {prefix}cs=\"{font}\"/>"
    );
    let mut style = String::new();
    let mut rest = String::new();
    for child in rpr.into_iter().flat_map(|n| n.children()) {
        if word(child, "rStyle") {
            style.push_str(&source[child.range()]);
        } else if !word(child, "rFonts") {
            rest.push_str(&source[child.range()]);
        }
    }
    format!("<{prefix}rPr>{style}{fonts}{rest}</{prefix}rPr>")
}

/// Maximal stretches of `text` drawn in one face (`None`: the run's own).
fn segments(text: &str, fallback: Fallback) -> Vec<(Option<&'static str>, &str)> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut current = None;
    for (i, c) in text.char_indices() {
        let font = fallback(c);
        if let Some(cur) = current
            && cur != font
        {
            out.push((cur, &text[start..i]));
            start = i;
        }
        current = Some(font);
    }
    if let Some(font) = current {
        out.push((font, &text[start..]));
    }
    out
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

    fn body(runs: &str) -> String {
        format!(r#"<w:document xmlns:w="{W}"><w:body><w:p>{runs}</w:p></w:body></w:document>"#)
    }

    /// (text, ascii font, bold) of every run, in order.
    fn runs(xml: &str) -> Vec<(String, Option<String>, bool)> {
        let doc = roxmltree::Document::parse(xml).unwrap();
        doc.descendants()
            .filter(|n| word(*n, "r"))
            .map(|r| {
                let text = r
                    .descendants()
                    .filter(|n| word(*n, "t"))
                    .filter_map(|n| n.text())
                    .collect();
                let fonts = r.descendants().find(|n| word(*n, "rFonts"));
                let bold = r.descendants().any(|n| word(n, "b"));
                (
                    text,
                    fonts.and_then(|f| f.attribute((W, "ascii")).map(str::to_owned)),
                    bold,
                )
            })
            .collect()
    }

    fn face(name: &str) -> Option<String> {
        Some(name.to_owned())
    }

    #[test]
    fn signs_get_runs_of_their_own_in_their_faces() {
        let xml = body(
            r#"<w:r w:rsidR="00A1"><w:rPr><w:rStyle w:val="Strong"/><w:rFonts w:asciiTheme="minorHAnsi" w:hAnsiTheme="minorHAnsi"/><w:b/></w:rPr><w:t xml:space="preserve">в размере 1 250 000 ₽ в течение &amp; &lt;14&gt;</w:t><w:tab/><w:t>₸₼₽ end</w:t></w:r>"#,
        );
        let out = rewrite_part(&xml, mac_fallback).unwrap().unwrap();
        assert_eq!(
            runs(&out),
            [
                ("в размере 1 250 000 ".to_owned(), None, true),
                ("₽".to_owned(), face("PT Sans"), true),
                (" в течение & <14>".to_owned(), None, true),
                ("₸₼".to_owned(), face("Rockwell"), true),
                ("₽".to_owned(), face("PT Sans"), true),
                (" end".to_owned(), None, true),
            ]
        );
        let doc = roxmltree::Document::parse(&out).unwrap();
        // The theme fonts that outrank a named font are gone from the rescued runs,
        // the style stays first, and the tab stays between its texts.
        let rescued = doc
            .descendants()
            .filter(|n| word(*n, "rFonts") && n.attribute((W, "ascii")).is_some())
            .collect::<Vec<_>>();
        assert_eq!(rescued.len(), 3);
        for fonts in rescued {
            assert!(fonts.attribute((W, "asciiTheme")).is_none());
            assert!(word(fonts.prev_sibling_element().unwrap(), "rStyle"));
            assert_eq!(fonts.attribute((W, "hAnsi")), fonts.attribute((W, "ascii")));
        }
        assert_eq!(doc.descendants().filter(|n| word(*n, "tab")).count(), 1);
        assert!(
            doc.descendants()
                .filter(|n| word(*n, "r"))
                .all(|r| r.attribute((W, "rsidR")) == Some("00A1"))
        );
    }

    #[test]
    fn a_run_without_properties_gains_them_and_others_are_untouched() {
        let xml = body("<w:r><w:t>50 000 ₽</w:t></w:r><w:r><w:t>€ 5 and ₴ 6</w:t></w:r>");
        let out = rewrite_part(&xml, mac_fallback).unwrap().unwrap();
        assert_eq!(
            runs(&out),
            [
                ("50 000 ".to_owned(), None, false),
                ("₽".to_owned(), face("PT Sans"), false),
                ("€ 5 and ₴ 6".to_owned(), None, false),
            ]
        );
        assert!(out.ends_with("<w:r><w:t>€ 5 and ₴ 6</w:t></w:r></w:p></w:body></w:document>"));
        let euro = body("<w:r><w:t>€ 5</w:t></w:r>");
        assert!(rewrite_part(&euro, mac_fallback).unwrap().is_none());
    }

    #[test]
    fn the_render_copy_changes_only_parts_with_signs() {
        let document = body("<w:r><w:t>₽</w:t></w:r>");
        let footer = body("<w:r><w:t>plain</w:t></w:r>");
        let bytes = super::super::tests::package(&[
            ("word/document.xml", document.as_bytes()),
            ("word/footer1.xml", footer.as_bytes()),
        ]);
        let copy = prepare(&bytes, mac_fallback).unwrap().unwrap();
        let mut zip = ZipArchive::new(Cursor::new(copy)).unwrap();
        let mut read = |name| {
            let mut s = String::new();
            zip.by_name(name).unwrap().read_to_string(&mut s).unwrap();
            s
        };
        assert!(read("word/document.xml").contains(r#"w:ascii="PT Sans""#));
        assert_eq!(read("word/footer1.xml"), footer);
        let plain = super::super::tests::package(&[("word/document.xml", footer.as_bytes())]);
        assert!(prepare(&plain, mac_fallback).unwrap().is_none());
    }

    /// The faces resolve to their regular weight and draw the sign (#305).
    #[cfg(target_os = "macos")]
    #[test]
    fn the_russian_demo_renders_its_ruble_signs_in_pt_sans() {
        let source = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/product-media/Соглашение об урегулировании.docx");
        let preview = super::super::render_for_test(&source).unwrap();
        let pdf = std::fs::read(&preview.pdf_path).unwrap();
        let has = |needle: &[u8]| pdf.windows(needle.len()).any(|w| w == needle);
        // The font's ToUnicode map, outside the compressed object streams.
        assert!(has(b"/CMapName /PTSans-UTF16"));
    }
}
