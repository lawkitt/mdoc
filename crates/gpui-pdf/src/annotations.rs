//! Comment annotations, for a host that shows them itself (mdoc's comment
//! rail): where an annotation's box lands on the page as drawn
//! ([`PageFrame`]), and a pass that leaves the PDF's own drawing of them out
//! so nothing draws twice ([`hide_comment_annotations`]).
//!
//! The comment annotations are the ones that carry text, as pdf-inspector
//! reports them: sticky notes (`/Text`), FreeText, and highlight, underline,
//! strikeout and squiggly markup with `/Contents` or `/RC`, plus every
//! `/Popup` and the review-state notes (`/Text` with `/State`). Everything
//! else stays drawn because it is part of the page: stamps, ink, form
//! widgets, links, markup without text and FreeText that is only an
//! appearance.

use lopdf::{Dictionary, Document, Object};

use crate::NormRect;

/// Annotation flag 2 (PDF 32000-1, 12.5.3): not drawn or printed.
const HIDDEN: i64 = 1 << 1;

/// A page's visible box (CropBox within MediaBox) before `/Rotate`: the
/// frame annotation geometry is measured in, and the turn that draws it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageFrame {
    /// The visible box's size in points, unrotated.
    pub width: f32,
    pub height: f32,
    /// `/Rotate` in clockwise quarter turns, 0 to 3.
    pub quarter_turns: u8,
}

impl PageFrame {
    pub(crate) fn of(page: &hayro::hayro_syntax::page::Page<'_>) -> Self {
        use hayro::hayro_syntax::page::Rotation;
        let (width, height) = page.base_dimensions();
        Self {
            width,
            height,
            quarter_turns: match page.rotation() {
                Rotation::None => 0,
                Rotation::Horizontal => 1,
                Rotation::Flipped => 2,
                Rotation::FlippedHorizontal => 3,
            },
        }
    }

    /// A box in points from the visible box's lower-left corner, `y` up,
    /// before `/Rotate` (pdf-inspector's annotation frame), on the page as
    /// drawn: normalized, top-left origin, the page turned clockwise.
    pub fn norm_rect(&self, x: f32, y: f32, width: f32, height: f32) -> NormRect {
        // Normalized on the unturned page, top-left origin.
        let (w, h) = (self.width.max(1.), self.height.max(1.));
        let (u, v, du, dv) = (x / w, 1. - (y + height) / h, width / w, height / h);
        match self.quarter_turns % 4 {
            0 => NormRect {
                x: u,
                y: v,
                w: du,
                h: dv,
            },
            // A quarter turn clockwise takes (u, v) to (1 - v, u).
            1 => NormRect {
                x: 1. - v - dv,
                y: u,
                w: dv,
                h: du,
            },
            2 => NormRect {
                x: 1. - u - du,
                y: 1. - v - dv,
                w: du,
                h: dv,
            },
            // Three quarter turns take (u, v) to (v, 1 - u).
            _ => NormRect {
                x: v,
                y: 1. - u - du,
                w: dv,
                h: du,
            },
        }
    }
}

/// Set the hidden flag on every comment annotation of `doc` (see the module
/// header): those in object streams or as objects, and those written inline
/// in a page's `/Annots`. True when something changed.
pub(crate) fn hide_comment_annotations(doc: &mut Document) -> bool {
    let mut changed = false;
    for object in doc.objects.values_mut() {
        match object {
            Object::Dictionary(dict) => {
                changed |= hide(dict);
                // A page whose `/Annots` array holds the dictionaries.
                if let Ok(Object::Array(annots)) = dict.get_mut(b"Annots") {
                    changed |= hide_inline(annots);
                }
            }
            // An `/Annots` array written as its own object.
            Object::Array(annots) => changed |= hide_inline(annots),
            _ => {}
        }
    }
    changed
}

fn hide_inline(annots: &mut [Object]) -> bool {
    let mut changed = false;
    for annot in annots {
        if let Object::Dictionary(dict) = annot {
            changed |= hide(dict);
        }
    }
    changed
}

/// Hide `dict` if it is a comment annotation not hidden yet.
fn hide(dict: &mut Dictionary) -> bool {
    let Ok(subtype) = dict.get(b"Subtype").and_then(Object::as_name) else {
        return false;
    };
    // Every annotation has a `/Rect`; nothing else with these subtypes does.
    if !dict.has(b"Rect") {
        return false;
    }
    let comment = match subtype {
        b"Popup" => true,
        b"Text" => carries_text(dict) || dict.has(b"State"),
        b"FreeText" | b"Highlight" | b"Underline" | b"StrikeOut" | b"Squiggly" => {
            carries_text(dict)
        }
        _ => false,
    };
    let flags = match dict.get(b"F") {
        Ok(Object::Integer(flags)) => *flags,
        Ok(Object::Real(flags)) => *flags as i64,
        _ => 0,
    };
    if !comment || flags & HIDDEN != 0 {
        return false;
    }
    dict.set("F", flags | HIDDEN);
    true
}

/// `/Contents` or `/RC` holds text: a string that is not blank, or an
/// indirect value or stream, which pdf-inspector reads as text.
fn carries_text(dict: &Dictionary) -> bool {
    [b"Contents".as_slice(), b"RC"]
        .iter()
        .any(|key| match dict.get(key) {
            Ok(Object::String(bytes, _)) => {
                !crate::forms::decode_pdf_string(bytes).trim().is_empty()
            }
            Ok(Object::Reference(_) | Object::Stream(_)) => true,
            _ => false,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ViewOptions, page_frames, parse_with_password};
    use lopdf::{Stream, dictionary};
    use std::sync::Arc;

    fn rect(x0: i64, y0: i64, x1: i64, y1: i64) -> Vec<Object> {
        vec![x0.into(), y0.into(), x1.into(), y1.into()]
    }

    /// A black box filling `bbox`, as an appearance stream.
    fn black_box(doc: &mut Document, w: i64, h: i64) -> Object {
        let id = doc.add_object(Stream::new(
            dictionary! { "Type" => "XObject", "Subtype" => "Form",
            "BBox" => rect(0, 0, w, h) },
            format!("0 g 0 0 {w} {h} re f").into_bytes(),
        ));
        Object::Reference(id)
    }

    /// One page, 400×400, holding `annots`; `extra` page entries
    /// (`/Rotate`, `/CropBox`) and `content` as given.
    fn one_page(
        doc: &mut Document,
        annots: Vec<Object>,
        extra: Dictionary,
        content: &str,
    ) -> Vec<u8> {
        let content = doc.add_object(Stream::new(dictionary! {}, content.as_bytes().to_vec()));
        let page = doc.new_object_id();
        let pages = doc.add_object(dictionary! {
            "Type" => "Pages", "Kids" => vec![Object::Reference(page)], "Count" => 1,
        });
        let mut dict = dictionary! {
            "Type" => "Page", "Parent" => pages,
            "MediaBox" => rect(0, 0, 400, 400),
            "Contents" => content,
            "Annots" => annots,
        };
        dict.extend(&extra);
        doc.objects.insert(page, Object::Dictionary(dict));
        let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
        doc.trailer.set("Root", catalog);
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    /// The annotation flags by `/NM`, after the pass.
    fn flags_after(bytes: &[u8]) -> Vec<(String, i64)> {
        let mut doc = Document::load_mem(bytes).unwrap();
        hide_comment_annotations(&mut doc);
        let page = *doc.get_pages().values().next().unwrap();
        let annots = doc
            .get_dictionary(page)
            .unwrap()
            .get(b"Annots")
            .unwrap()
            .as_array()
            .unwrap()
            .clone();
        annots
            .iter()
            .map(|annot| {
                let dict = match annot {
                    Object::Reference(id) => doc.get_dictionary(*id).unwrap(),
                    Object::Dictionary(dict) => dict,
                    _ => panic!("annotation"),
                };
                let name = String::from_utf8_lossy(dict.get(b"NM").unwrap().as_str().unwrap());
                let flags = dict.get(b"F").and_then(Object::as_i64).unwrap_or(0);
                (name.into_owned(), flags)
            })
            .collect()
    }

    #[test]
    fn comment_annotations_hide_and_the_page_stays() {
        let mut doc = Document::with_version("1.7");
        let rc = doc.add_object(Object::string_literal("<body><p>Rich</p></body>"));
        let ap = black_box(&mut doc, 10, 10);
        let annot = |doc: &mut Document, name: &str, subtype: &str, extra: Dictionary| {
            let mut dict = dictionary! {
                "Type" => "Annot", "Subtype" => subtype,
                "NM" => Object::string_literal(name),
                "Rect" => rect(10, 10, 20, 20), "F" => 4,
            };
            dict.extend(&extra);
            Object::Reference(doc.add_object(dict))
        };
        let text = |s: &str| Object::string_literal(s);
        let annots = vec![
            annot(
                &mut doc,
                "note",
                "Text",
                dictionary! { "Contents" => text("Check") },
            ),
            annot(&mut doc, "popup", "Popup", dictionary! {}),
            annot(
                &mut doc,
                "freetext",
                "FreeText",
                dictionary! { "Contents" => text("Box"), "AP" => dictionary! { "N" => ap.clone() } },
            ),
            annot(
                &mut doc,
                "drawn-freetext",
                "FreeText",
                dictionary! { "AP" => dictionary! { "N" => ap.clone() } },
            ),
            annot(
                &mut doc,
                "highlight",
                "Highlight",
                dictionary! { "Contents" => text("Why?") },
            ),
            annot(&mut doc, "plain-highlight", "Highlight", dictionary! {}),
            annot(
                &mut doc,
                "underline",
                "Underline",
                dictionary! { "RC" => rc },
            ),
            annot(
                &mut doc,
                "blank-strike",
                "StrikeOut",
                dictionary! { "Contents" => text("  ") },
            ),
            annot(
                &mut doc,
                "squiggly",
                "Squiggly",
                // UTF-16BE "Да".
                dictionary! { "Contents" => Object::String(vec![0xFE, 0xFF, 0x04, 0x14, 0x04, 0x30], lopdf::StringFormat::Hexadecimal) },
            ),
            annot(
                &mut doc,
                "state",
                "Text",
                dictionary! { "State" => text("Completed") },
            ),
            annot(
                &mut doc,
                "stamp",
                "Stamp",
                dictionary! { "Contents" => text("Approved") },
            ),
            annot(
                &mut doc,
                "ink",
                "Ink",
                dictionary! { "Contents" => text("Signature") },
            ),
            annot(
                &mut doc,
                "widget",
                "Widget",
                dictionary! { "Contents" => text("Field") },
            ),
            annot(
                &mut doc,
                "link",
                "Link",
                dictionary! { "Contents" => text("Go") },
            ),
            // Written inline in `/Annots`, and one hidden already.
            Object::Dictionary(dictionary! {
                "Type" => "Annot", "Subtype" => "Text", "NM" => text("inline"),
                "Rect" => rect(10, 10, 20, 20), "Contents" => text("Inline"),
            }),
            annot(
                &mut doc,
                "hidden",
                "Text",
                dictionary! { "Contents" => text("Gone"), "F" => 2 },
            ),
        ];
        let bytes = one_page(&mut doc, annots, dictionary! {}, "");
        let hidden: Vec<String> = flags_after(&bytes)
            .into_iter()
            .filter(|(_, flags)| flags & HIDDEN != 0)
            .map(|(name, _)| name)
            .collect();
        assert_eq!(
            hidden,
            [
                "note",
                "popup",
                "freetext",
                "highlight",
                "underline",
                "squiggly",
                "state",
                "inline",
                "hidden"
            ]
        );
        // Other flags stay (Print, 4).
        assert!(flags_after(&bytes).contains(&("note".into(), 4 | HIDDEN)));
        // Nothing to hide leaves the bytes alone.
        let mut plain = Document::with_version("1.7");
        let stamp = Object::Reference(plain.add_object(dictionary! {
            "Type" => "Annot", "Subtype" => "Stamp", "Rect" => rect(0, 0, 9, 9),
        }));
        let plain = one_page(&mut plain, vec![stamp], dictionary! {}, "");
        let options = ViewOptions {
            hide_comment_annotations: true,
        };
        assert!(crate::display_bytes(&plain, options).is_none());
        assert!(crate::display_bytes(&bytes, options).is_some());
        assert!(crate::display_bytes(&bytes, ViewOptions::default()).is_none());
    }

    /// The darkest luma in a 3×3 spot around `(x, y)` (fractions of the
    /// page) of a page rendered at 1 pt per pixel.
    fn ink_at(pixels: &(u32, u32, Vec<u8>), x: f32, y: f32) -> u8 {
        let (w, h, luma) = pixels;
        let (cx, cy) = ((x * *w as f32) as i64, (y * *h as f32) as i64);
        let mut darkest = 255;
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (px, py) = (cx + dx, cy + dy);
                if px >= 0 && py >= 0 && px < *w as i64 && py < *h as i64 {
                    darkest = darkest.min(luma[(py * *w as i64 + px) as usize]);
                }
            }
        }
        darkest
    }

    #[test]
    fn hidden_appearances_are_not_drawn_and_stamps_are() {
        let mut doc = Document::with_version("1.7");
        let ap = black_box(&mut doc, 40, 40);
        let freetext = doc.add_object(dictionary! {
            "Type" => "Annot", "Subtype" => "FreeText", "Contents" => Object::string_literal("Box"),
            "Rect" => rect(20, 20, 60, 60), "AP" => dictionary! { "N" => ap.clone() },
        });
        let stamp = doc.add_object(dictionary! {
            "Type" => "Annot", "Subtype" => "Stamp",
            "Rect" => rect(200, 20, 240, 60), "AP" => dictionary! { "N" => ap },
        });
        let bytes = Arc::new(one_page(
            &mut doc,
            vec![Object::Reference(freetext), Object::Reference(stamp)],
            dictionary! {},
            "",
        ));
        let render = |hide| {
            let options = ViewOptions {
                hide_comment_annotations: hide,
            };
            let doc = parse_with_password(bytes.clone(), "", options).unwrap();
            crate::page_luma(&doc, 0, 1.).unwrap()
        };
        // FreeText at (40, 40) from the bottom; the stamp at (220, 40).
        let (freetext, stamp) = ((40. / 400., 360. / 400.), (220. / 400., 360. / 400.));
        let shown = render(false);
        assert!(ink_at(&shown, freetext.0, freetext.1) < 50);
        let hidden = render(true);
        assert!(ink_at(&hidden, freetext.0, freetext.1) > 200);
        assert!(ink_at(&hidden, stamp.0, stamp.1) < 50);
    }

    #[test]
    fn a_box_lands_where_the_turned_page_draws_it() {
        // A black box 40×20 at (50, 100) from the CropBox's lower-left
        // corner, drawn by the page's content; the CropBox is offset.
        for turns in 0..4u8 {
            let mut doc = Document::with_version("1.7");
            let bytes = Arc::new(one_page(
                &mut doc,
                Vec::new(),
                dictionary! {
                    "CropBox" => rect(30, 40, 330, 390),
                    "Rotate" => i64::from(turns) * 90,
                },
                "0 g 80 140 40 20 re f",
            ));
            let doc = crate::parse(bytes).unwrap();
            let frame = page_frames(&doc)[0];
            assert_eq!(
                (frame.width, frame.height, frame.quarter_turns),
                (300., 350., turns)
            );
            let r = frame.norm_rect(50., 100., 40., 20.);
            let pixels = crate::page_luma(&doc, 0, 1.).unwrap();
            let turned = turns % 2 == 1;
            assert_eq!(
                (pixels.0, pixels.1),
                if turned { (350, 300) } else { (300, 350) }
            );
            let (mx, my) = (r.x + r.w / 2., r.y + r.h / 2.);
            assert!(ink_at(&pixels, mx, my) < 50, "inside, {turns} turns");
            // The page's other half is blank: a misplaced box would be there.
            assert!(
                ink_at(&pixels, 1. - mx, 1. - my) > 200,
                "mirrored, {turns} turns"
            );
            // Its size turns with the page.
            let expected = if turned {
                (20. / 350., 40. / 300.)
            } else {
                (40. / 300., 20. / 350.)
            };
            assert!((r.w - expected.0).abs() < 1e-4 && (r.h - expected.1).abs() < 1e-4);
        }
    }
}
