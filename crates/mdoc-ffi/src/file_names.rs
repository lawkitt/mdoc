//! Titles and file names: the file stem a title gives, by one rule the
//! apps share (`file_name_stem`), and the title a file name reads back as.

/// The longest file stem a title makes, in bytes: room for " 99.md" in a
/// 255-byte file name.
const MAX_STEM: usize = 240;

/// Characters a file name cannot hold everywhere, and the look-alikes that
/// stand for them, so a title reads back from its file as it was typed. A
/// title that really holds one of the look-alikes (a fullwidth ？＂＜＞, or
/// ∕ ⧵ ꞉ ∗ ∣) reads back with the ASCII character instead; accepted, as
/// those are rare in titles.
const FILE_NAME_STAND_INS: [(char, char); 9] = [
    ('/', '\u{2215}'),  // ∕ DIVISION SLASH
    ('\\', '\u{29F5}'), // ⧵ REVERSE SOLIDUS OPERATOR
    (':', '\u{A789}'),  // ꞉ MODIFIER LETTER COLON
    ('*', '\u{2217}'),  // ∗ ASTERISK OPERATOR
    ('?', '\u{FF1F}'),  // ？ FULLWIDTH QUESTION MARK
    ('"', '\u{FF02}'),  // ＂ FULLWIDTH QUOTATION MARK
    ('<', '\u{FF1C}'),  // ＜ FULLWIDTH LESS-THAN SIGN
    ('>', '\u{FF1E}'),  // ＞ FULLWIDTH GREATER-THAN SIGN
    ('|', '\u{2223}'),  // ∣ DIVIDES
];

/// A file's title: its name without the extension, with the look-alikes a
/// file name holds read back as the characters they stand for
/// (`file_stem`).
pub(crate) fn title_of(path: &std::path::Path) -> String {
    path.file_stem().map_or_else(String::new, |stem| {
        stem.to_string_lossy()
            .chars()
            .map(|c| {
                FILE_NAME_STAND_INS
                    .iter()
                    .find(|(_, stand_in)| *stand_in == c)
                    .map_or(c, |(original, _)| *original)
            })
            .collect()
    })
}

/// The file stem for a document titled `title`: characters a file name
/// cannot hold on every platform become look-alikes (`title_of` reads them
/// back), control characters `-`; trimmed, without leading dots, at most
/// `MAX_STEM` bytes, and "Document" when nothing is left.
fn file_stem(title: &str) -> String {
    let stem: String = title
        .chars()
        .map(|c| {
            FILE_NAME_STAND_INS
                .iter()
                .find(|(original, _)| *original == c)
                .map_or(if c.is_control() { '-' } else { c }, |(_, stand_in)| {
                    *stand_in
                })
        })
        .collect();
    let mut stem = stem.trim().trim_start_matches('.');
    if stem.len() > MAX_STEM {
        let mut end = MAX_STEM;
        while !stem.is_char_boundary(end) {
            end -= 1;
        }
        stem = stem[..end].trim_end();
    }
    if stem.is_empty() {
        "Document".into()
    } else {
        stem.into()
    }
}

/// The file stem the core gives a document titled `title` (`file_stem`), so
/// the apps name the files they keep, such as a received file, by the same
/// rule.
#[uniffi::export]
pub fn file_name_stem(title: String) -> String {
    file_stem(&title)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn file_of(title: &str) -> std::path::PathBuf {
        Path::new("/documents").join(format!("{}.md", file_name_stem(title.into())))
    }

    /// A title reads back from its file as typed; the file name holds none
    /// of the characters some file systems refuse.
    #[test]
    fn a_title_reads_back_as_typed() {
        for title in [
            "Sample: services agreement",
            r#"A/B\C:D*E?F"G<H>I|J"#,
            "Договор № 5: аренда",
        ] {
            let path = file_of(title);
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            assert!(!name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']));
            assert_eq!(title_of(&path), title);
        }
        assert_eq!(title_of(&file_of(" .. ")), "Document");
        assert_eq!(title_of(&file_of("Tab\there")), "Tab-here");
        // A title too long for a file name is shortened at a character.
        let long = "Ж".repeat(200);
        let path = file_of(&long);
        assert!(path.file_name().unwrap().len() <= 255);
        assert!(long.starts_with(&title_of(&path)));
    }

    #[test]
    fn the_stem_has_no_leading_dot_and_fits_a_file_name() {
        assert_eq!(
            file_name_stem("Claim: Bennett".into()),
            "Claim\u{A789} Bennett"
        );
        assert_eq!(file_name_stem(".hidden".into()), "hidden");
        assert_eq!(file_name_stem("  ".into()), "Document");
        assert_eq!(file_name_stem("ж".repeat(200)).len(), 240);
    }
}
