//! Word-anchored editor for the shared replacement workspace.
use super::{
    Applying, MappingAction,
    popup::{self, Scope, Step},
};
use crate::{
    document_view::DocumentView,
    pii::ui::{APPLIED_ID, KEPT_ID, PiiClosePopup, PiiConfirm, Popup},
    style,
};
use gpui::{
    Animation, AnimationExt, AnyElement, App, Context, Focusable, Hsla, Pixels, SharedString,
    Window, anchored, deferred, div, prelude::*, px, uniform_list,
};
use mdoc_pii::Category;
use std::{ops::Range, sync::Arc, time::Duration};

/// The quiet button look: a thin border, regular weight.
fn secondary(
    v: gpui::Stateful<gpui::Div>,
    theme: crate::style::Theme,
) -> gpui::Stateful<gpui::Div> {
    v.px_3()
        .py_1()
        .rounded_md()
        .border_1()
        .border_color(theme.pdf_style().border)
}
/// A 12 × 6 px notch centred at `at` on the card's top edge (`up`) or bottom
/// edge, filled like the card and outlined like its border.
fn notch(up: bool, fill: Hsla, border: Hsla, at: Pixels) -> gpui::Canvas<()> {
    const W: f32 = 12.;
    const H: f32 = 6.;
    let canvas = gpui::canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let o = bounds.origin;
            let (tip, base) = if up { (0., H + 1.) } else { (H + 1., 0.) };
            let p = |x: f32, y: f32| o + gpui::point(px(x), px(y));
            let mut shape = gpui::PathBuilder::fill();
            shape.move_to(p(0., base));
            shape.line_to(p(W / 2., tip));
            shape.line_to(p(W, base));
            shape.close();
            if let Ok(path) = shape.build() {
                window.paint_path(path, fill);
            }
            let mut edge = gpui::PathBuilder::stroke(px(1.));
            edge.move_to(p(0., if up { H } else { 1. }));
            edge.line_to(p(W / 2., tip));
            edge.line_to(p(W, if up { H } else { 1. }));
            if let Ok(path) = edge.build() {
                window.paint_path(path, border);
            }
        },
    )
    .absolute()
    .left(at - px(W / 2.))
    .w(px(W))
    .h(px(H + 1.));
    if up {
        canvas.top(px(-H))
    } else {
        canvas.bottom(px(-H))
    }
}
/// The alias cue: one soft accent glow, `delay` ms after the popup opens.
fn cue_glow(delta: f32, delay: f32, accent: Hsla) -> Hsla {
    const TOTAL: f32 = 850.;
    let local = ((delta * TOTAL - delay) / 700.).clamp(0., 1.);
    Hsla {
        a: 0.24 * (local * std::f32::consts::PI).sin(),
        ..accent
    }
}
impl DocumentView {
    pub(super) fn readable_mention(
        &self,
        range: &Range<usize>,
        cx: &App,
    ) -> (String, String, String) {
        let source = self.editor.read(cx).text();
        if range.end > source.len()
            || !source.is_char_boundary(range.start)
            || !source.is_char_boundary(range.end)
        {
            return Default::default();
        }
        let (start, end) = snippet_bounds(source, range);
        fn clean(value: &str) -> String {
            static TAGS: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
            let value = TAGS
                .get_or_init(|| regex::Regex::new(r"<[^>]*>").unwrap())
                .replace_all(value, " ");
            value
                .replace(['*', '`', '|', '\\', '#'], "")
                .replace(['\n', '\r'], " ")
        }
        (
            clean(&source[start..range.start]),
            clean(&source[range.clone()]),
            clean(&source[range.end..end]),
        )
    }
    fn popup_control(
        &self,
        id: impl Into<gpui::ElementId>,
        label: impl Into<SharedString>,
        enabled: bool,
        cx: &App,
    ) -> gpui::Stateful<gpui::Div> {
        let id = id.into();
        let debug = id.to_string();
        self.pii.reveal_popup_control(
            id.clone(),
            crate::ui::control(id, label, self.theme.get(), enabled)
                .when(cfg!(test), |v| v.debug_selector(move || debug.clone())),
            cx,
        )
    }
    /// A floating-menu row: focusable, but it sits outside the card's scroll
    /// area, so focusing it must not scroll the card (and shift the menu
    /// between press and release, dropping the click).
    pub(super) fn menu_control(
        &self,
        id: impl Into<gpui::ElementId>,
        enabled: bool,
        cx: &App,
    ) -> gpui::Stateful<gpui::Div> {
        let id = id.into();
        let debug = id.to_string();
        let focus = self.pii.popup_control_focus(id.clone(), cx);
        crate::ui::control(id, "", self.theme.get(), enabled)
            .when(cfg!(test), |v| v.debug_selector(move || debug.clone()))
            .track_focus(&focus)
    }
    /// The mention bar (ADR 0048, ADR 0055), placed in the scrolled content
    /// beside the editor so it moves with its mention in the same frame.
    pub(in crate::pii::ui) fn replacement_popup(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !matches!(self.pii.popup, Some(Popup::Selection))
            || self.pii.restore.as_ref().is_some_and(|r| r.popup.is_some())
        {
            return None;
        }
        let annotation = self.active_annotation()?;
        let id = self.selected_entity()?;
        let (alias, category) = self.selected_name()?;
        let range = self.active_replacement_range()?;
        let original = self.active_original()?;
        let review = &self.pii.review;
        let kept = self.active_is_kept();
        // A kept mention shows its name read-only; ticking Replace brings
        // the alias and category controls back.
        let identity = review.identity(id).filter(|_| !kept);
        let mapping = &self.pii.mapping;
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let enabled = !self.pii.scanning();
        // The mention's own colour, as the editor draws it.
        let accent = if annotation & APPLIED_ID != 0 {
            theme.applied()
        } else {
            theme.category(category)
        };
        // Cancel addition while this mention's addition is the latest step.
        let cancel = !kept
            && mapping
                .added_at(self.editor.read(cx).history_id())
                .is_some_and(|(added, _)| added.original == original);
        let applied = annotation & APPLIED_ID != 0;
        let mentions = self.popup_occurrences(id);
        let index = mentions
            .iter()
            .position(|(a, _)| *a == annotation)
            .unwrap_or(0);
        let selected_count = self.scoped_ranges(mapping.scope).len();
        let draft = mapping.alias.read(cx).value().trim().to_string();
        let changed = draft != alias;
        let input_focused = mapping.alias.read(cx).focus_handle(cx).is_focused(window);
        let choices = identity.is_some() && (mapping.pickers.alias || input_focused);
        let query = if changed {
            draft.to_lowercase()
        } else {
            String::new()
        };
        let targets: Arc<[_]> = if choices {
            self.alias_targets(&query, false).into()
        } else {
            Vec::new().into()
        };
        // Contacts and identifiers can belong to a person or organization;
        // the category menu holds that choice (ADR 0055).
        let ownable = identity.is_some_and(|i| {
            i.owner.is_some()
                || !matches!(
                    i.category,
                    Category::Person | Category::Organization | Category::Date | Category::Other
                )
        });
        let owner = identity
            .and_then(|i| i.owner)
            .and_then(|owner| review.identity(owner))
            .map(|owner| owner.alias.clone());

        // Geometry, in the editor's own coordinates (ADR 0055). The bar keeps
        // its mention's line and the line below visible, flips above under
        // the same rule, and hides (keeping its state) while the mention is
        // scrolled out of view.
        let editor = self.editor.read(cx);
        // A row is taller than the base line height and the mention's box is
        // inset inside its row: clear a little more than one line.
        let line = editor.base_line_height().max(px(12.)) * 1.25;
        let hidden = editor.annotation_is_hidden(annotation);
        let local = editor.local_annotation_bounds(annotation, range.start);
        if local.is_some() {
            self.pii.popup_anchor.set(local);
        }
        let layer = self.pii.popup_layer.get();
        if local.is_none() || layer.is_none() {
            // Not painted yet (new text or first frame): look again next frame.
            window.request_animation_frame();
        }
        // Until both are known the bar stays in the tree (it holds focus and
        // the keys) but invisible, never drawn at a guessed spot.
        let placed = local.or(self.pii.popup_anchor.get()).zip(layer);
        let (anchor, (layer, scrolled)) = placed.unwrap_or_default();
        let view = self.scroll.bounds();
        let origin = layer + (self.scroll.offset() - scrolled);
        let top = origin.y + anchor.top();
        let bottom = origin.y + anchor.bottom();
        let offscreen = placed.is_none() || bottom < view.top() || top > view.bottom();
        let measured = self.pii.popup_size.get();
        let bar = measured.height;
        let room_below = view.bottom() - bottom - line - px(4.);
        let room_above = top - line - view.top() - px(4.);
        let below = room_below >= bar || room_below >= room_above;
        // The card fits its content, capped at the view (ADR 0055).
        let max_width = (view.size.width - px(8.)).max(px(120.));
        let width = measured.width.min(max_width);
        // One row; only a truly narrow view lets the bar wrap.
        let narrow = max_width < px(560.);
        let left_min = view.left() + px(4.) - origin.x;
        let left_max = (view.right() - width - px(4.) - origin.x).max(left_min);
        let x = anchor.left().clamp(left_min, left_max);
        let (corner, y) = if offscreen {
            // Out of the window entirely, so its hitbox covers nothing.
            (gpui::Anchor::TopLeft, px(-100_000.))
        } else if below {
            (gpui::Anchor::TopLeft, anchor.bottom() + line)
        } else {
            (gpui::Anchor::BottomLeft, anchor.top() - line)
        };
        let height = (if below { room_below } else { room_above }).max(px(60.));
        // Re-recorded by whichever menu paints this frame.
        mapping.menu_bounds.set(None);
        let mut panel = crate::ui::panel("direct-replacement-popup", theme)
            .when(cfg!(test), |v| {
                v.debug_selector(|| "pseudonym-popup".into())
            })
            .key_context(if choices {
                "PseudonymReview UiPanel PiiChooser"
            } else {
                "PseudonymReview UiPanel"
            })
            .tab_group()
            .tab_stop(false)
            .track_focus(&self.pii.focus)
            .map(|v| self.popup_decision_actions(v, cx))
            .max_w(max_width)
            .max_h(height)
            .overflow_y_scroll()
            .track_scroll(&self.pii.popup_scroll)
            .px(px(6.))
            .py(px(4.))
            .rounded_lg()
            .border_color(theme.popup_border())
            .shadow_lg()
            .flex()
            .flex_col()
            .gap_1()
            .text_size(px(12.))
            // Hidden by opacity, not visibility: a hidden element skips paint,
            // where its key and action listeners register.
            .when(offscreen, |v| v.opacity(0.))
            .on_action(cx.listener(|this, _: &crate::ui::NextControl, window, cx| {
                crate::ui::cycle(window, cx, Some(&this.pii.focus), false);
                cx.stop_propagation();
            }))
            .on_action(
                cx.listener(|this, _: &crate::ui::PreviousControl, window, cx| {
                    crate::ui::cycle(window, cx, Some(&this.pii.focus), true);
                    cx.stop_propagation();
                }),
            )
            .on_action(
                cx.listener(|this, _: &crate::pii::ui::PiiNextChoice, _, cx| {
                    this.step_alias_choice(false, cx);
                    cx.stop_propagation();
                }),
            )
            .on_action(
                cx.listener(|this, _: &crate::pii::ui::PiiPreviousChoice, _, cx| {
                    this.step_alias_choice(true, cx);
                    cx.stop_propagation();
                }),
            )
            .on_action(
                cx.listener(|this, _: &crate::pii::ui::PiiOpenChoice, window, cx| {
                    this.open_alias_choice(window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_action(cx.listener(|this, _: &PiiClosePopup, window, cx| {
                this.escape_popup(window, cx);
                cx.stop_propagation();
            }))
            // Document undo from the popup, which keeps focus after a keep.
            .on_action(cx.listener(|this, _: &mdoc_editor::Undo, window, cx| {
                this.undo_last_step(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &mdoc_editor::Redo, window, cx| {
                this.redo_last_step(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(
                |this, _: &crate::pii::ui::PopupPreviousMention, window, cx| {
                    this.step_identity_occurrence(Step::Previous, window, cx);
                    cx.stop_propagation();
                },
            ))
            .on_action(
                cx.listener(|this, _: &crate::pii::ui::PopupNextMention, window, cx| {
                    this.step_identity_occurrence(Step::Next, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_action(
                cx.listener(|this, _: &crate::pii::ui::PopupFirstMention, window, cx| {
                    this.step_identity_occurrence(Step::First, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_action(
                cx.listener(|this, _: &crate::pii::ui::PopupLastMention, window, cx| {
                    this.step_identity_occurrence(Step::Last, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_action(cx.listener(|this, _: &PiiConfirm, window, cx| {
                if this
                    .pii
                    .mapping
                    .alias
                    .read(cx)
                    .focus_handle(cx)
                    .is_focused(window)
                {
                    this.open_alias_choice(window, cx);
                } else if this.pii.enter_applies
                    && this
                        .active_annotation()
                        .is_some_and(|a| a & (APPLIED_ID | KEPT_ID) == 0)
                {
                    // After a manual addition, Apply is the default action.
                    this.apply_scope(cx);
                }
                cx.stop_propagation();
            }))
            .on_mouse_down_out(
                cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                    let mapping = &this.pii.mapping;
                    if [&mapping.bounds, &mapping.menu_bounds]
                        .iter()
                        .any(|b| b.get().is_some_and(|b| b.contains(&event.position)))
                    {
                        return;
                    }
                    this.close_pii_popup(&PiiClosePopup, window, cx);
                }),
            )
            // ☑ Replace · original → alias · category ▾ · scope · ‹ n of N › ·
            // × in one row; the original truncates first, and only a narrow
            // view wraps.
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .when(narrow, |v| v.flex_wrap().gap_y_1())
                    .items_center()
                    .gap_x(px(6.))
                    .child(if cancel {
                        // A just-made selection is backed out, not kept (ADR 0032).
                        secondary(
                            self.popup_control("direct-cancel-addition", "Cancel", enabled, cx),
                            theme,
                        )
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "direct-cancel-addition".into())
                        })
                        .aria_label("Cancel addition")
                        .on_click(
                            cx.listener(|this, _, window, cx| this.cancel_addition(window, cx)),
                        )
                        .into_any_element()
                    } else {
                        // Ticked: replaced. Unticking keeps the originals in
                        // scope; ticking a kept mention proposes it again.
                        let (scoped_candidates, scoped_applied) = self.scoped_mentions();
                        let in_scope = if kept {
                            self.scoped_kept(mapping.scope).len()
                        } else {
                            scoped_candidates.len() + scoped_applied.len()
                        };
                        let (fill, mark) = theme.checkbox();
                        self.popup_control("direct-keep", "", enabled && in_scope > 0, cx)
                            .when(cfg!(test), |v| v.debug_selector(|| "direct-keep".into()))
                            .role(gpui::Role::CheckBox)
                            .aria_toggled(if kept {
                                gpui::Toggled::False
                            } else {
                                gpui::Toggled::True
                            })
                            .aria_label(if kept {
                                format!("Replace · tick to propose {in_scope} again")
                            } else {
                                format!("Replace · untick to keep {in_scope} originals")
                            })
                            .flex()
                            .items_center()
                            .gap(px(5.))
                            .px_1()
                            .child(
                                div()
                                    .size(px(14.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(4.))
                                    .when(!kept, |v| {
                                        v.bg(fill).child(
                                            gpui::svg()
                                                .data(crate::ui::Icon::Check.data())
                                                .size(px(10.))
                                                .text_color(mark),
                                        )
                                    })
                                    .when(kept, |v| {
                                        v.border_1().border_color(palette.header_muted)
                                    }),
                            )
                            .child("Replace")
                            .tooltip(crate::style::tooltip(
                                if kept {
                                    "Tick to replace again (Space)".to_string()
                                } else {
                                    format!(
                                        "Untick to keep the original (Space, {})",
                                        scope_keys(mapping.scope, "⌫")
                                    )
                                },
                                theme,
                            ))
                            .on_click(
                                cx.listener(|this, _, window, cx| this.toggle_replace(window, cx)),
                            )
                            .into_any_element()
                    })
                    .child(div().w(px(1.)).h(px(16.)).bg(palette.border))
                    .child(
                        // The original → alias pair; the original gives way first.
                        self.with_dropdown(
                            div()
                                .min_w_0()
                                .flex()
                                .when(narrow, |v| v.flex_wrap().gap_y_1())
                                .items_center()
                                .gap_x(px(6.))
                                .child(
                                    div()
                                        .id("direct-original")
                                        .when(cfg!(test), |v| {
                                            v.debug_selector(|| "pseudonym-original".into())
                                        })
                                        .min_w_0()
                                        .max_w(px(120.))
                                        .truncate()
                                        .text_color(palette.header_muted)
                                        .child(original.to_string())
                                        .tooltip(crate::style::tooltip(
                                            original.to_string(),
                                            theme,
                                        )),
                                )
                                .child(
                                    div()
                                        .flex_shrink_0()
                                        .text_color(palette.header_muted)
                                        .child("→"),
                                )
                                .child(if identity.is_some() {
                                    self.alias_field(input_focused, accent, cx)
                                } else {
                                    div()
                                        .id("direct-alias-kept")
                                        .when(cfg!(test), |v| {
                                            v.debug_selector(|| "direct-alias-kept".into())
                                        })
                                        .min_w_0()
                                        .max_w(px(140.))
                                        .truncate()
                                        .text_color(palette.header_muted)
                                        .child(alias.clone())
                                        .tooltip(crate::style::tooltip(
                                            format!(
                                                "Kept as the original; tick Replace to use {alias}"
                                            ),
                                            theme,
                                        ))
                                        .into_any_element()
                                }),
                            identity.filter(|_| choices).map(|identity| {
                                self.alias_menu(
                                    identity,
                                    &draft,
                                    changed,
                                    selected_count,
                                    targets,
                                    enabled,
                                    cx,
                                )
                            }),
                        )
                        .flex_shrink(1.)
                        .min_w_0(),
                    )
                    .child(
                        self.with_dropdown(
                            self.popup_control(
                                "direct-category",
                                "",
                                enabled && identity.is_some(),
                                cx,
                            )
                            .flex()
                            .items_center()
                            .gap(px(5.))
                            .child(
                                div()
                                    .size(px(7.))
                                    .rounded_full()
                                    .bg(theme.category(category)),
                            )
                            .child(match (&owner, identity.is_some()) {
                                (Some(owner), _) => format!("{} · {owner} ▾", category.label()),
                                (None, true) => format!("{} ▾", category.label()),
                                (None, false) => category.label().to_string(),
                            })
                            .aria_label("Correct category for selected scope")
                            .when(identity.is_some(), |v| {
                                v.on_click(cx.listener(|this, _, _, cx| {
                                    this.pii.mapping.toggle_category_picker();
                                    cx.notify();
                                }))
                            })
                            .map(|v| match mapping.cue {
                                Some((generation, true)) => {
                                    let accent = theme.applied();
                                    v.with_animation(
                                        SharedString::from(format!("category-cue-{generation}")),
                                        Animation::new(Duration::from_millis(850))
                                            .with_easing(gpui::ease_in_out),
                                        move |v, delta| v.bg(cue_glow(delta, 150., accent)),
                                    )
                                    .into_any_element()
                                }
                                _ => v.into_any_element(),
                            }),
                            identity
                                .filter(|_| mapping.pickers.category)
                                .map(|identity| {
                                    self.category_menu(
                                        identity,
                                        selected_count,
                                        mentions.len(),
                                        ownable.then(|| owner.clone()),
                                        enabled,
                                        cx,
                                    )
                                })
                                .or_else(|| {
                                    identity
                                        .filter(|_| mapping.pickers.owner)
                                        .map(|identity| self.owner_menu(identity, id, enabled, cx))
                                }),
                        ),
                    )
                    .when(applied, |v| {
                        let accent = theme.applied();
                        v.child(
                            div()
                                .flex_shrink_0()
                                .when(cfg!(test), |v| v.debug_selector(|| "direct-applied".into()))
                                .px_1()
                                .rounded_sm()
                                .text_size(px(11.))
                                .text_color(accent)
                                .bg(Hsla { a: 0.16, ..accent })
                                .child("Applied"),
                        )
                    })
                    .when(mentions.len() > 1, |v| {
                        v.child(self.scope_control(mentions.len(), &alias, enabled, cx))
                    })
                    // ‹ n of N › and × follow the scope (ADR 0055).
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(2.))
                            .flex_shrink_0()
                            .when(mentions.len() > 1, |v| {
                                v.child(
                                    self.popup_control("direct-prev", "‹", true, cx)
                                        .aria_label("Previous mention")
                                        .tooltip(crate::style::tooltip(
                                            format!("Previous mention of {alias} (←)"),
                                            theme,
                                        ))
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.step_identity_occurrence(
                                                Step::Previous,
                                                window,
                                                cx,
                                            )
                                        })),
                                )
                                .child(
                                    crate::ui::live_text(
                                        "direct-position",
                                        crate::ui::position_announcement(
                                            "Mention",
                                            index + 1,
                                            mentions.len(),
                                        ),
                                    )
                                    .flex_shrink_0()
                                    .when(cfg!(test), |v| {
                                        v.debug_selector(|| "direct-position".into())
                                    })
                                    .text_size(px(11.))
                                    .text_color(palette.header_muted)
                                    .child(format!(
                                        "{}/{}",
                                        index + 1,
                                        mentions.len()
                                    )),
                                )
                                .child(
                                    self.popup_control("direct-next", "›", true, cx)
                                        .aria_label("Next mention")
                                        .tooltip(crate::style::tooltip(
                                            format!("Next mention of {alias} (→)"),
                                            theme,
                                        ))
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.step_identity_occurrence(Step::Next, window, cx)
                                        })),
                                )
                            })
                            .child(
                                self.popup_control("direct-close", "×", true, cx)
                                    .aria_label("Close replacement popup")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.close_pii_popup(&PiiClosePopup, window, cx)
                                    })),
                            ),
                    ),
            )
            .when(hidden, |v| {
                v.child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(11.))
                        .text_color(palette.header_muted)
                        .child("Hidden source value · containing passage shown in document"),
                )
            })
            .when_some(mapping.field_error.clone(), |v, error| {
                v.child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(12.))
                        .text_color(style::markdown_style(theme).alert_warning)
                        .child(error),
                )
            });
        // The editor records geometry during paint, after this popup is
        // rendered. Reconcile once on the next frame so Apply, resize and
        // stepping do not keep a stale anchor.
        let weak = cx.entity().downgrade();
        let measured = self.pii.popup_size.clone();
        panel = panel.relative().child(
            gpui::canvas(
                move |bounds, window, _| {
                    let resized = measured.replace(bounds.size) != bounds.size;
                    let weak = weak.clone();
                    window.on_next_frame(move |_, cx| {
                        let _ = weak.update(cx, |this, cx| {
                            let editor = this.editor.read(cx);
                            if this.active_annotation() == Some(annotation)
                                && (resized
                                    || editor.local_annotation_bounds(annotation, range.start)
                                        != Some(anchor))
                            {
                                cx.notify();
                            }
                        });
                    });
                },
                |_, _, _, _| {},
            )
            .absolute()
            .inset_0(),
        );
        Some(
            deferred(
                anchored()
                    .anchor(corner)
                    .position(gpui::point(x, y))
                    .position_mode(gpui::AnchoredPositionMode::Local)
                    // Kept inside the window, except when parked off it.
                    .when(!offscreen, |v| v.snap_to_window_with_margin(px(4.)))
                    .child(div().relative().child(panel).when(!offscreen, |v| {
                        // A notch on the edge facing the mention ties the card
                        // to its word across the cleared line (ADR 0055).
                        let at = anchor.left() + (anchor.size.width / 2.).min(px(14.)) - x;
                        let at = at.clamp(px(14.), (width - px(14.)).max(px(14.)));
                        v.child(notch(below, palette.bg, theme.popup_border(), at))
                    })),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }

    /// `anchor` with `menu` floating just below it, above the popup card.
    pub(super) fn with_dropdown(
        &self,
        anchor: impl IntoElement,
        menu: Option<AnyElement>,
    ) -> gpui::Div {
        div()
            .flex_shrink_0()
            .relative()
            .child(anchor)
            // A zero-size point at the anchor's bottom-left places the menu.
            .children(menu.map(|menu| {
                div().absolute().top_full().left_0().child(
                    deferred(anchored().snap_to_window().child(div().pt_1().child(menu)))
                        .with_priority(3),
                )
            }))
    }
    /// The surface shared by the popup's floating menus. A press inside it
    /// must not reach the popup's outside-click handler.
    pub(super) fn menu_surface(&self, id: &'static str) -> gpui::Stateful<gpui::Div> {
        crate::ui::panel(id, self.theme.get())
            .when(cfg!(test), move |v| v.debug_selector(move || id.into()))
            .w(px(300.))
            .p_1()
            .flex()
            .flex_col()
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            // Records where the menu floats, so a press on a part hanging
            // below the card is not an outside click.
            .child({
                let menu_bounds = self.pii.mapping.menu_bounds.clone();
                gpui::canvas(move |b, _, _| menu_bounds.set(Some(b)), |_, _, _, _| {})
                    .absolute()
                    .inset_0()
            })
    }
    /// A menu row: label left, a muted detail right. `checked` reserves a ✓
    /// column in menus that mark the current choice.
    pub(super) fn menu_row(
        &self,
        id: impl Into<gpui::ElementId>,
        label: impl Into<SharedString>,
        detail: impl Into<SharedString>,
        checked: Option<bool>,
        enabled: bool,
        cx: &App,
    ) -> gpui::Stateful<gpui::Div> {
        let palette = self.theme.get().pdf_style();
        let label = label.into();
        self.menu_control(id, enabled, cx)
            .aria_label(label.clone())
            .when_some(checked, |v, checked| {
                v.aria_toggled(if checked {
                    gpui::Toggled::True
                } else {
                    gpui::Toggled::False
                })
            })
            .w_full()
            .flex()
            .items_center()
            .gap_2()
            .when_some(checked, |v, checked| {
                v.child(
                    div()
                        .w(px(12.))
                        .flex_shrink_0()
                        .child(if checked { "✓" } else { "" }),
                )
            })
            .child(div().flex_1().min_w_0().text_ellipsis().child(label))
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(12.))
                    .text_color(palette.header_muted)
                    .child(detail.into()),
            )
    }
    fn menu_heading(&self, text: &'static str) -> gpui::Div {
        div()
            .px_2()
            .pt_1()
            .text_size(px(11.))
            .text_color(self.theme.get().pdf_style().header_muted)
            .child(text)
    }
    /// Category correction: each category with the alias it would give.
    fn category_menu(
        &self,
        identity: &mdoc_pii::Identity,
        selected_count: usize,
        mentions: usize,
        owner: Option<Option<String>>,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let review = &self.pii.review;
        let custom = identity.custom_alias;
        let palette = self.theme.get().pdf_style();
        self.menu_surface("direct-category-menu")
            .children(Category::ALL.into_iter().map(|category| {
                let current = identity.category == category;
                let alias = if (custom || current) && selected_count == mentions {
                    identity.alias.clone()
                } else {
                    review.next_alias(category)
                };
                self.menu_row(
                    SharedString::from(format!("direct-category-{}", category.token())),
                    category.label(),
                    alias,
                    Some(current),
                    enabled,
                    cx,
                )
                .on_click(cx.listener(move |this, _, _, cx| this.correct_category(category, cx)))
            }))
            // Ownership opens in the menu's place (ADR 0055).
            .when_some(owner, |v, owner| {
                v.child(div().h(px(1.)).my_1().bg(palette.border)).child(
                    self.menu_row(
                        "direct-owner",
                        "Belongs to…",
                        owner.unwrap_or_else(|| "nobody".into()),
                        None,
                        enabled,
                        cx,
                    )
                    .aria_label("Belongs to")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.pii.mapping.toggle_owner_picker();
                        cx.notify();
                    })),
                )
            })
            .into_any_element()
    }
    /// The editable alias: a boxed field, accent-bordered while editing.
    /// The alias in its mention's colour (the category's while proposed, the
    /// applied blue once applied); its field shows on hover and focus.
    fn alias_field(&self, focused: bool, accent: Hsla, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let mapping = &self.pii.mapping;
        // Sized to the alias, with room for the pencil (ADR 0055).
        let chars = mapping.alias.read(cx).value().chars().count() as f32;
        let width = px((chars * 8.2 + 22.).clamp(60., 220.));
        div()
            .id("direct-alias")
            .when(cfg!(test), |v| v.debug_selector(|| "direct-alias".into()))
            .w(width)
            .flex_shrink_0()
            .relative()
            .group("direct-alias")
            .cursor(gpui::CursorStyle::IBeam)
            .px(px(6.))
            .py(px(2.))
            .rounded_md()
            .border_1()
            .border_color(if focused {
                accent
            } else {
                gpui::transparent_black()
            })
            .when(focused, |v| v.bg(palette.placeholder_bg))
            .text_color(accent)
            .font_weight(gpui::FontWeight::MEDIUM)
            .hover(move |s| s.border_color(palette.border))
            .child(mapping.alias.clone())
            // A pencil on hover marks the alias as editable.
            .child(
                div()
                    .absolute()
                    .right(px(6.))
                    .top_0()
                    .bottom_0()
                    .flex()
                    .items_center()
                    .text_size(px(11.))
                    .text_color(palette.header_muted)
                    .opacity(0.)
                    .group_hover("direct-alias", |s| s.opacity(1.))
                    .child("✎"),
            )
            .map(|v| {
                if mapping.target_index.is_none() {
                    crate::ui::reveal_focus(
                        v,
                        mapping.alias.read(cx).focus_handle(cx),
                        self.pii.popup_scroll.clone(),
                    )
                } else {
                    v
                }
            })
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    // A visible caret shows the alias is editable. A press on
                    // the text already placed it; one on the padding puts it
                    // at the end.
                    this.pii.mapping.show_alias_choices();
                    let alias = this.pii.mapping.alias.clone();
                    let focus = alias.read(cx).focus_handle(cx);
                    if !focus.is_focused(window) {
                        window.focus(&focus, cx);
                        alias.update(cx, |input, cx| input.move_to_end(cx));
                    }
                    // Keep the card's own focus-on-press from taking it back.
                    window.prevent_default();
                    cx.notify();
                }),
            )
            .map(|v| match mapping.cue {
                Some((generation, _)) => {
                    let accent = theme.applied();
                    v.with_animation(
                        SharedString::from(format!("alias-cue-{generation}")),
                        Animation::new(Duration::from_millis(850)).with_easing(gpui::ease_in_out),
                        move |v, delta| v.bg(cue_glow(delta, 0., accent)),
                    )
                    .into_any_element()
                }
                None => v.into_any_element(),
            })
    }
    /// The alias combobox: rename in scope, link to an existing entity, or
    /// split off with a new alias.
    #[allow(clippy::too_many_arguments)]
    fn alias_menu(
        &self,
        identity: &mdoc_pii::Identity,
        draft: &str,
        changed: bool,
        selected_count: usize,
        targets: Arc<[popup::AliasTarget]>,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let new_alias = self.pii.review.next_alias(identity.category);
        let count = targets.len();
        self.menu_surface("direct-alias-menu")
            .when(changed && !draft.is_empty(), |v| {
                v.child(
                    self.menu_row(
                        "direct-use-alias",
                        format!("Rename to \"{draft}\""),
                        format!("{selected_count} mentions"),
                        None,
                        enabled,
                        cx,
                    )
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.confirm_alias(false, Applying::Nothing, cx);
                        window.focus(&this.pii.focus, cx);
                    })),
                )
            })
            .when(count > 0, |v| {
                v.child(self.menu_heading("Same as…")).child(
                    uniform_list(
                        "direct-alias-targets",
                        count,
                        cx.processor(move |this, indices: Range<usize>, _, cx| {
                            indices
                                .map(|index| {
                                    let popup::AliasTarget {
                                        id: target,
                                        alias,
                                        original,
                                        category,
                                        ..
                                    } = targets[index].clone();
                                    let palette = this.theme.get().pdf_style();
                                    this.menu_control(
                                        SharedString::from(format!("direct-target-{target}")),
                                        !this.pii.scanning(),
                                        cx,
                                    )
                                    .when(cfg!(test), |v| {
                                        v.debug_selector(move || format!("target-{target}"))
                                    })
                                    .aria_label(format!(
                                        "Use {alias}, {original}, {}",
                                        category.label()
                                    ))
                                    .w_full()
                                    .h(px(30.))
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .when(this.pii.mapping.target_index == Some(index), |v| {
                                        v.bg(palette.placeholder_bg)
                                    })
                                    .child(
                                        div()
                                            .flex_shrink_0()
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .child(alias),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .text_ellipsis()
                                            .text_size(px(12.))
                                            .text_color(palette.header_muted)
                                            .child(original),
                                    )
                                    .on_click(cx.listener(
                                        move |this, _, window, cx| {
                                            this.link_to_entity(target, cx);
                                            window.focus(&this.pii.focus, cx);
                                        },
                                    ))
                                })
                                .collect()
                        }),
                    )
                    .h(px((count as f32 * 30.).clamp(0., 180.)))
                    .flex_shrink_0()
                    .track_scroll(&self.pii.mapping.target_scroll),
                )
            })
            .child(
                self.menu_row(
                    "direct-new-alias",
                    "New alias",
                    new_alias,
                    None,
                    enabled,
                    cx,
                )
                .on_click(cx.listener(|this, _, _, cx| this.separate_with_new_alias(cx))),
            )
            .into_any_element()
    }
    /// `Belongs to PERSON_1 ▾` for contacts and identifiers, or whenever an
    /// owner is set (ADR 0032).
    /// The owner picker, opened from the category menu's "Belongs to…".
    fn owner_menu(
        &self,
        identity: &mdoc_pii::Identity,
        id: u64,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mapping = &self.pii.mapping;
        let query = mapping.target.read(cx).value().trim().to_lowercase();
        let owners: Arc<[_]> = self.alias_targets(&query, true).into();
        let count = owners.len();
        self.menu_surface("direct-owner-menu")
            .child(self.menu_heading("Belongs to"))
            .child(div().px_1().pb_1().child(mapping.target.clone()))
            .child(
                self.menu_row(
                    "direct-owner-none",
                    "Nobody",
                    "",
                    Some(identity.owner.is_none()),
                    enabled,
                    cx,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.change_mapping(MappingAction::Owner(id, None), cx)
                })),
            )
            .child(
                uniform_list(
                    "direct-owners",
                    count,
                    cx.processor(move |this, indices: Range<usize>, _, cx| {
                        indices
                            .map(|index| {
                                let popup::AliasTarget {
                                    id: target_id,
                                    alias,
                                    original,
                                    ..
                                } = owners[index].clone();
                                let current = this
                                    .pii
                                    .review
                                    .identity(id)
                                    .is_some_and(|i| i.owner == Some(target_id));
                                this.menu_row(
                                    SharedString::from(format!("direct-owner-{target_id}")),
                                    alias,
                                    original,
                                    Some(current),
                                    !this.pii.scanning(),
                                    cx,
                                )
                                .h(px(30.))
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.change_mapping(
                                            MappingAction::Owner(id, Some(target_id)),
                                            cx,
                                        )
                                    },
                                ))
                            })
                            .collect()
                    }),
                )
                .h(px((count as f32 * 30.).clamp(0., 150.)))
                .flex_shrink_0(),
            )
            .into_any_element()
    }
    /// `( This one | All N )`: the popup's one scope, and the only place its
    /// count appears. All, the whole entity, is the default (ADR 0055).
    fn scope_control(
        &self,
        mentions: usize,
        alias: &str,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let selected = self.pii.mapping.scope;
        div().flex_shrink_0().flex().items_center().child(
            div().flex().min_w_0().gap(px(2.)).children(
                [Scope::Mention, Scope::Entity].into_iter().map(|scope| {
                    let (label, count, tip) = match scope {
                        Scope::Mention => ("This one", None, "Only this mention".to_string()),
                        _ => (
                            "All",
                            Some(mentions),
                            format!("All {mentions} mentions of {alias}, in every spelling"),
                        ),
                    };
                    let on = selected == scope;
                    self.popup_control(
                        SharedString::from(format!("scope-{}", scope as u8)),
                        "",
                        enabled,
                        cx,
                    )
                    // The label and count are drawn as children; name the
                    // segment for screen readers and report the chosen one.
                    .aria_label(match count {
                        Some(count) => format!("All {count} mentions"),
                        None => label.to_string(),
                    })
                    .aria_toggled(if on {
                        gpui::Toggled::True
                    } else {
                        gpui::Toggled::False
                    })
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .py(px(1.))
                    .rounded_sm()
                    .text_size(px(12.))
                    .whitespace_nowrap()
                    .child(label)
                    .children(count.map(|count| {
                        div()
                            .text_size(px(11.))
                            .text_color(palette.header_muted)
                            .child(count.to_string())
                    }))
                    .tooltip(crate::style::tooltip(tip, theme))
                    .when(on, |v| {
                        v.bg(palette.placeholder_bg)
                            .text_color(palette.header_fg)
                            .font_weight(gpui::FontWeight::MEDIUM)
                    })
                    .when(!on, |v| v.text_color(palette.header_muted))
                    // Hovering previews the segment's scope outline.
                    .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                        let mapping = &mut this.pii.mapping;
                        if *hovered {
                            mapping.scope_hover = Some(scope);
                        } else if mapping.scope_hover == Some(scope) {
                            mapping.scope_hover = None;
                        }
                        cx.notify();
                    }))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.pii.mapping.set_scope(scope);
                        cx.notify();
                    }))
                }),
            ),
        )
    }
}

/// The shortcut for `key` in `scope`: ⌘ widens to the same text, ⇧⌘ to the
/// whole entity (ADR 0033).
pub(super) fn scope_keys(scope: Scope, key: &str) -> String {
    let modifier = if cfg!(target_os = "macos") {
        "⌘"
    } else {
        "Ctrl+"
    };
    match scope {
        Scope::Mention => key.to_string(),
        Scope::Wording => format!("{modifier}{key}"),
        Scope::Entity => format!("⇧{modifier}{key}"),
    }
}

/// The context shown around a mention: about 30 characters before it and 45
/// after. A cut that would split a word moves to the nearest space inside
/// that span instead, so a snippet never starts or ends mid-word.
pub(super) fn snippet_bounds(source: &str, range: &Range<usize>) -> (usize, usize) {
    // Inside an unclosed HTML tag such as `<span style=…>`: the cleanup
    // can't remove half a tag. Looks back at most 256 bytes.
    let in_tag = |at: usize| {
        let mut from = at.saturating_sub(256);
        while !source.is_char_boundary(from) {
            from -= 1;
        }
        let before = &source[from..at];
        before
            .rfind('<')
            .is_some_and(|open| before.rfind('>').is_none_or(|close| close < open))
    };
    // Cut only next to whitespace outside tags: contractions ("can't"),
    // hyphens, accents and Markdown markers ("**") stay with their word.
    let start_ok =
        |at: usize| at == 0 || (source[..at].ends_with(char::is_whitespace) && !in_tag(at));
    let end_ok = |at: usize| {
        at == source.len() || (source[at..].starts_with(char::is_whitespace) && !in_tag(at))
    };
    let mut start = source[..range.start]
        .char_indices()
        .rev()
        .nth(30)
        .map_or(0, |(i, _)| i);
    if !start_ok(start) {
        start = source[start..range.start]
            .char_indices()
            .map(|(i, c)| start + i + c.len_utf8())
            .find(|&at| start_ok(at))
            .map_or(range.start, |at| at.min(range.start));
    }
    let mut end = source[range.end..]
        .char_indices()
        .nth(45)
        .map_or(source.len(), |(i, _)| range.end + i);
    if !end_ok(end) {
        end = source[range.end..end]
            .char_indices()
            .rev()
            .map(|(i, _)| range.end + i)
            .find(|&at| end_ok(at))
            .unwrap_or(range.end);
    }
    (start, end)
}

#[cfg(test)]
mod snippet_tests {
    use super::snippet_bounds;

    fn snippet(source: &str, word: &str) -> String {
        let at = source.find(word).unwrap();
        let (start, end) = snippet_bounds(source, &(at..at + word.len()));
        source[start..end].to_owned()
    }

    /// Both ends of `text` sit at whitespace or the ends of `source`.
    fn assert_whole_words(source: &str, text: &str) {
        let at = source.find(text).unwrap();
        let end = at + text.len();
        assert!(
            at == 0 || source[..at].ends_with(char::is_whitespace),
            "{text:?}"
        );
        assert!(
            end == source.len() || source[end..].starts_with(char::is_whitespace),
            "{text:?}"
        );
    }

    #[test]
    fn snippets_cut_at_word_boundaries() {
        let source = "Agreement between Maya Bennett, known to colleagues as Mia, and the \
                      Employer, signed in Springfield on the first day of the month.";
        let text = snippet(source, "Mia");
        // 30 characters back would start at "ennett"; the snippet starts at
        // the next word instead.
        assert_eq!(
            text,
            "known to colleagues as Mia, and the Employer, signed in Springfield on"
        );
        assert_whole_words(source, &text);
    }

    #[test]
    fn short_text_and_unbroken_runs_keep_the_mention() {
        assert_eq!(snippet("Call Mia today.", "Mia"), "Call Mia today.");
        // A run without spaces is dropped rather than shown cut.
        let run = "x".repeat(80);
        let source = format!("{run}Mia{run}");
        assert_eq!(snippet(&source, "Mia"), "Mia");
        // A decomposed accent is part of its word, not a break in it.
        // 30 characters back from "Mia" falls just after the accent.
        let source = "The cafe\u{301}teria was visited yesterday by Mia.";
        assert_eq!(snippet(source, "Mia"), "was visited yesterday by Mia.");
        // Apostrophes and Markdown markers belong to their word.
        let source = "The client can't disclose personal details to Mia.";
        assert_whole_words(source, &snippet(source, "Mia"));
        let source = "The cafe**teria** was visited today by Mia.";
        let text = snippet(source, "Mia");
        assert!(
            !text.starts_with("teria") && !text.starts_with("**"),
            "{text}"
        );
        assert_whole_words(source, &text);
        // A space inside an HTML tag is not a place to cut.
        let source =
            "Mia visited the cafe<span style=\"color:red\">teria</span> yesterday and signed.";
        let text = snippet(source, "Mia");
        assert!(!text.contains("<span"), "{text}");
        assert_whole_words(source, &text);
        // Cyrillic, multi-byte boundaries.
        let source = "Договор между Павловой Мариной Сергеевной, далее Заказчик, и Исполнителем по настоящему договору.";
        let text = snippet(source, "Заказчик");
        assert!(text.contains("Заказчик"));
        assert_whole_words(source, &text);
    }
}
