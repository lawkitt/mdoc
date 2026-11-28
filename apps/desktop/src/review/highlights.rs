//! The review's highlights in the editor: proposals in their category's
//! colour, replacements in the alias blue and kept originals dotted, and
//! how they flash, roll and fade as a decision lands.
use super::{APPLIED_ID, KEPT_ID};
use crate::document::DocumentView;
use gpui::{Context, Hsla};
use std::ops::Range;

impl DocumentView {
    pub(crate) fn sync_pii_theme(&mut self, cx: &mut Context<Self>) {
        self.sync_annotations(cx);
    }

    /// Paint only: opening a panel or changing theme must not schedule work.
    pub(super) fn sync_annotations(&mut self, cx: &mut Context<Self>) {
        // Proposals take their category's colour; applied replacements share
        // the alias blue.
        let theme = self.theme.get();
        let accent = theme.proposed();
        let applied_accent = self.theme.get().applied();
        let review = &self.pii.review;
        let show_candidates = self.pii.reviewing;
        let mut candidates = review
            .candidates()
            .iter()
            .filter(|_| show_candidates)
            .peekable();
        let mut applied = review.applied().iter().peekable();
        let mut annotations = Vec::with_capacity(
            if show_candidates {
                review.candidates().len()
            } else {
                0
            } + review.applied().len(),
        );
        // Both inputs already follow source order; ordinary edits need no new sort.
        while candidates.peek().is_some() || applied.peek().is_some() {
            let candidate_first = match (candidates.peek(), applied.peek()) {
                (Some(candidate), Some(applied)) => candidate.range.start <= applied.range.start,
                (Some(_), None) => true,
                _ => false,
            };
            annotations.push(if candidate_first {
                let occurrence = candidates.next().unwrap();
                // A proposal: a light tint of its category with the category's
                // underline; the glyphs keep their colour.
                let category = review
                    .occurrence_identity(occurrence.variant, &occurrence.range)
                    .and_then(|id| review.identity(id))
                    .map(|identity| identity.category)
                    .or_else(|| review.variant(occurrence.variant).map(|v| v.category));
                let tint = category.map_or(accent, |c| theme.category(c));
                mdoc_editor::SourceAnnotation {
                    id: occurrence.id,
                    range: occurrence.range.clone(),
                    color: Hsla { a: 0.12, ..tint },
                    active_color: Hsla { a: 0.28, ..tint },
                    border: gpui::transparent_black(),
                    text_color: None,
                    underline: Some(Hsla { a: 0.9, ..tint }),
                    dotted: false,
                }
            } else {
                let occurrence = applied.next().unwrap();
                mdoc_editor::SourceAnnotation {
                    id: APPLIED_ID | occurrence.id,
                    range: occurrence.range.clone(),
                    color: Hsla {
                        a: 0.16,
                        ..applied_accent
                    },
                    active_color: Hsla {
                        a: 0.34,
                        ..applied_accent
                    },
                    border: Hsla {
                        a: 0.5,
                        ..applied_accent
                    },
                    text_color: Some(theme.applied_text()),
                    underline: None,
                    dotted: false,
                }
            });
        }
        // Kept originals keep a quiet dotted underline while reviewing; a
        // click reopens their controls.
        if show_candidates {
            let muted = theme.pdf_style().header_muted;
            annotations.extend(self.kept_entries().into_iter().map(|(range, _, _)| {
                mdoc_editor::SourceAnnotation {
                    id: KEPT_ID | range.start as u64,
                    range,
                    color: gpui::transparent_black(),
                    active_color: Hsla { a: 0.14, ..muted },
                    border: gpui::transparent_black(),
                    text_color: None,
                    underline: Some(Hsla { a: 0.75, ..muted }),
                    dotted: true,
                }
            }));
        }
        self.editor.update(cx, |editor, cx| {
            editor.set_annotations(editor.revision(), annotations, cx)
        });
        self.sync_selection_action(cx);
    }

    /// Replacement motion: a just-applied chip eases from the
    /// proposal gold through a soft blue glow to rest.
    pub(super) fn flash_applied(
        &mut self,
        ids: std::collections::HashSet<u64>,
        cx: &mut Context<Self>,
    ) {
        if cx.reduce_motion() {
            return;
        }
        let gold = self.theme.get().proposed();
        let blue = self.theme.get().applied();
        self.editor.update(cx, |editor, cx| {
            editor.flash_annotations(
                mdoc_editor::AnnotationFlash {
                    ids,
                    from: (Hsla { a: 0.22, ..gold }, Hsla { a: 0.6, ..gold }),
                    peak: (Hsla { a: 0.3, ..blue }, Hsla { a: 0.85, ..blue }),
                    duration: std::time::Duration::from_millis(700),
                    mark: None,
                },
                cx,
            )
        });
    }
    /// Roll each replaced span's words inside its chip. `edits`
    /// are (range in `before`, new text), applied to `before` just now.
    pub(super) fn roll_replaced(
        &mut self,
        edits: &[(Range<usize>, String)],
        before: &str,
        cx: &mut Context<Self>,
    ) {
        if cx.reduce_motion() || edits.is_empty() {
            return;
        }
        let mut sorted: Vec<_> = edits.iter().collect();
        sorted.sort_by_key(|(r, _)| r.start);
        let mut shift = 0isize;
        let items = sorted
            .into_iter()
            .filter_map(|(r, text)| {
                let start = r.start.saturating_add_signed(shift);
                shift += text.len() as isize - r.len() as isize;
                let previous = before.get(r.clone())?;
                (previous != text).then(|| (start..start + text.len(), previous.to_string().into()))
            })
            .collect();
        let surface = self.theme.get().pdf_style().bg;
        self.editor.update(cx, |editor, cx| {
            let revision = editor.revision();
            editor.roll_text(
                mdoc_editor::TextRoll {
                    revision,
                    items,
                    surface,
                    duration: std::time::Duration::from_millis(600),
                },
                cx,
            )
        });
    }
    /// Undo replacement: the restored proposal eases from blue back to gold.
    pub(super) fn flash_unapplied(
        &mut self,
        ids: std::collections::HashSet<u64>,
        cx: &mut Context<Self>,
    ) {
        if cx.reduce_motion() {
            return;
        }
        let gold = self.theme.get().proposed();
        let blue = self.theme.get().applied();
        self.editor.update(cx, |editor, cx| {
            editor.flash_annotations(
                mdoc_editor::AnnotationFlash {
                    ids,
                    from: (Hsla { a: 0.22, ..blue }, Hsla { a: 0.6, ..blue }),
                    peak: (Hsla { a: 0.28, ..gold }, Hsla { a: 0.85, ..gold }),
                    duration: std::time::Duration::from_millis(650),
                    mark: None,
                },
                cx,
            )
        });
    }
    /// Keep original: the text loses its chip, so a ghost of the chip (blue
    /// for a reverted replacement, gold for a declined proposal) fades out.
    pub(super) fn fade_out_kept(
        &mut self,
        ranges: Vec<(Range<usize>, bool)>,
        cx: &mut Context<Self>,
    ) {
        if cx.reduce_motion() {
            return;
        }
        let gold = self.theme.get().proposed();
        let blue = self.theme.get().applied();
        let ranges = ranges
            .into_iter()
            .map(|(range, applied)| {
                let c = if applied { blue } else { gold };
                (range, Hsla { a: 0.22, ..c }, Hsla { a: 0.7, ..c })
            })
            .collect();
        self.editor.update(cx, |editor, cx| {
            let revision = editor.revision();
            editor.flash_ranges(
                mdoc_editor::RangeFlash {
                    revision,
                    ranges,
                    duration: std::time::Duration::from_millis(700),
                },
                cx,
            )
        });
    }
    /// Scan results fade in together as the sweep stops.
    pub(super) fn fade_in_candidates(&mut self, cx: &mut Context<Self>) {
        if cx.reduce_motion() {
            return;
        }
        let gold = self.theme.get().proposed();
        let ids = self.pii.review.candidates().iter().map(|c| c.id).collect();
        let clear = Hsla { a: 0., ..gold };
        self.editor.update(cx, |editor, cx| {
            editor.flash_annotations(
                mdoc_editor::AnnotationFlash {
                    ids,
                    from: (clear, clear),
                    peak: (Hsla { a: 0.1, ..gold }, Hsla { a: 0.35, ..gold }),
                    duration: std::time::Duration::from_millis(250),
                    mark: None,
                },
                cx,
            )
        });
    }
}
