//! The fallback fonts the app ships for documents whose fonts are missing:
//! Carlito for Calibri, Caladea for Cambria, and Liberation Sans, Serif and
//! Mono, metric-compatible with Arial, Times New Roman and Courier New, all
//! under the SIL Open Font License, which sits beside them in `resources/fonts`.
//!
//! The iOS app also ships a subset of Noto Sans CJK SC, in `resources/fonts-ios`
//! (#1036): iOS has no Chinese, Japanese or Korean face docxide can read, so
//! without it every such character drew as an empty box. Desktop finds the
//! system's CJK faces, so its packages, which carry `resources/fonts`, leave
//! that folder out. A document that embeds a face under one of the subset's
//! family names keeps its own: docxide prefers an installed face to an
//! embedded one of the same name, so the subset would hide the characters the
//! document's fuller copy draws (#1039).
//!
//! docxide looks a run's own font up first and reaches Carlito and Caladea
//! only when Calibri or Cambria is not installed, so installed fonts always
//! win. The directories are a conversion option (`DOCXIDE_FONTS` is the same
//! setting read from the environment, which the worker leaves alone): docxide
//! searches them before the system directories, which matters only for a
//! family of the same name, such as a Carlito the user installed.
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

/// A file every copy of the fallback folder has.
const SENTINEL: &str = "Carlito-Regular.ttf";
/// The iOS app's CJK face, the one file of its folder besides the licence.
const CJK_FACE: &str = "NotoSansCJKsc-Regular-Subset.otf";
/// The family names the CJK face carries: its own, and the Japanese and Korean
/// ones `scripts/dev/subset-cjk-font.py` adds for docxide's fallback lists.
const CJK_FAMILIES: [&str; 3] = ["Noto Sans CJK SC", "Noto Sans CJK JP", "Noto Sans CJK KR"];

/// The options for rendering `docx`.
pub(super) fn options(docx: &[u8]) -> docxide_pdf::ConvertOptions {
    with_fonts(cfg!(target_os = "ios"), docx)
}

/// The options with the bundled fonts, and with `ios` the iOS app's: its CJK
/// face unless `docx` embeds one of that face's names, and no font index
/// written, since iOS has no system font directories docxide reads and the
/// bundle's are all it scans, once per process. Without the subset, the
/// embedded face draws its own runs, and docxide's CJK lists reach it by name
/// for the other runs' characters.
fn with_fonts(ios: bool, docx: &[u8]) -> docxide_pdf::ConvertOptions {
    let options = docxide_pdf::ConvertOptions::new().font_cache(!ios);
    let cjk = ios && !embeds_a_cjk_family(docx);
    let dirs: Vec<PathBuf> = bundled(SENTINEL, "fonts")
        .into_iter()
        .chain(cjk.then(|| bundled(CJK_FACE, "fonts-ios")).flatten())
        .collect();
    if dirs.is_empty() {
        options
    } else {
        options.font_dirs(dirs)
    }
}

/// Whether the document's font table, the part docxide takes embedded fonts
/// from, embeds a face under one of `CJK_FAMILIES`.
fn embeds_a_cjk_family(docx: &[u8]) -> bool {
    let mut xml = String::new();
    let read = zip::ZipArchive::new(Cursor::new(docx)).is_ok_and(|mut zip| {
        zip.by_name("word/fontTable.xml")
            .is_ok_and(|mut part| part.read_to_string(&mut xml).is_ok())
    });
    let Some(table) = read
        .then(|| roxmltree::Document::parse(&xml).ok())
        .flatten()
    else {
        return false;
    };
    let embeds = |font: &roxmltree::Node<'_, '_>| {
        font.children().any(|child| {
            [
                "embedRegular",
                "embedBold",
                "embedItalic",
                "embedBoldItalic",
            ]
            .iter()
            .any(|tag| super::word(child, tag))
        })
    };
    table
        .descendants()
        .filter(|node| super::word(*node, "font") && embeds(node))
        .filter_map(|font| font.attributes().find(|a| a.name() == "name"))
        .any(|name| {
            CJK_FAMILIES
                .iter()
                .any(|f| f.eq_ignore_ascii_case(name.value()))
        })
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

    /// The iOS face is in the iOS set, licensed, and not in the desktop one,
    /// nor in the iOS one for a document that embeds a face of its name. Its
    /// family names are the ones `CJK_FAMILIES` checks.
    #[test]
    fn only_the_ios_set_has_the_cjk_face() {
        let dirs = |ios, docx: &[u8]| with_fonts(ios, docx).font_dirs;
        let cjk =
            bundled(CJK_FACE, "fonts-ios").expect("a development build finds resources/fonts-ios");
        assert!(cjk.join("NotoSansCJK-OFL.txt").is_file());
        let plain = docx(&[(None, CHINESE)], false);
        assert!(dirs(true, &plain).contains(&cjk));
        assert!(!dirs(false, &plain).contains(&cjk));
        assert!(!dirs(true, &docx(&[(None, CHINESE)], true)).contains(&cjk));

        let data = std::fs::read(cjk.join(CJK_FACE)).unwrap();
        let face = ttf_parser::Face::parse(&data, 0).unwrap();
        let mut families: Vec<String> = face
            .names()
            .into_iter()
            .filter(|n| n.name_id == ttf_parser::name_id::FAMILY && n.is_unicode())
            .filter_map(|n| n.to_string())
            .collect();
        families.sort();
        let mut expected = CJK_FAMILIES.map(String::from);
        expected.sort();
        assert_eq!(families, expected);
    }

    /// Each script's runs, named by the face's three family names, draw from
    /// it: the Japanese and Korean names are the subset's own aliases, which
    /// docxide's Japanese and Korean fallback lists look up.
    #[test]
    fn the_ios_cjk_face_draws_chinese_japanese_and_korean() {
        ios_fonts_only("the_ios_cjk_face_draws_chinese_japanese_and_korean", || {
            let drawn = drawn(&render_ios(&docx(
                &[
                    (Some("Noto Sans CJK SC"), CHINESE),
                    (Some("Noto Sans CJK JP"), JAPANESE),
                    (Some("Noto Sans CJK KR"), KOREAN),
                ],
                false,
            )));
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
        });
    }

    /// Runs in Word's own Chinese, Japanese and Korean fonts, and runs in the
    /// default font, reach the face through docxide's fallback: every
    /// occurrence of every character, none as .notdef (`drawn`).
    #[test]
    fn word_cjk_fonts_fall_back_to_the_ios_face() {
        ios_fonts_only("word_cjk_fonts_fall_back_to_the_ios_face", || {
            for (font, text) in [
                ("SimSun", CHINESE),
                ("MS Mincho", JAPANESE),
                ("Malgun Gothic", KOREAN),
            ] {
                let drawn = drawn(&render_ios(&docx(
                    &[(Some(font), text), (None, text)],
                    false,
                )));
                for c in text.chars().filter(|c| !c.is_whitespace()) {
                    let glyphs: Vec<_> = drawn.iter().filter(|g| g.char == c).collect();
                    assert_eq!(
                        glyphs.len(),
                        2 * text.matches(c).count(),
                        "{font}: {c} is missing"
                    );
                    // Digits and Latin punctuation in the default font stay Latin.
                    if !c.is_ascii() {
                        assert!(
                            glyphs.iter().all(|g| g.font.starts_with("NotoSansCJK")),
                            "{font}: {c} is not drawn from the iOS face: {glyphs:?}"
                        );
                    }
                }
            }
        });
    }

    /// A document's embedded copy of a face the subset also names wins over
    /// the subset: 髙, which the subset lacks, draws from it (#1039). With the
    /// subset in the set, docxide took the installed subset and drew .notdef.
    #[test]
    fn an_embedded_cjk_face_wins_over_the_ios_one() {
        ios_fonts_only("an_embedded_cjk_face_wins_over_the_ios_one", || {
            let drawn = drawn(&render_ios(&docx(
                &[(Some("Noto Sans CJK SC"), EMBEDDED)],
                true,
            )));
            for c in EMBEDDED.chars() {
                assert!(
                    drawn
                        .iter()
                        .any(|g| g.char == c && g.font == "NotoSansCJKSC"),
                    "{c} is not drawn from the embedded face: {drawn:?}"
                );
            }
        });
    }

    /// The characters of the embedded test face,
    /// `tests/fixtures/docx-preview/NotoSansCJKsc-Embedded.otf`.
    const EMBEDDED: &str = "髙橋一郎";

    /// Set in the child process `ios_fonts_only` starts.
    const IOS_FONTS_ONLY: &str = "MDOC_TEST_IOS_FONTS_ONLY";

    /// Runs `body` where docxide sees no system font folder, as on iOS, so the
    /// bundled folders are the only fonts it has: elsewhere docxide always adds
    /// the system's, whose SimSun, Hiragino or Apple SD Gothic would draw the
    /// characters first. On macOS the test reruns itself, `test` by name, under
    /// a sandbox that denies the font folders docxide scans. Other platforms
    /// have no such sandbox at hand and skip it; the macOS job runs it.
    fn ios_fonts_only(test: &str, body: impl FnOnce()) {
        if std::env::var_os(IOS_FONTS_ONLY).is_some() {
            assert!(
                std::fs::read_dir("/System/Library/Fonts").is_err(),
                "the sandbox shows the system fonts"
            );
            return body();
        }
        if !cfg!(target_os = "macos") {
            eprintln!("{test}: skipped, no sandbox hides this platform's fonts");
            return;
        }
        let home = std::env::var("HOME").unwrap_or_default();
        let profile = format!(
            r#"(version 1)(allow default)(deny file-read* (subpath "/Library/Fonts") (subpath "/System/Library/Fonts") (subpath "{home}/Library/Fonts"))"#
        );
        let name = format!("{}::{test}", module_path!().split_once("::").unwrap().1);
        let out = std::process::Command::new("/usr/bin/sandbox-exec")
            .args(["-p", &profile])
            .arg(std::env::current_exe().unwrap())
            .args([name.as_str(), "--exact", "--nocapture", "--test-threads=1"])
            .env(IOS_FONTS_ONLY, "1")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            out.status.success() && stdout.contains(" 1 passed"),
            "{name} failed with the system fonts hidden:\n{stdout}\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// A DOCX of one paragraph per (font, text); `None` leaves the run in the
    /// document's default font. With `embed`, its font table embeds
    /// `NotoSansCJKsc-Embedded.otf` as Noto Sans CJK SC, unobfuscated.
    fn docx(paragraphs: &[(Option<&str>, &str)], embed: bool) -> Vec<u8> {
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
        let types = r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="otf" ContentType="application/x-font-otf"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/fontTable.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.fontTable+xml"/></Types>"#;
        let rels = |items: &[(&str, &str, &str)]| {
            let items: String = items
                .iter()
                .map(|(id, kind, target)| {
                    format!(r#"<Relationship Id="{id}" Type="{R}/{kind}" Target="{target}"/>"#)
                })
                .collect();
            format!(
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{items}</Relationships>"#
            )
        };
        let package_rels = rels(&[("rId1", "officeDocument", "word/document.xml")]);
        let document_rels = rels(&[("rId1", "fontTable", "fontTable.xml")]);
        let font_rels = rels(&[("rIdFont", "font", "fonts/font1.otf")]);
        let font_table = format!(
            r#"<w:fonts xmlns:w="{W}" xmlns:r="{R}"><w:font w:name="Noto Sans CJK SC"><w:charset w:val="86"/>{}</w:font></w:fonts>"#,
            if embed {
                r#"<w:embedRegular r:id="rIdFont"/>"#
            } else {
                ""
            }
        );
        let face = std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/docx-preview/NotoSansCJKsc-Embedded.otf"),
        )
        .unwrap();
        let mut parts: Vec<(&str, &[u8])> = vec![
            ("[Content_Types].xml", types.as_bytes()),
            ("_rels/.rels", package_rels.as_bytes()),
            ("word/document.xml", document.as_bytes()),
            ("word/_rels/document.xml.rels", document_rels.as_bytes()),
            ("word/fontTable.xml", font_table.as_bytes()),
        ];
        if embed {
            parts.push(("word/_rels/fontTable.xml.rels", font_rels.as_bytes()));
            parts.push(("word/fonts/font1.otf", &face));
        }
        super::super::tests::package(&parts)
    }

    /// `docx` rendered with the iOS font set.
    fn render_ios(docx: &[u8]) -> Vec<u8> {
        docxide_pdf::convert_docx_bytes_to_pdf_bytes_with(docx, &with_fonts(true, docx)).unwrap()
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
