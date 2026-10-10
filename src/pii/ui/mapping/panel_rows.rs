//! Replacements panel rows: group headers and mention rows (ADR 0033).
use super::panel::{PanelEntry, PanelItem, ReplacementRow};
use super::*;
use crate::ui;
use gpui::{AnyElement, HighlightStyle, SharedString, StyledText, div, prelude::*};

/// Starts a panel drag: remembers what is dragged and shows its ghost chip.
fn start_drag(
    weak: gpui::WeakEntity<DocumentView>,
    theme: crate::style::Theme,
) -> impl Fn(&PanelDrag, gpui::Point<Pixels>, &mut Window, &mut App) -> Entity<DragGhost> + 'static
{
    move |drag, _, _, cx| {
        let _ = weak.update(cx, |this, _| {
            this.pii.mapping.dragging = Some(drag.clone());
            this.pii.mapping.drop_target = None;
            this.pii.mapping.drag_in_panel = false;
        });
        let label = match drag {
            PanelDrag::Mention { original, .. } | PanelDrag::Entity { original, .. } => {
                original.clone()
            }
        };
        cx.new(|_| DragGhost { label, theme })
    }
}

/// Panel row surfaces: a band for open groups, a stronger fill for the
/// selected mention, and a guide line tying mentions to their header.
struct RowColors {
    hover: Hsla,
    band: Hsla,
    selected: Hsla,
    guide: Hsla,
    guide_selected: Hsla,
    separator: Hsla,
}
impl RowColors {
    fn new(theme: crate::style::Theme) -> Self {
        let p = theme.pdf_style();
        Self {
            hover: Hsla {
                a: 0.35,
                ..p.placeholder_bg
            },
            band: Hsla {
                a: 0.55,
                ..p.placeholder_bg
            },
            selected: p.placeholder_bg,
            guide: Hsla {
                a: 0.45,
                ..p.header_muted
            },
            guide_selected: p.header_fg,
            separator: p.border,
        }
    }
}

/// The vertical line left of a group's mention rows.
fn guide(color: Hsla) -> gpui::Div {
    div()
        .absolute()
        .left(px(17.))
        .top_0()
        .bottom_0()
        .w(px(2.))
        .bg(color)
}

impl DocumentView {
    pub(super) fn panel_row(
        &self,
        index: usize,
        (row, mention): PanelEntry,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = row.id;
        let muted = self.theme.get().pdf_style().header_muted;
        let hover = RowColors::new(self.theme.get()).hover;
        // Flat, full-width rows: the list cursor drives the keyboard.
        let base = div()
            .id(SharedString::from(format!("replacement-entry-{index}")))
            .role(gpui::Role::Button)
            .cursor_pointer()
            .hover(move |s| s.bg(hover))
            .w_full()
            .h(px(48.))
            .line_height(px(17.))
            .overflow_hidden()
            .relative()
            .flex_col()
            .items_start()
            .justify_center()
            .gap_1()
            // Every row of a group accepts drops for that group.
            .drag_over::<PanelDrag>(move |s, drag, _, _| {
                if drag.identity() == id {
                    s
                } else {
                    s.border_1().border_color(muted)
                }
            })
            .on_drag_move::<PanelDrag>(cx.listener(
                move |this, e: &gpui::DragMoveEvent<PanelDrag>, _, cx| {
                    if e.bounds.contains(&e.event.position) {
                        this.drag_over_target(DropTarget::Entity(id), cx);
                    }
                },
            ))
            .on_drop(
                cx.listener(move |this, drag: &PanelDrag, _, cx| this.drop_on_entity(drag, id, cx)),
            );
        match mention {
            PanelItem::Mention(annotation, range) => {
                self.mention_row(base, &row, annotation, range, cx)
            }
            PanelItem::Header => self.header_row(base, &row, cx),
            PanelItem::Controls(annotation) => self.inline_controls(index, annotation, cx),
        }
    }

    /// A group header: original, category, alias and count. A click toggles
    /// only this group; the alias and category edit the whole entity.
    fn header_row(
        &self,
        base: gpui::Stateful<gpui::Div>,
        row: &ReplacementRow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = row.id;
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let mapping = &self.pii.mapping;
        let expanded = mapping.expanded.contains(&id);
        // "Link to ALIAS" while another group's row is dragged here.
        let link_hint = (cx.has_active_drag() || mapping.key_moving)
            && mapping.drop_target == Some(DropTarget::Entity(id))
            && mapping
                .dragging
                .as_ref()
                .is_some_and(|d| d.identity() != id);
        let menu = (mapping.panel_menu == Some(PanelMenu::Category(id)))
            .then(|| self.entity_category_menu(id, row.category, cx));
        let category = div()
            .id(SharedString::from(format!("header-category-{id}")))
            .when(cfg!(test), |v| {
                v.debug_selector(move || format!("header-category-{id}"))
            })
            .flex_shrink_0()
            .px_1()
            .rounded_sm()
            .text_size(px(11.))
            .text_color(palette.header_muted)
            .cursor_pointer()
            .hover(move |s| s.bg(palette.placeholder_bg))
            .child(format!("{} ▾", row.category.label()))
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                let menu = &mut this.pii.mapping.panel_menu;
                *menu = (*menu != Some(PanelMenu::Category(id))).then_some(PanelMenu::Category(id));
                cx.notify();
            }));
        let alias = if mapping.header_edit == Some(id) {
            div()
                .id(SharedString::from(format!("header-alias-edit-{id}")))
                .flex_1()
                .min_w_0()
                .text_color(theme.applied())
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(|_, _, cx| cx.stop_propagation())
                .on_key_down(
                    cx.listener(move |this, e: &gpui::KeyDownEvent, window, cx| {
                        match e.keystroke.key.as_str() {
                            "enter" => this.commit_header_alias(id, window, cx),
                            "escape" => this.cancel_header_alias(window, cx),
                            _ => return,
                        }
                        cx.stop_propagation();
                    }),
                )
                .child(mapping.header_alias.clone())
                .into_any_element()
        } else {
            let alias = row.alias.clone();
            div()
                .id(SharedString::from(format!("header-alias-{id}")))
                .when(cfg!(test), |v| {
                    v.debug_selector(move || format!("header-alias-{id}"))
                })
                .min_w_0()
                .flex_shrink(1.)
                .text_ellipsis()
                .text_color(theme.applied())
                .cursor(gpui::CursorStyle::IBeam)
                .child(format!("→ {}", row.alias))
                .on_click(cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    this.edit_header_alias(id, alias.clone(), window, cx);
                }))
                .into_any_element()
        };
        let count = match row.applied {
            _ if link_hint => format!("Link to {}", row.alias),
            0 => format!("{} · proposed", row.mentions),
            n if n == row.mentions => format!("{n} · applied"),
            n => format!("{n} of {} applied", row.mentions),
        };
        base.aria_label(format!(
            "{} to {}, {} mentions, {}",
            row.original,
            row.alias,
            row.mentions,
            if expanded { "expanded" } else { "collapsed" }
        ))
        .when(cfg!(test), |v| {
            v.debug_selector(move || format!("identity-{id}"))
        })
        .px_3()
        .border_t_1()
        .border_color(RowColors::new(theme).separator)
        .when(expanded, |v| v.bg(RowColors::new(theme).band))
        // The keyboard row is outlined, so it never reads as selected.
        .when(mapping.cursor == Some(PanelCursor::Header(id)), |v| {
            v.border_1().border_color(palette.header_muted)
        })
        // The keyboard move's target group (ADR 0033).
        .when(mapping.key_moving && link_hint, |v| {
            v.border_1().border_color(palette.header_muted)
        })
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .flex_shrink_0()
                        .w(px(10.))
                        .text_size(px(10.))
                        .text_color(palette.header_muted)
                        .child(if expanded { "▾" } else { "▸" }),
                )
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .text_ellipsis()
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .child(row.original.to_string()),
                )
                .child(self.with_dropdown(category, menu)),
        )
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .gap_2()
                .pl(px(14.))
                .child(alias)
                .child(div().flex_1())
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(11.))
                        .text_color(palette.header_muted)
                        .child(count),
                ),
        )
        .on_click(cx.listener(move |this, _, window, cx| {
            window.focus(&this.pii.mapping.focus, cx);
            this.pii.mapping.cursor = Some(PanelCursor::Header(id));
            this.pii.mapping.triage_expanded.remove(&id);
            this.toggle_group(id, cx);
        }))
        .on_drag(
            PanelDrag::Entity {
                identity: id,
                original: row.original.clone(),
            },
            start_drag(cx.entity().downgrade(), theme),
        )
        .into_any_element()
    }

    /// One mention under an expanded header: indented, with its snippet.
    fn mention_row(
        &self,
        base: gpui::Stateful<gpui::Div>,
        row: &ReplacementRow,
        annotation: u64,
        range: Range<usize>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let applied = annotation & APPLIED_ID != 0;
        let current = self.active_annotation() == Some(annotation)
            && (self.pii.popup.is_some() || self.pii.mapping.panel_selected);
        let original = if applied {
            self.pii
                .review
                .applied_occurrence(annotation & !APPLIED_ID)
                .unwrap()
                .step
                .original_shared()
                .clone()
        } else {
            let candidate = self.pii.review.candidate(annotation).unwrap();
            self.pii
                .review
                .variant(candidate.variant)
                .unwrap()
                .original
                .clone()
        };
        let (before, word, after) = self.readable_mention(&range, cx);
        let undo = applied && (current || self.pii.mapping.hovered == Some(annotation));
        let state = if applied {
            theme.applied()
        } else {
            theme.proposed()
        };
        base.aria_label(format!(
            "{original}: {before}{word}{after}{}",
            if applied { ", applied" } else { "" }
        ))
        .when(cfg!(test), |v| {
            v.debug_selector(move || format!("mention-{annotation}"))
        })
        .pl(px(28.))
        .pr_3()
        .when(current, |v| v.bg(RowColors::new(theme).selected))
        .child(guide(if current {
            RowColors::new(theme).guide_selected
        } else {
            RowColors::new(theme).guide
        }))
        .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
            if this.pii.mapping.hover_mention(annotation, *hovered) {
                cx.notify();
            }
        }))
        .when(undo, |v| v.pr(px(36.)))
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .gap_1()
                .text_size(px(11.))
                .text_color(palette.header_muted)
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .text_ellipsis()
                        .child(original.to_string()),
                )
                .when(applied, |v| v.child(div().flex_shrink_0().child("applied"))),
        )
        .when(undo, |v| {
            let id = annotation & !APPLIED_ID;
            v.child(
                ui::icon_button(
                    SharedString::from(format!("mention-undo-{annotation}")),
                    "Undo this replacement (back to proposed)",
                    ui::Icon::Undo,
                    theme,
                    !self.pii.scanning(),
                )
                .when(cfg!(test), |v| {
                    v.debug_selector(move || format!("mention-undo-{annotation}"))
                })
                .absolute()
                .right(px(4.))
                .top(px(8.))
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.undo_replacements([id].into(), cx);
                })),
            )
        })
        .child(div().w_full().text_ellipsis().child(
            StyledText::new(format!("{before}{word}{after}")).with_highlights([(
                before.len()..before.len() + word.len(),
                HighlightStyle {
                    color: Some(state),
                    ..Default::default()
                },
            )]),
        ))
        .on_click(cx.listener(move |this, _, window, cx| {
            window.focus(&this.pii.mapping.focus, cx);
            this.panel_select_mention(annotation, cx);
        }))
        // Middle click keeps this original; ⌘ same text, ⇧⌘ all (ADR 0033).
        .on_mouse_down(
            gpui::MouseButton::Middle,
            cx.listener(move |this, e: &gpui::MouseDownEvent, window, cx| {
                cx.stop_propagation();
                this.middle_click_mention(annotation, e.modifiers, true, window, cx);
            }),
        )
        .on_drag(
            PanelDrag::Mention {
                identity: row.id,
                range: range.clone(),
                original,
            },
            start_drag(cx.entity().downgrade(), theme),
        )
        .into_any_element()
    }

    /// One line under the panel-selected mention: Apply to · Apply · Keep
    /// (Undo for an applied mention). Buttons follow the scope selector; the
    /// keys use fixed scopes (ADR 0033).
    fn inline_controls(&self, index: usize, annotation: u64, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let applied = annotation & APPLIED_ID != 0;
        let enabled = !self.pii.scanning();
        let scope = self.pii.mapping.scope;
        let mentions = self
            .selected_entity()
            .map_or(0, |id| self.identity_occurrences(id).len());
        let colors = RowColors::new(theme);
        let accent = theme.applied();
        let raised = Hsla {
            a: 0.12,
            ..palette.header_fg
        };
        let segment = |id: &'static str,
                       label: &'static str,
                       count: Option<usize>,
                       value: Scope,
                       cx: &mut Context<Self>| {
            div()
                .id(id)
                .when(cfg!(test), move |v| v.debug_selector(move || id.into()))
                .flex()
                .items_center()
                .gap_1()
                .px_2()
                .py(px(2.))
                .rounded_sm()
                .cursor_pointer()
                .text_color(if scope == value {
                    palette.header_fg
                } else {
                    palette.header_muted
                })
                .when(scope == value, |v| v.bg(raised))
                .child(label)
                .when_some(count, |v, n| {
                    v.child(div().text_size(px(11.)).opacity(0.7).child(n.to_string()))
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.pii.mapping.set_scope(value);
                    this.sync_scope_outlines(cx);
                    cx.notify();
                }))
        };
        let action = |id: &'static str,
                      label: &'static str,
                      key: &'static str,
                      keep: bool,
                      cx: &mut Context<Self>| {
            let hint = super::popup_render::scope_keys(scope, key);
            div()
                .id(id)
                .when(cfg!(test), move |v| v.debug_selector(move || id.into()))
                .flex()
                .items_center()
                .gap_1()
                .px_2()
                .py(px(2.))
                .rounded_md()
                .border_1()
                .cursor_pointer()
                .map(|v| {
                    if keep {
                        // Secondary: outlined, neutral.
                        v.border_color(palette.header_muted)
                            .text_color(palette.header_fg)
                            .hover(move |s| s.bg(raised))
                    } else {
                        // Primary, like the popup's Apply.
                        v.border_color(Hsla { a: 0.6, ..accent })
                            .bg(Hsla { a: 0.18, ..accent })
                            .text_color(accent)
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .hover(move |s| s.bg(Hsla { a: 0.28, ..accent }))
                    }
                })
                .when(!enabled, |v| v.opacity(0.5))
                .child(label)
                .child(div().text_size(px(11.)).opacity(0.7).child(hint))
                .on_click(cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    if enabled {
                        let scope = this.pii.mapping.scope;
                        this.panel_decide(scope, keep, window, cx);
                    }
                }))
        };
        div()
            .id(SharedString::from(format!("replacement-entry-{index}")))
            .when(cfg!(test), |v| {
                v.debug_selector(|| "inline-controls".into())
            })
            .w_full()
            .h(px(48.))
            .pl(px(28.))
            .pr_3()
            .flex()
            .items_center()
            .gap_1()
            .relative()
            .text_size(px(12.))
            // Continues the selected mention's surface and guide.
            .bg(colors.selected)
            .child(guide(colors.guide_selected))
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_shrink_0()
                    .p(px(2.))
                    .rounded_md()
                    .border_1()
                    .border_color(Hsla {
                        a: 0.5,
                        ..palette.header_muted
                    })
                    .child(segment(
                        "inline-scope-this",
                        "This",
                        None,
                        Scope::Mention,
                        cx,
                    ))
                    .child(segment(
                        "inline-scope-same",
                        "Same",
                        None,
                        Scope::Wording,
                        cx,
                    ))
                    .child(segment(
                        "inline-scope-all",
                        "All",
                        Some(mentions),
                        Scope::Entity,
                        cx,
                    )),
            )
            .child(div().flex_1())
            .map(|v| {
                if applied {
                    v.child(action("inline-undo", "Undo", "⌫", true, cx))
                } else {
                    v.child(action("inline-apply", "Apply", "↵", false, cx))
                        .child(action("inline-keep", "Keep", "⌫", true, cx))
                }
            })
            .into_any_element()
    }

    fn entity_category_menu(
        &self,
        id: u64,
        current: Category,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let enabled = !self.pii.scanning();
        self.menu_surface("panel-category-menu")
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.pii.mapping.panel_menu = None;
                cx.notify();
            }))
            .children(Category::ALL.into_iter().map(|category| {
                self.menu_row(
                    SharedString::from(format!("panel-category-{}", category.token())),
                    category.label(),
                    "",
                    Some(category == current),
                    enabled,
                    cx,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.pii.mapping.panel_menu = None;
                    if category != current {
                        this.change_mapping(MappingAction::Category(id, category), cx);
                    }
                    cx.notify();
                }))
            }))
            .into_any_element()
    }

    /// Expand or collapse one group; nothing is selected (ADR 0033).
    pub(super) fn toggle_group(&mut self, id: u64, cx: &mut Context<Self>) {
        let expanded = &mut self.pii.mapping.expanded;
        if !expanded.remove(&id) {
            expanded.insert(id);
        }
        cx.notify();
    }

    fn edit_header_alias(
        &mut self,
        id: u64,
        alias: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = self.pii.mapping.header_alias.clone();
        input.update(cx, |input, cx| input.set_value(alias, cx));
        self.pii.mapping.header_edit = Some(id);
        window.focus(&input.read(cx).focus_handle(cx), cx);
        cx.notify();
    }

    /// Rename the whole entity from its header; a rejected alias keeps editing.
    fn commit_header_alias(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let value = self
            .pii
            .mapping
            .header_alias
            .read(cx)
            .value()
            .trim()
            .to_owned();
        if self
            .pii
            .review
            .identity(id)
            .is_some_and(|i| i.alias != value)
        {
            self.change_mapping(MappingAction::Rename(id, value), cx);
            if self.pii.error.is_some() {
                cx.notify();
                return;
            }
        }
        self.cancel_header_alias(window, cx);
    }

    fn cancel_header_alias(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pii.mapping.header_edit = None;
        window.focus(&self.pii.mapping.focus, cx);
        cx.notify();
    }
}
