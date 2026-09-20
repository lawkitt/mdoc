//! Frame-local lookup for repeated search geometry on a long shaped line.
//! GPUI's position_for_index scans glyphs and wrap boundaries for every call.
//! Build once and binary-search; unusual/nonmonotone glyph orders keep GPUI's
//! original path (including editor-owned bidi positioning).
use gpui::{Pixels, Point, WrappedLine, point};

pub(crate) struct SearchLinePositions {
    glyphs: Vec<(usize, Pixels)>,
    ends: Vec<usize>,
    width: Pixels,
    len: usize,
}

impl SearchLinePositions {
    pub fn new(line: &WrappedLine) -> Option<Self> {
        let glyphs: Vec<_> = line
            .unwrapped_layout
            .runs
            .iter()
            .flat_map(|run| {
                run.glyphs
                    .iter()
                    .map(|glyph| (glyph.index, glyph.position.x))
            })
            .collect();
        if !glyphs.windows(2).all(|pair| pair[0].0 <= pair[1].0) {
            return None;
        }
        let ends: Vec<_> = line
            .wrap_boundaries
            .iter()
            .map(|boundary| {
                line.unwrapped_layout.runs[boundary.run_ix].glyphs[boundary.glyph_ix].index
            })
            .chain([line.len()])
            .collect();
        if !ends.is_sorted() {
            return None;
        }
        Some(Self {
            glyphs,
            ends,
            width: line.unwrapped_layout.width,
            len: line.len(),
        })
    }

    fn x(&self, index: usize) -> Pixels {
        self.glyphs
            .get(self.glyphs.partition_point(|glyph| glyph.0 < index))
            .map_or(self.width, |glyph| glyph.1)
    }

    pub fn position(&self, index: usize, height: Pixels) -> Option<Point<Pixels>> {
        if index > self.len {
            return None;
        }
        // An offset exactly on a wrap boundary belongs to the preceding row,
        // matching GPUI (not an arbitrary binary_search duplicate).
        let row = self.ends.partition_point(|&end| end < index);
        let start = if row == 0 { 0 } else { self.ends[row - 1] };
        Some(point(self.x(index) - self.x(start), height * row))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EditorState;
    use gpui::{TextRun, font, px, rgba};

    #[gpui::test]
    fn indexed_positions_match_gpui_at_every_byte_and_wrap_boundary(cx: &mut gpui::TestAppContext) {
        let (_editor, cx) = cx.add_window_view(EditorState::new);
        cx.update(|window, _| {
            for text in [
                "",
                "alpha beta ",
                "office ffi café 😀 Привет ",
                "नमस्ते दुनिया कक्षा ",
                "مرحبا mixed text ",
                "שלום mixed text ",
            ] {
                let text = text.repeat(40);
                let runs = [TextRun {
                    len: text.len(),
                    font: font("Helvetica"),
                    color: rgba(0xFFFFFFFF).into(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                }];
                let lines = window
                    .text_system()
                    .shape_text(text.clone().into(), px(16.), &runs, Some(px(120.)), None)
                    .unwrap();
                for line in lines {
                    let indexed = SearchLinePositions::new(&line);
                    let monotone = line
                        .unwrapped_layout
                        .runs
                        .iter()
                        .flat_map(|run| run.glyphs.iter().map(|glyph| glyph.index))
                        .collect::<Vec<_>>()
                        .is_sorted();
                    assert_eq!(indexed.is_some(), monotone, "fallback for {text:?}");
                    if let Some(indexed) = indexed {
                        for byte in 0..=line.len() + 1 {
                            assert_eq!(
                                indexed.position(byte, px(22.)),
                                line.position_for_index(byte, px(22.)),
                                "offset {byte}"
                            );
                        }
                    }
                }
            }
        });
    }

    #[gpui::test]
    fn indexed_positions_cover_multiple_styled_runs_and_unwrapped_lines(
        cx: &mut gpui::TestAppContext,
    ) {
        let (_, cx) = cx.add_window_view(EditorState::new);
        cx.update(|window, _| {
            let first = "plain café ".repeat(30);
            let second = "bold office 😀 ".repeat(30);
            let text = format!("{first}{second}");
            let run = TextRun {
                len: first.len(),
                font: font("Helvetica"),
                color: rgba(0xFFFFFFFF).into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let mut bold = run.clone();
            bold.len = second.len();
            bold.font.weight = gpui::FontWeight::BOLD;
            for width in [None, Some(px(80.)), Some(px(300.))] {
                let lines = window
                    .text_system()
                    .shape_text(
                        text.clone().into(),
                        px(16.),
                        &[run.clone(), bold.clone()],
                        width,
                        None,
                    )
                    .unwrap();
                assert!(!lines.is_empty());
                for line in lines {
                    let indexed =
                        SearchLinePositions::new(&line).expect("LTR fast path must be exercised");
                    for byte in 0..=line.len() + 1 {
                        assert_eq!(
                            indexed.position(byte, px(22.)),
                            line.position_for_index(byte, px(22.)),
                            "offset {byte}, width {width:?}"
                        );
                    }
                }
            }
        });
    }
}
