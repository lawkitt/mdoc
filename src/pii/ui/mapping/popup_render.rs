//! Word-anchored editor for the shared replacement workspace.
use super::*;
use gpui::{
    Animation, AnimationExt, AnyElement, SharedString, anchored, deferred, div, prelude::*,
    uniform_list,
};
use std::time::Duration;

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
        let start = source[..range.start]
            .char_indices()
            .rev()
            .nth(30)
            .map_or(0, |(i, _)| i);
        let end = source[range.end..]
            .char_indices()
            .nth(45)
            .map_or(source.len(), |(i, _)| range.end + i);
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
    fn menu_control(
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
    pub(in crate::pii::ui) fn replacement_popup(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let annotation = self.active_annotation()?;
        let id = self.selected_entity()?;
        let review = &self.pii.review;
        let identity = review.identity(id)?;
        let mapping = &self.pii.mapping;
        let original = self.active_original()?;
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let enabled = !self.pii.scanning();
        // Cancel addition while this mention's addition is the latest step.
        let cancel = mapping
            .added_at(self.editor.read(cx).history_id())
            .is_some_and(|(added, _)| added.original == original);
        let applied = annotation & APPLIED_ID != 0;
        self.active_replacement_range()?;
        let mentions = self.identity_occurrences(id);
        let index = mentions
            .iter()
            .position(|(a, _)| *a == annotation)
            .unwrap_or(0);
        let wording = self.scoped_ranges(Scope::Wording).len();
        let selected_count = self.scoped_ranges(mapping.scope).len();
        let draft = mapping.alias.read(cx).value().trim().to_string();
        let changed = draft != identity.alias;
        let input_focused = mapping.alias.read(cx).focus_handle(cx).is_focused(window);
        let choices = mapping.pickers.alias || input_focused;
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
        let owner_row = self.owner_row(identity, id, enabled, cx);
        let available = self.scroll.bounds();
        let anchor = self.editor.read(cx).annotation_bounds(annotation);
        if anchor.is_some() {
            self.pii.popup_anchor.set(anchor);
        }
        let hidden = self.editor.read(cx).annotation_is_hidden(annotation);
        let width = px(360.).min((available.size.width - px(16.)).max(px(160.)));
        let maximum = (available.size.height - px(44.)).max(px(110.));
        // Pickers float over the card, so its height no longer depends on them.
        let desired = px(280.).min(maximum);
        let bounds = anchor
            .or(self.pii.popup_anchor.get())
            .unwrap_or(gpui::Bounds::new(
                available.origin + gpui::point(px(8.), px(8.)),
                gpui::size(px(1.), px(18.)),
            ));
        let below = (available.bottom() - bounds.bottom() - px(8.)).max(px(0.));
        let above = (bounds.top() - available.top() - px(8.)).max(px(0.));
        let height = desired.min(below.max(above).max(px(100.)));
        let y = if below >= height || below >= above {
            bounds.bottom() + px(4.)
        } else {
            bounds.top() - height - px(4.)
        };
        let position = gpui::point(
            bounds.left().clamp(
                available.left() + px(4.),
                (available.right() - width - px(4.)).max(available.left() + px(4.)),
            ),
            y.max(available.top()),
        );
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
            .w(width)
            .max_h(height)
            .overflow_y_scroll()
            .track_scroll(&self.pii.popup_scroll)
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .text_size(px(13.))
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
            .on_action(cx.listener(|this, _: &crate::PiiNextChoice, _, cx| {
                this.step_alias_choice(false, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &crate::PiiPreviousChoice, _, cx| {
                this.step_alias_choice(true, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &crate::PiiOpenChoice, window, cx| {
                this.open_alias_choice(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &PiiClosePopup, window, cx| {
                this.escape_popup(window, cx);
                cx.stop_propagation();
            }))
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
                        .is_some_and(|a| a & APPLIED_ID == 0)
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
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        self.with_dropdown(
                            self.popup_control(
                                "direct-category",
                                format!("{} ▾", identity.category.label()),
                                enabled,
                                cx,
                            )
                            .aria_label("Correct category for selected scope")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.pii.mapping.toggle_category_picker();
                                cx.notify();
                            }))
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
                            mapping.pickers.category.then(|| {
                                self.category_menu(
                                    identity,
                                    selected_count,
                                    mentions.len(),
                                    enabled,
                                    cx,
                                )
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
                    .child(div().flex_1())
                    // One mention needs no navigation (ADR 0032).
                    .when(mentions.len() > 1, |v| {
                        v.child(
                            self.popup_control("direct-prev", "‹", true, cx)
                                .aria_label("Previous mention")
                                .tooltip(crate::style::tooltip(
                                    format!("Previous mention of {}", identity.alias),
                                    theme,
                                ))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.step_identity_occurrence(true, window, cx)
                                })),
                        )
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_size(px(11.))
                                .text_color(palette.header_muted)
                                .child(format!("{} of {}", index + 1, mentions.len())),
                        )
                        .child(
                            self.popup_control("direct-next", "›", true, cx)
                                .aria_label("Next mention")
                                .tooltip(crate::style::tooltip(
                                    format!("Next mention of {}", identity.alias),
                                    theme,
                                ))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.step_identity_occurrence(false, window, cx)
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
            // original → [alias]: the arrow states the relationship.
            .child(
                self.with_dropdown(
                    div()
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .id("direct-original")
                                .when(cfg!(test), |v| {
                                    v.debug_selector(|| "pseudonym-original".into())
                                })
                                .min_w_0()
                                .max_w(gpui::relative(0.45))
                                .flex_shrink(1.)
                                .line_clamp(2)
                                .text_color(palette.header_muted)
                                .child(original.to_string())
                                .tooltip(crate::style::tooltip(original.to_string(), theme)),
                        )
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_color(palette.header_muted)
                                .child("→"),
                        )
                        .child(self.alias_field(input_focused, cx)),
                    choices.then(|| {
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
                ),
            )
            .children(owner_row)
            .when(mentions.len() > 1, |v| {
                v.child(self.scope_control(
                    wording,
                    mentions.len(),
                    &original,
                    &identity.alias,
                    enabled,
                    cx,
                ))
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
        let (scoped_candidates, scoped_applied) = self.scoped_mentions();
        let in_scope = scoped_candidates.len() + scoped_applied.len();
        let accent = theme.applied();
        panel = panel.child(
            div()
                .flex_shrink_0()
                .pt_1()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_2()
                .map(|v| {
                    if applied {
                        let ids: std::collections::HashSet<_> =
                            scoped_applied.iter().copied().collect();
                        let count = ids.len();
                        v.child(
                            secondary(
                                self.popup_control("direct-undo", "", enabled && count > 0, cx),
                                theme,
                            )
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(
                                gpui::svg()
                                    .data(crate::ui::Icon::Undo.data())
                                    .size(px(14.))
                                    .text_color(theme.pdf_style().header_fg),
                            )
                            .child("Undo")
                            .when(cfg!(test), |v| v.debug_selector(|| "direct-undo".into()))
                            .aria_label(format!("Undo replacement · {count} (back to proposed)"))
                            .on_click(cx.listener(
                                move |this, _, _, cx| this.undo_replacements(ids.clone(), cx),
                            )),
                        )
                    } else {
                        let count = scoped_candidates.len();
                        let ready = enabled && count > 0;
                        v.child(
                            self.popup_control("direct-apply", "", ready, cx)
                                .px_3()
                                .py_1()
                                .rounded_md()
                                .border_1()
                                .border_color(Hsla { a: 0.6, ..accent })
                                .bg(Hsla { a: 0.18, ..accent })
                                .text_color(accent)
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .flex()
                                .items_center()
                                .gap_2()
                                .child("Apply")
                                // Enter applies only after a manual addition (ADR 0024).
                                .when(self.pii.enter_applies && ready, |v| {
                                    v.child(div().text_size(px(11.)).opacity(0.7).child("↵"))
                                })
                                .aria_label(format!("Apply {count} replacements in scope"))
                                .on_click(cx.listener(|this, _, _, cx| this.apply_scope(cx))),
                        )
                    }
                })
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
                    .on_click(cx.listener(|this, _, window, cx| this.cancel_addition(window, cx)))
                } else {
                    secondary(
                        self.popup_control(
                            "direct-keep",
                            "Keep original",
                            enabled && in_scope > 0,
                            cx,
                        ),
                        theme,
                    )
                    .aria_label(format!("Keep {in_scope} originals in scope"))
                    .on_click(cx.listener(|this, _, window, cx| this.keep_originals(window, cx)))
                }),
        );
        // The editor records word geometry during paint, after this popup is rendered.
        // Reconcile once on the next frame so Apply and resize do not retain a stale anchor.
        let weak = cx.entity().downgrade();
        panel = panel.relative().child(
            gpui::canvas(
                move |_, window, _| {
                    let weak = weak.clone();
                    window.on_next_frame(move |_, cx| {
                        let _ = weak.update(cx, |this, cx| {
                            if this.active_annotation() == Some(annotation)
                                && this.editor.read(cx).annotation_bounds(annotation) != anchor
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
            deferred(anchored().position(position).snap_to_window().child(panel))
                .with_priority(2)
                .into_any_element(),
        )
    }

    /// `anchor` with `menu` floating just below it, above the popup card.
    fn with_dropdown(&self, anchor: impl IntoElement, menu: Option<AnyElement>) -> gpui::Div {
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
    fn menu_surface(&self, id: &'static str) -> gpui::Stateful<gpui::Div> {
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
    fn menu_row(
        &self,
        id: impl Into<gpui::ElementId>,
        label: impl Into<SharedString>,
        detail: impl Into<SharedString>,
        checked: Option<bool>,
        enabled: bool,
        cx: &App,
    ) -> gpui::Stateful<gpui::Div> {
        let palette = self.theme.get().pdf_style();
        self.menu_control(id, enabled, cx)
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
            .child(div().flex_1().min_w_0().text_ellipsis().child(label.into()))
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
        identity: &pii::Identity,
        selected_count: usize,
        mentions: usize,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let review = &self.pii.review;
        let custom = identity.custom_alias;
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
            .into_any_element()
    }
    /// The editable alias: a boxed field, accent-bordered while editing.
    fn alias_field(&self, focused: bool, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let mapping = &self.pii.mapping;
        div()
            .id("direct-alias")
            .when(cfg!(test), |v| v.debug_selector(|| "direct-alias".into()))
            .flex_1()
            .min_w(px(96.))
            .relative()
            .group("direct-alias")
            .cursor(gpui::CursorStyle::IBeam)
            .px_2()
            .py(px(3.))
            .rounded_md()
            .border_1()
            .border_color(if focused {
                theme.applied()
            } else {
                palette.border
            })
            .bg(palette.placeholder_bg)
            .text_color(theme.applied())
            .font_weight(gpui::FontWeight::MEDIUM)
            .hover(move |s| s.border_color(palette.header_muted))
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
        identity: &pii::Identity,
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
    fn owner_row(
        &self,
        identity: &pii::Identity,
        id: u64,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Div> {
        let review = &self.pii.review;
        let owner = identity.owner.and_then(|owner| review.identity(owner));
        if owner.is_none()
            && matches!(
                identity.category,
                Category::Person | Category::Organization | Category::Date | Category::Other
            )
        {
            return None;
        }
        let mapping = &self.pii.mapping;
        let palette = self.theme.get().pdf_style();
        let label = owner.map_or("nobody", |owner| owner.alias.as_str());
        let menu = mapping.pickers.owner.then(|| {
            let query = mapping.target.read(cx).value().trim().to_lowercase();
            let owners: Arc<[_]> = self.alias_targets(&query, true).into();
            let count = owners.len();
            self.menu_surface("direct-owner-menu")
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
        });
        Some(
            div()
                .flex_shrink_0()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(palette.header_muted)
                        .child("Belongs to"),
                )
                .child(
                    self.with_dropdown(
                        self.popup_control("direct-owner", format!("{label} ▾"), enabled, cx)
                            .text_size(px(12.))
                            .aria_label(format!("Belongs to {label}"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.pii.mapping.toggle_owner_picker();
                                cx.notify();
                            })),
                        menu,
                    ),
                ),
        )
    }
    /// `Apply to ( This one | All N )`: the popup's one scope, and the only
    /// place counts appear (ADR 0032). Labels stay short so three segments fit
    /// the popup; tooltips spell each scope out.
    #[allow(clippy::too_many_arguments)]
    fn scope_control(
        &self,
        wording: usize,
        mentions: usize,
        original: &str,
        alias: &str,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let selected = self.pii.mapping.scope;
        let mut scopes = vec![Scope::Mention];
        if wording > 1 {
            scopes.push(Scope::Wording);
        }
        if mentions > wording {
            scopes.push(Scope::Entity);
        }
        let three = scopes.len() == 3;
        div()
            .flex_shrink_0()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_x_2()
            .gap_y_1()
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(palette.header_muted)
                    .child("Apply to"),
            )
            .child(
                div()
                    .flex()
                    .min_w_0()
                    .p(px(2.))
                    .gap(px(2.))
                    .rounded_md()
                    .border_1()
                    .border_color(palette.border)
                    .children(scopes.into_iter().map(|scope| {
                        let (label, count, tip) = match scope {
                            Scope::Mention => ("This one", None, "Only this mention".to_string()),
                            Scope::Wording if three => (
                                "Same text",
                                Some(wording),
                                format!("Every mention written “{original}”"),
                            ),
                            Scope::Wording => (
                                "All",
                                Some(wording),
                                format!("All {wording} mentions of {alias}"),
                            ),
                            Scope::Entity => (
                                "All",
                                Some(mentions),
                                format!("All {mentions} mentions of {alias}, in every spelling"),
                            ),
                        };
                        // A one-mention wording is the same as this mention.
                        let on = selected == scope
                            || (scope == Scope::Mention
                                && selected == Scope::Wording
                                && wording <= 1);
                        self.popup_control(
                            SharedString::from(format!("scope-{}", scope as u8)),
                            "",
                            enabled,
                            cx,
                        )
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
                    })),
            )
    }
}
