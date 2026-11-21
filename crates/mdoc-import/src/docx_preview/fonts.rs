//! The fallback fonts the app ships for documents whose fonts are missing:
//! Carlito for Calibri, Caladea for Cambria, and Liberation Sans, Serif and
//! Mono, metric-compatible with Arial, Times New Roman and Courier New, all
//! under the SIL Open Font License, which sits beside them in `resources/fonts`.
//!
//! The iOS app also ships a subset of Noto Sans CJK SC, in `resources/fonts-ios`
//! (#1036): iOS has no Chinese, Japanese or Korean face docxide can read, so
//! without it every such character drew as an empty box. Desktop finds the
//! system's CJK faces, so its packages, which carry `resources/fonts`, leave
//! that folder out.
//!
//! docxide looks a run's own font up first and reaches Carlito and Caladea
//! only when Calibri or Cambria is not installed, so installed fonts always
//! win. The directories are a conversion option (`DOCXIDE_FONTS` is the same
//! setting read from the environment, which the worker leaves alone): docxide
//! searches them before the system directories, which matters only for a
//! family of the same name, such as a Carlito the user installed.
use std::path::{Path, PathBuf};

/// A file every copy of the fallback folder has.
const SENTINEL: &str = "Carlito-Regular.ttf";
/// The iOS app's CJK face, the one file of its folder besides the licence.
const CJK_FACE: &str = "NotoSansCJKsc-Regular-Subset.otf";

pub(super) fn options() -> docxide_pdf::ConvertOptions {
    with_fonts(cfg!(target_os = "ios"))
}

/// The options with the bundled fonts, and `ios` for the iOS app's: its CJK
/// face, and no font index written, since iOS has no system font directories
/// docxide reads and the bundle's are all it scans, once per process.
fn with_fonts(ios: bool) -> docxide_pdf::ConvertOptions {
    let options = docxide_pdf::ConvertOptions::new().font_cache(!ios);
    let dirs: Vec<PathBuf> = bundled(SENTINEL, "fonts")
        .into_iter()
        .chain(ios.then(|| bundled(CJK_FACE, "fonts-ios")).flatten())
        .collect();
    if dirs.is_empty() {
        options
    } else {
        options.font_dirs(dirs)
    }
}

/// The bundled `folder` holding `sentinel`: beside the executable on Windows
/// and in the iOS app bundle, in the bundle's Resources on macOS, and in the
/// source tree's `resources` for a development build.
fn bundled(sentinel: &str, folder: &str) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;
    let source = cfg!(debug_assertions).then(|| {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../resources")
            .join(folder)
    });
    [
        exe_dir.join(folder),
        exe_dir.join("../Resources").join(folder),
    ]
    .into_iter()
    .chain(source)
    .find(|dir| dir.join(sentinel).is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every family the decision names, in its four styles, with a licence.
    #[test]
    fn the_bundle_has_every_face_and_its_licences() {
        let dir = bundled(SENTINEL, "fonts").expect("a development build finds resources/fonts");
        for family in [
            "Carlito",
            "Caladea",
            "LiberationSans",
            "LiberationSerif",
            "LiberationMono",
        ] {
            for style in ["Regular", "Bold", "Italic", "BoldItalic"] {
                let face = dir.join(format!("{family}-{style}.ttf"));
                assert!(face.is_file(), "{} is missing", face.display());
            }
        }
        for licence in [
            "Carlito-OFL.txt",
            "Caladea-OFL.txt",
            "Liberation-LICENSE.txt",
        ] {
            assert!(dir.join(licence).is_file(), "{licence} is missing");
        }
    }

    // Contract-style lines with CJK punctuation and full-width forms: 镕 is
    // one of the subset's extras beyond GB2312, 締 a JIS X 0208 kanji, ㈜
    // and ㎡ enclosed and compatibility CJK.
    const CHINESE: &str = "甲方与乙方签订本合同（第一条），双方应遵守法律。朱镕基";
    const JAPANESE: &str = "本契約は、甲と乙の間で締結される。ひらがな・カタカナ";
    const KOREAN: &str = "본 계약은 갑과 을 사이에 체결된다. ㈜한국 100㎡";

    /// The iOS face is in the iOS set, licensed, and not in the desktop one.
    #[test]
    fn only_the_ios_set_has_the_cjk_face() {
        let dirs = |ios| with_fonts(ios).font_dirs;
        let cjk =
            bundled(CJK_FACE, "fonts-ios").expect("a development build finds resources/fonts-ios");
        assert!(cjk.join("NotoSansCJK-OFL.txt").is_file());
        assert!(dirs(true).contains(&cjk));
        assert!(!dirs(false).contains(&cjk));
    }

    /// Each script's runs, named by the face's three family names, draw from
    /// it: the Japanese and Korean names are the subset's own aliases, which
    /// docxide's Japanese and Korean fallback lists look up.
    #[test]
    fn the_ios_cjk_face_draws_chinese_japanese_and_korean() {
        let pdf = render_ios(&[
            (Some("Noto Sans CJK SC"), CHINESE),
            (Some("Noto Sans CJK JP"), JAPANESE),
            (Some("Noto Sans CJK KR"), KOREAN),
        ]);
        let drawn = drawn(&pdf);
        for (face, text) in [
            ("NotoSansCJKSC", CHINESE),
            ("NotoSansCJKJP", JAPANESE),
            ("NotoSansCJKKR", KOREAN),
        ] {
            for c in text.chars().filter(|c| !c.is_whitespace()) {
                assert!(
                    drawn.iter().any(|g| g.char == c && g.font == face && g.cff),
                    "{c} is not drawn from {face}'s CFF outlines: {:?}",
                    drawn.iter().filter(|g| g.char == c).collect::<Vec<_>>()
                );
            }
        }
    }

    /// Runs in Word's own Chinese, Japanese and Korean fonts, and runs in the
    /// default font, reach the face through docxide's fallback. Where the
    /// system has a face docxide's lists put first (Hiragino on macOS, SimSun
    /// on Windows) that one draws them, so the face is checked elsewhere.
    #[test]
    fn word_cjk_fonts_fall_back_to_the_ios_face() {
        let system_cjk_first = cfg!(any(target_os = "macos", target_os = "windows"));
        for (font, text) in [
            ("SimSun", CHINESE),
            ("MS Mincho", JAPANESE),
            ("Malgun Gothic", KOREAN),
        ] {
            let drawn = drawn(&render_ios(&[(Some(font), text), (None, text)]));
            for c in text.chars().filter(|c| !c.is_whitespace()) {
                // Both paragraphs draw every occurrence, none as .notdef (`drawn`).
                let glyphs: Vec<_> = drawn.iter().filter(|g| g.char == c).collect();
                assert_eq!(
                    glyphs.len(),
                    2 * text.matches(c).count(),
                    "{font}: {c} is missing"
                );
                if !system_cjk_first {
                    assert!(
                        glyphs.iter().all(|g| g.font.starts_with("NotoSansCJK")),
                        "{font}: {c} is not drawn from the iOS face: {glyphs:?}"
                    );
                }
            }
        }
    }

    /// A DOCX of one paragraph per (font, text), rendered with the iOS font
    /// set. `None` leaves the run in the document's default font.
    fn render_ios(paragraphs: &[(Option<&str>, &str)]) -> Vec<u8> {
        const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
        const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
        let body: String = paragraphs
            .iter()
            .map(|(font, text)| {
                let fonts = font.map_or_else(String::new, |f| {
                    format!(r#"<w:rPr><w:rFonts w:ascii="{f}" w:hAnsi="{f}" w:eastAsia="{f}" w:cs="{f}"/></w:rPr>"#)
                });
                format!("<w:p><w:r>{fonts}<w:t>{text}</w:t></w:r></w:p>")
            })
            .collect();
        let document = format!(
            r#"<w:document xmlns:w="{W}"><w:body>{body}<w:sectPr><w:pgSz w:w="11906" w:h="16838"/></w:sectPr></w:body></w:document>"#
        );
        let types = r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
        let rels = format!(
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{R}/officeDocument" Target="word/document.xml"/></Relationships>"#
        );
        let docx = super::super::tests::package(&[
            ("[Content_Types].xml", types.as_bytes()),
            ("_rels/.rels", rels.as_bytes()),
            ("word/document.xml", document.as_bytes()),
        ]);
        docxide_pdf::convert_docx_bytes_to_pdf_bytes_with(&docx, &with_fonts(true)).unwrap()
    }

    /// A character the PDF draws: the font's base name, and whether its
    /// embedded program is CFF.
    #[derive(Debug)]
    struct Glyph {
        char: char,
        font: String,
        cff: bool,
    }

    /// Every character drawn on the PDF's pages, read back through each
    /// font's ToUnicode map. A code that maps to no glyph outline in the
    /// embedded program, .notdef among them, fails the test.
    fn drawn(pdf: &[u8]) -> Vec<Glyph> {
        use lopdf::{Document, Object, content::Content};
        let doc = Document::load_from(pdf).unwrap();
        let deref = |object: &'_ Object| -> Object { doc.dereference(object).unwrap().1.clone() };
        let mut glyphs = Vec::new();
        for page in doc.get_pages().into_values() {
            let fonts: std::collections::HashMap<Vec<u8>, EmbeddedFont> = doc
                .get_page_fonts(page)
                .unwrap()
                .into_iter()
                .map(|(name, dict)| (name, EmbeddedFont::read(dict, &deref)))
                .collect();
            let content = Content::decode(&doc.get_page_content(page)).unwrap();
            let mut font = None;
            for op in content.operations {
                let strings: Vec<&[u8]> = match op.operator.as_str() {
                    "Tf" => {
                        font = fonts.get(op.operands[0].as_name().unwrap());
                        continue;
                    }
                    "Tj" | "'" | "\"" => vec![op.operands.last().unwrap().as_str().unwrap()],
                    "TJ" => op.operands[0]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter_map(|o| o.as_str().ok())
                        .collect(),
                    _ => continue,
                };
                let font = font.expect("text is drawn after Tf");
                for code in strings.concat().chunks(2) {
                    let code = u16::from_be_bytes([code[0], code[1]]);
                    let text = font.to_unicode.get(&code).cloned().unwrap_or_default();
                    for char in text.chars() {
                        if !char.is_whitespace() {
                            assert!(
                                font.has_outline(code),
                                "{} draws {char} as .notdef",
                                font.name
                            );
                        }
                        glyphs.push(Glyph {
                            char,
                            font: font.name.clone(),
                            cff: font.cff(),
                        });
                    }
                }
            }
        }
        glyphs
    }

    /// The parts of a Type0 font the test reads.
    struct EmbeddedFont {
        name: String,
        to_unicode: std::collections::HashMap<u16, String>,
        cid_to_gid: Option<Vec<u16>>,
        program: Vec<u8>,
    }

    impl EmbeddedFont {
        fn read(dict: &lopdf::Dictionary, deref: &dyn Fn(&lopdf::Object) -> lopdf::Object) -> Self {
            let get = |dict: &lopdf::Dictionary, key: &[u8]| dict.get(key).ok().map(deref);
            let stream = |object: Option<lopdf::Object>| {
                object.and_then(|o| {
                    o.as_stream()
                        .ok()
                        .map(|s| s.decompressed_content().unwrap())
                })
            };
            let name =
                String::from_utf8(get(dict, b"BaseFont").unwrap().as_name().unwrap().to_vec())
                    .unwrap();
            let descendant = get(dict, b"DescendantFonts")
                .and_then(|a| a.as_array().ok().and_then(|a| a.first().map(deref)))
                .and_then(|d| d.as_dict().ok().cloned());
            let (cid_to_gid, program) = descendant.map_or((None, Vec::new()), |cid| {
                let map = stream(get(&cid, b"CIDToGIDMap")).map(|bytes| {
                    bytes
                        .chunks(2)
                        .map(|b| u16::from_be_bytes([b[0], b[1]]))
                        .collect()
                });
                let descriptor = get(&cid, b"FontDescriptor").unwrap();
                let descriptor = descriptor.as_dict().unwrap();
                let program = [&b"FontFile2"[..], b"FontFile3", b"FontFile"]
                    .into_iter()
                    .find_map(|key| stream(get(descriptor, key)))
                    .unwrap_or_default();
                (map, program)
            });
            let to_unicode = stream(get(dict, b"ToUnicode"))
                .map(|cmap| parse_to_unicode(&String::from_utf8_lossy(&cmap)))
                .unwrap_or_default();
            Self {
                name,
                to_unicode,
                cid_to_gid,
                program,
            }
        }

        fn cff(&self) -> bool {
            ttf_parser::Face::parse(&self.program, 0).is_ok_and(|face| face.tables().cff.is_some())
        }

        fn has_outline(&self, code: u16) -> bool {
            struct Ink(bool);
            impl ttf_parser::OutlineBuilder for Ink {
                fn move_to(&mut self, _: f32, _: f32) {}
                fn line_to(&mut self, _: f32, _: f32) {
                    self.0 = true;
                }
                fn quad_to(&mut self, _: f32, _: f32, _: f32, _: f32) {
                    self.0 = true;
                }
                fn curve_to(&mut self, _: f32, _: f32, _: f32, _: f32, _: f32, _: f32) {
                    self.0 = true;
                }
                fn close(&mut self) {}
            }
            let gid = match &self.cid_to_gid {
                Some(map) => map.get(usize::from(code)).copied().unwrap_or(0),
                None => code,
            };
            let Ok(face) = ttf_parser::Face::parse(&self.program, 0) else {
                return false;
            };
            let mut ink = Ink(false);
            gid != 0
                && face
                    .outline_glyph(ttf_parser::GlyphId(gid), &mut ink)
                    .is_some()
                && ink.0
        }
    }

    /// A ToUnicode CMap's `bfchar` and `bfrange` entries, code to text.
    fn parse_to_unicode(cmap: &str) -> std::collections::HashMap<u16, String> {
        let hex = |token: &str| -> Vec<u16> {
            let digits = token.trim_matches(['<', '>']);
            (0..digits.len())
                .step_by(4)
                .map(|i| u16::from_str_radix(&digits[i..i + 4], 16).unwrap())
                .collect()
        };
        let mut map = std::collections::HashMap::new();
        let mut section = "";
        let mut pending = Vec::new();
        for token in cmap.split_whitespace() {
            match token {
                "beginbfchar" | "beginbfrange" => section = token,
                "endbfchar" | "endbfrange" => section = "",
                _ if section.is_empty() || !token.starts_with('<') => {}
                _ => {
                    pending.push(hex(token));
                    match (section, pending.as_slice()) {
                        ("beginbfchar", [code, text]) => {
                            map.insert(code[0], String::from_utf16(text).unwrap());
                        }
                        ("beginbfrange", [low, high, first]) => {
                            for (i, code) in (low[0]..=high[0]).enumerate() {
                                let mut text = first.clone();
                                *text.last_mut().unwrap() += i as u16;
                                map.insert(code, String::from_utf16(&text).unwrap());
                            }
                        }
                        _ => continue,
                    }
                    pending.clear();
                }
            }
        }
        map
    }
}
