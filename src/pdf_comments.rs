//! A PDF's annotations on the comment rail: pdf-inspector's annotations that
//! carry text become the rail's comments, anchored at their rectangles. Text
//! markup anchors at its quads, one rectangle per line; a note or FreeText box
//! at its rectangle. Replies nest by `in_reply_to`. The PDF's own drawing of
//! them is hidden by the Source's page view ([`gpui_pdf::ViewOptions`]), so
//! mdoc's tint is the only one.
use gpui_pdf::PageFrame;
use mdoc_import::{
    comments::Comment,
    docx_preview::{AnchorRect, CommentAnchor},
};
use pdf_inspector::PdfAnnotation;

/// The rail's comments and anchors for `annotations` on pages drawn in
/// `frames`. An annotation whose page is missing, or whose box lies off the
/// visible page, has no anchor: it sits in "Not on the page". Anchors come
/// in page order, top to bottom.
pub(crate) fn rail_input(
    annotations: &[PdfAnnotation],
    frames: &[PageFrame],
) -> (Vec<Comment>, Vec<CommentAnchor>) {
    let comments = annotations
        .iter()
        .enumerate()
        .map(|(i, annotation)| Comment::from_pdf(i, annotation))
        .collect();
    let mut anchors: Vec<CommentAnchor> = annotations
        .iter()
        .enumerate()
        .filter_map(|(i, annotation)| anchor(i, annotation, frames))
        .collect();
    anchors.sort_by(|a, b| {
        let key = |anchor: &CommentAnchor| (anchor.page, anchor.rects[0].y, anchor.rects[0].x);
        key(a)
            .partial_cmp(&key(b))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    (comments, anchors)
}

fn anchor(index: usize, annotation: &PdfAnnotation, frames: &[PageFrame]) -> Option<CommentAnchor> {
    let page = usize::try_from(annotation.page).ok()?.checked_sub(1)?;
    let frame = frames.get(page)?;
    let boxes = if annotation.quads.is_empty() {
        std::slice::from_ref(&annotation.rect)
    } else {
        &annotation.quads
    };
    let rects: Vec<AnchorRect> = boxes
        .iter()
        .filter_map(|b| {
            let r = frame.norm_rect(b.x, b.y, b.width, b.height);
            // The part on the visible page.
            let (x0, y0) = (r.x.max(0.), r.y.max(0.));
            let (x1, y1) = ((r.x + r.w).min(1.), (r.y + r.h).min(1.));
            (x1 > x0 && y1 > y0).then_some(AnchorRect {
                x: x0,
                y: y0,
                w: x1 - x0,
                h: y1 - y0,
            })
        })
        .collect();
    // Lines top to bottom, whatever order the quads were written in: the
    // first is where the card and marker sit.
    let mut rects = rects;
    rects.sort_by(|a, b| {
        (a.y, a.x)
            .partial_cmp(&(b.y, b.x))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    (!rects.is_empty()).then(|| CommentAnchor {
        id: index.to_string(),
        page,
        rects,
        point: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::comment_rail::threads;
    use pdf_inspector::{AnnotationKind, AnnotationRect};
    use std::{path::PathBuf, sync::Arc};

    fn fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pdf-source/comments.pdf")
    }

    fn close(a: &AnchorRect, expected: [f32; 4]) -> bool {
        [a.x, a.y, a.w, a.h]
            .iter()
            .zip(expected)
            .all(|(got, want)| (got - want).abs() < 0.002)
    }

    /// The generated fixture's annotations land where its pages draw them:
    /// on page 1 through an offset CropBox, on page 2 through a quarter turn.
    #[test]
    fn the_fixture_anchors_where_its_pages_draw() {
        let source = mdoc_import::import::Source::read(&fixture()).unwrap();
        let annotations = mdoc_import::import::source_annotations(&source, None).unwrap();
        let doc = gpui_pdf::parse(Arc::new(std::fs::read(fixture()).unwrap())).unwrap();
        let frames = gpui_pdf::page_frames(&doc);
        assert_eq!(frames.len(), 2);
        assert_eq!(
            (frames[0].width, frames[0].height, frames[0].quarter_turns),
            (540., 720., 0)
        );
        assert_eq!(
            (frames[1].width, frames[1].height, frames[1].quarter_turns),
            (612., 792., 1)
        );
        let (comments, anchors) = rail_input(&annotations, &frames);
        let texts: Vec<&str> = comments.iter().map(|c| c.text.as_str()).collect();
        assert_eq!(
            texts,
            [
                "Check the delivery date with logistics.",
                "Logistics confirmed 1 March.",
                "Is this cap right? It looks low.",
                "Yes, it matches the 2025 schedule.",
                "Signed copy to follow.",
                "Should this be 60 days?",
                "Change to 30.",
                "Or Wales?",
                "Add the Leeds site.",
                "Landscape table: check the dates.",
            ]
        );
        let anchor = |id: &str| anchors.iter().find(|a| a.id == id).unwrap();
        // The note's icon at [500 652 520 672], less the CropBox's corner
        // (36, 36), from the visible box's top-left: 540 × 720.
        let note = anchor("0");
        assert_eq!(note.page, 0);
        assert!(
            close(
                &note.rects[0],
                [464. / 540., 84. / 720., 20. / 540., 20. / 720.]
            ),
            "{note:?}"
        );
        // The FreeText box at [330 470 560 510].
        assert!(close(
            &anchor("4").rects[0],
            [294. / 540., 246. / 720., 230. / 540., 40. / 720.]
        ));
        // The highlight's quad on line 2 (baseline 636: 633 to 646).
        let cap = &anchor("2").rects[0];
        assert!((cap.y - 110. / 720.).abs() < 0.002 && (cap.h - 13. / 720.).abs() < 0.002);
        assert_eq!(comments[2].anchor_text, "capped at the fees paid");
        // Page 2 is turned a quarter clockwise: a box at x 150..163 (up the
        // unturned page) lands 150/612 down the turned one, and its run up
        // the page runs right from the turned page's left edge.
        let bristol = anchor("8");
        assert_eq!(bristol.page, 1);
        let r = bristol.rects[0];
        assert!(
            (r.y - 150. / 612.).abs() < 0.002 && (r.h - 13. / 612.).abs() < 0.002,
            "{r:?}"
        );
        assert!((r.x - 72. / 792.).abs() < 0.002, "{r:?}");
        // Each markup anchor covers the ink of its words, which the
        // page's content draws; just above the line is blank.
        for id in ["2", "5", "6", "7", "8"] {
            let a = anchor(id);
            let pixels = gpui_pdf::page_luma(&doc, a.page, 2.).unwrap();
            let r = a.rects[0];
            let ink = darkest(&pixels, r.x, r.y, r.w, r.h);
            // Black text, not the markup's own coloured drawing.
            assert!(ink < 40, "{id}: no ink under {r:?}");
            // Turned or not, the line before is above, with blank between.
            let above = darkest(&pixels, r.x, r.y - r.h * 0.6, r.w, r.h * 0.5);
            assert!(above > 200, "{id}: ink above {r:?}");
        }
        // Rail order is page order; replies nest under their annotation and
        // the review state resolved the note.
        let (placed, nowhere) = threads(&comments, &anchors);
        let order: Vec<&str> = placed.iter().map(|t| t.comment.id.as_str()).collect();
        assert_eq!(order, ["0", "2", "5", "6", "7", "4", "9", "8"]);
        assert!(nowhere.is_empty());
        assert!(placed[0].comment.resolved);
        assert_eq!(placed[0].replies[0].text, "Logistics confirmed 1 March.");
        assert_eq!(placed[1].replies[0].author, "Boris Ivanov");
        assert!(!placed[1].comment.resolved);
    }

    /// The darkest luma in a rectangle (fractions of the page).
    fn darkest(pixels: &(u32, u32, Vec<u8>), x: f32, y: f32, w: f32, h: f32) -> u8 {
        let (pw, ph, luma) = pixels;
        let span = |from: f32, len: f32, size: u32| {
            let size = size as f32;
            ((from * size).max(0.) as u32)..(((from + len) * size).min(size) as u32)
        };
        let mut darkest = 255;
        for py in span(y, h, *ph) {
            for px in span(x, w, *pw) {
                darkest = darkest.min(luma[(py * pw + px) as usize]);
            }
        }
        darkest
    }

    fn annotation(page: u32, rect: AnnotationRect) -> PdfAnnotation {
        PdfAnnotation {
            page,
            kind: AnnotationKind::Text,
            author: None,
            date: None,
            text: "Note".into(),
            rect,
            quads: Vec::new(),
            in_reply_to: None,
            resolved: false,
            marked_text: None,
        }
    }

    /// A highlight over two lines written bottom quad first anchors at its
    /// top line.
    #[test]
    fn quads_anchor_at_their_top_line() {
        let frames = [PageFrame {
            width: 100.,
            height: 100.,
            quarter_turns: 0,
        }];
        let line = |y| AnnotationRect {
            x: 10.,
            y,
            width: 50.,
            height: 10.,
        };
        let mut highlight = annotation(1, line(10.));
        highlight.quads = vec![line(10.), line(80.)];
        let (_, anchors) = rail_input(&[highlight], &frames);
        let ys: Vec<f32> = anchors[0].rects.iter().map(|r| r.y).collect();
        assert!(
            (ys[0] - 0.1).abs() < 1e-4 && (ys[1] - 0.8).abs() < 1e-4,
            "{ys:?}"
        );
    }

    #[test]
    fn a_missing_page_or_a_box_off_the_page_has_no_place() {
        let frames = [PageFrame {
            width: 100.,
            height: 200.,
            quarter_turns: 0,
        }];
        let rect = |x, y| AnnotationRect {
            x,
            y,
            width: 20.,
            height: 10.,
        };
        let annotations = [
            annotation(1, rect(10., 180.)),
            annotation(2, rect(10., 10.)),
            annotation(1, rect(150., 10.)),
            // Half over the right edge: the part on the page.
            annotation(1, rect(90., 10.)),
            annotation(1, rect(10., 20.)),
        ];
        let (comments, anchors) = rail_input(&annotations, &frames);
        assert_eq!(comments.len(), 5);
        let ids: Vec<&str> = anchors.iter().map(|a| a.id.as_str()).collect();
        // Top to bottom: the first sits near the top.
        assert_eq!(ids, ["0", "4", "3"]);
        assert!(close(&anchors[0].rects[0], [0.1, 0.05, 0.2, 0.05]));
        assert!(close(&anchors[2].rects[0], [0.9, 0.9, 0.1, 0.05]));
        let (placed, nowhere) = threads(&comments, &anchors);
        assert_eq!(placed.len(), 3);
        let lost: Vec<&str> = nowhere.iter().map(|t| t.comment.id.as_str()).collect();
        assert_eq!(lost, ["1", "2"]);
    }
}
