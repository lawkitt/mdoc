//! Replacements panel rows: group headers and mention rows.
use super::{
    DropTarget, PanelCursor, PanelDrag, PanelMenu,
    checklist::Tick,
    panel::{PanelEntry, PanelItem, ROW_HEIGHT, ReplacementRow},
};
use crate::review::text;
use crate::ui::MenuPanel;
use crate::{document::DocumentView, review::APPLIED_ID, ui, ui::style};
use gpui::{
    AnyElement, App, AppContext, Context, Entity, HighlightStyle, Hsla, Pixels, SharedString,
    StyledText, Window, div, prelude::*, px,
};
use mdoc_pii::Category;
use std::{ops::Range, sync::Arc};

/// The drag ghost: the dragged original as a compact chip.
pub(super) struct DragGhost {
    pub label: Arc<str>,
    pub theme: crate::ui::style::Theme,
}
impl gpui::Render for DragGhost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl gpui::IntoElement {
        use gpui::{ParentElement, Styled};
        let palette = self.theme.pdf_style();
        gpui::div()
            .px_2()
            .py_1()
            .rounded_md()
            .bg(palette.bg)
            .border_1()
            .border_color(self.theme.applied())
            .text_size(px(12.))
            .text_color(palette.header_fg)
            .shadow_md()
            .child(self.label.to_string())
    }
}
/// Starts a panel drag: remembers what is dragged and shows its ghost chip.
fn start_drag(
    weak: gpui::WeakEntity<DocumentView>,
    theme: crate::ui::style::Theme,
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
/// selected mention, and a guide line tying mentions to their header. The
/// guide is blue only for applied mentions, dimmed unless selected.
struct RowColors {
    hover: Hsla,
    selected: Hsla,
    guide: Hsla,
    guide_selected: Hsla,
    guide_applied: Hsla,
    guide_applied_selected: Hsla,
}
impl RowColors {
    fn new(theme: crate::ui::style::Theme) -> Self {
        let p = theme.pdf_style();
        Self {
            hover: Hsla {
                a: 0.35,
                ..p.placeholder_bg
            },
            selected: p.placeholder_bg,
            guide: Hsla {
                a: 0.45,
                ..p.header_muted
            },
            guide_selected: p.header_fg,
            guide_applied: Hsla {
                a: 0.45,
                ..theme.applied()
            },
            guide_applied_selected: theme.applied(),
        }
    }
    fn guide_for(&self, applied: bool, selected: bool) -> Hsla {
        match (applied, selected) {
            (true, true) => self.guide_applied_selected,
            (true, false) => self.guide_applied,
            (false, true) => self.guide_selected,
            (false, false) => self.guide,
        }
    }
}

/// The header text's left edge; the guide sits on it and mention content
/// starts [`MENTION_INSET`] after (one indent level).
const ROW_INSET: f32 = 12.;
pub(super) const MENTION_INSET: f32 = ROW_INSET + 22.;
/// An entity row's mention count fits four digits; a longer one widens the
/// cell rather than squeezing the chevron beside it (#308).
pub(super) const COUNT_WIDTH: f32 = 28.;
pub(super) const CHEVRON_SIZE: f32 = 12.;

/// The vertical line left of a group's mention rows.
fn guide(color: Hsla) -> gpui::Div {
    div()
        .absolute()
        .left(px(ROW_INSET))
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
        let selected = matches!(mention, PanelItem::Mention(annotation, _)
            if self.mention_selected(annotation));
        // Flat, full-width rows: the list cursor drives the keyboard.
        let base = div()
            .id(SharedString::from(format!("replacement-entry-{index}")))
            .role(gpui::Role::Button)
            .cursor_pointer()
            .when(!selected, |v| v.hover(move |s| s.bg(hover)))
            .w_full()
            .h(px(ROW_HEIGHT))
            .overflow_hidden()
            .relative()
            .flex()
            .items_center()
            .gap_2()
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
            PanelItem::Category { entities, mentions } => {
                self.category_row(index, row.category, entities, mentions)
            }
            PanelItem::Mention(annotation, range) => {
                self.mention_row(base, &row, annotation, range, cx)
            }
            PanelItem::Header => self.header_row(base, &row, cx),
            PanelItem::Kept(range) => self.kept_row(base, &row, range, cx),
            PanelItem::Hint(hint) => self.hint_row(index, hint, cx),
        }
    }

    /// A category label over its entities, "● Person 3 (7)": entities, then
    /// mentions. Inert: no hover, focus or drop.
    fn category_row(
        &self,
        index: usize,
        category: Category,
        entities: usize,
        mentions: usize,
    ) -> AnyElement {
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        div()
            .id(SharedString::from(format!("replacement-entry-{index}")))
            .when(cfg!(test), |v| {
                v.debug_selector(move || format!("replacement-category-{}", category.token()))
            })
            .role(gpui::Role::Heading)
            .aria_label(text::category_heading(category.label(), entities, mentions))
            .w_full()
            .h(px(ROW_HEIGHT))
            .px(px(ROW_INSET))
            .pb(px(6.))
            .flex()
            .items_end()
            .gap(px(6.))
            .text_size(px(11.))
            .text_color(palette.header_muted)
            .child(
                div()
                    .mb(px(4.))
                    .size(px(7.))
                    .flex_shrink_0()
                    .rounded_full()
                    .bg(theme.category(category)),
            )
            .child(
                div()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(palette.header_fg)
                    .child(category.label().to_uppercase()),
            )
            .child(format!("{entities} ({mentions})"))
            .into_any_element()
    }

    /// A checkbox: ticked will be replaced, unticked keeps the original,
    /// dashed means some mentions are kept. `name` says what it
    /// replaces, so a screen reader tells the boxes apart.
    fn tick_box(
        &self,
        id: impl Into<gpui::ElementId>,
        name: String,
        tick: Tick,
        cx: &mut Context<Self>,
        toggle: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> gpui::Stateful<gpui::Div> {
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let filled = tick != Tick::Off;
        let (fill, mark) = theme.checkbox();
        div()
            .id(id)
            .role(gpui::Role::CheckBox)
            .aria_label(name)
            .aria_toggled(match tick {
                Tick::On => gpui::Toggled::True,
                Tick::Off => gpui::Toggled::False,
                Tick::Mixed => gpui::Toggled::Mixed,
            })
            .flex_shrink_0()
            .size(px(15.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(4.))
            .border_1()
            .border_color(if filled {
                fill
            } else {
                Hsla {
                    a: 0.7,
                    ..palette.header_muted
                }
            })
            .when(filled, |v| v.bg(fill))
            .when(tick == Tick::On, |v| {
                v.child(
                    gpui::svg()
                        .data(ui::Icon::Check.data())
                        .size(px(11.))
                        .text_color(mark),
                )
            })
            .when(tick == Tick::Mixed, |v| {
                v.child(div().w(px(7.)).h(px(1.5)).bg(mark))
            })
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                window.focus(&this.pii.mapping.focus, cx);
                toggle(this, cx);
            }))
    }

    /// An entity row: checkbox, original, alias. A click jumps to its first
    /// mention; the chevron shows its mentions; the alias edits in place.
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
        let tick = row.tick();
        let kept_only = tick == Tick::Off;
        // "Link to ALIAS" while another group's row is dragged here.
        let link_hint = (cx.has_active_drag() || mapping.key_moving)
            && mapping.drop_target == Some(DropTarget::Entity(id))
            && mapping
                .dragging
                .as_ref()
                .is_some_and(|d| d.identity() != id);
        let menu = (mapping.panel_menu == Some(PanelMenu::Category(id)))
            .then(|| self.entity_category_menu(id, row.category, cx));
        // Right-click changes the entity's category; the menu hangs here.
        let category = div()
            .id(SharedString::from(format!("header-category-{id}")))
            .when(cfg!(test), |v| {
                v.debug_selector(move || format!("header-category-{id}"))
            })
            .w(px(0.))
            .h_full();
        let alias = if mapping.header_edit == Some(id) {
            div()
                .id(SharedString::from(format!("header-alias-edit-{id}")))
                .w(px(112.))
                .flex_shrink_0()
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
        } else if link_hint {
            div()
                .flex_shrink_0()
                .text_size(px(12.))
                .text_color(theme.applied())
                .child(text::link_to(&row.alias))
                .into_any_element()
        } else if kept_only {
            div()
                .flex_shrink_0()
                .text_size(px(12.))
                .text_color(palette.header_muted)
                .child(text::KEPT_TAG)
                .into_any_element()
        } else {
            let alias = row.alias.clone();
            let applied = row.applied > 0;
            div()
                .id(SharedString::from(format!("header-alias-{id}")))
                .when(cfg!(test), |v| {
                    v.debug_selector(move || format!("header-alias-{id}"))
                })
                .flex_shrink_0()
                .max_w(px(140.))
                .truncate()
                .px(px(5.))
                .rounded_sm()
                .font_family(style::markdown_style(theme).mono.family.clone())
                .text_size(px(11.))
                .text_color(if applied {
                    theme.applied()
                } else {
                    palette.header_muted
                })
                .when(applied, |v| {
                    v.bg(Hsla {
                        a: 0.14,
                        ..theme.applied()
                    })
                })
                .cursor(gpui::CursorStyle::IBeam)
                .child(row.alias.clone())
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    this.edit_header_alias(id, alias.clone(), window, cx);
                }))
                .into_any_element()
        };
        let total = row.mentions + row.kept;
        let chevron = div()
            .id(SharedString::from(format!("entity-expand-{id}")))
            .when(cfg!(test), |v| {
                v.debug_selector(move || format!("entity-expand-{id}"))
            })
            .aria_label(if expanded {
                text::HIDE_MENTIONS
            } else {
                text::SHOW_MENTIONS
            })
            .flex_shrink_0()
            .min_w(px(COUNT_WIDTH + 2. + CHEVRON_SIZE))
            .h(px(20.))
            .flex()
            .items_center()
            .justify_end()
            .gap(px(2.))
            .rounded_sm()
            .text_size(px(11.))
            .text_color(palette.header_muted)
            .hover(move |s| s.bg(palette.placeholder_bg))
            .when(total > 1, |v| {
                v.child(
                    div()
                        .when(cfg!(test), |v| {
                            v.debug_selector(move || format!("entity-count-{id}"))
                        })
                        .flex_shrink_0()
                        .min_w(px(COUNT_WIDTH))
                        .flex()
                        .justify_end()
                        .child(total.to_string()),
                )
            })
            .when(total > 0, |v| {
                v.child(
                    gpui::svg()
                        .when(cfg!(test), |v| {
                            v.debug_selector(move || format!("entity-chevron-{id}"))
                        })
                        .data(if expanded {
                            ui::Icon::ChevronDown.data()
                        } else {
                            ui::Icon::ChevronRight.data()
                        })
                        .flex_shrink_0()
                        .size(px(CHEVRON_SIZE))
                        .text_color(palette.header_muted),
                )
            })
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.toggle_group(id, cx);
            }));
        base.aria_label(text::identity_row(
            &row.original,
            &row.alias,
            row.mentions,
            row.applied,
            row.kept,
            expanded,
        ))
        .when(cfg!(test), |v| {
            v.debug_selector(move || format!("identity-{id}"))
        })
        .px(px(ROW_INSET))
        // The keyboard row is outlined, so it never reads as selected.
        .when(mapping.cursor == Some(PanelCursor::Header(id)), |v| {
            v.border_1().border_color(palette.header_muted).rounded_md()
        })
        .when(mapping.key_moving && link_hint, |v| {
            v.border_1().border_color(palette.header_muted)
        })
        .child(
            self.tick_box(
                SharedString::from(format!("entity-tick-{id}")),
                text::replace_with(&row.original, &row.alias),
                tick,
                cx,
                move |this, cx| this.toggle_entity(id, cx),
            )
            .when(cfg!(test), |v| {
                v.debug_selector(move || format!("entity-tick-{id}"))
            }),
        )
        .child(
            div()
                .when(cfg!(test), |v| {
                    v.debug_selector(move || format!("header-original-{id}"))
                })
                .min_w_0()
                .flex_1()
                .truncate()
                .when(kept_only, |v| v.text_color(palette.header_muted))
                .child(row.original.to_string()),
        )
        .child(self.with_dropdown(category, menu))
        .child(alias)
        .child(chevron)
        .on_mouse_down(
            gpui::MouseButton::Right,
            cx.listener(move |this, _, window, cx| this.toggle_category_menu(id, window, cx)),
        )
        // A click toggles the group: opening shows its mentions and the popup
        // on the first one; closing folds it away.
        .on_click(cx.listener(move |this, _, window, cx| this.toggle_identity_row(id, window, cx)))
        .on_drag(
            PanelDrag::Entity {
                identity: id,
                original: row.original.clone(),
            },
            start_drag(cx.entity().downgrade(), theme),
        )
        .into_any_element()
    }

    /// A kept mention under its entity: unticked, muted; ticking proposes it
    /// again, a click shows it in the text.
    fn kept_row(
        &self,
        base: gpui::Stateful<gpui::Div>,
        row: &ReplacementRow,
        range: Range<usize>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = row.id;
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let colors = RowColors::new(theme);
        let (before, word, after) = self.readable_mention(&range, cx);
        let start = range.start;
        let tick_range = range.clone();
        base.aria_label(text::original_kept(&word))
            .when(cfg!(test), |v| {
                v.debug_selector(move || format!("kept-{start}"))
            })
            .pl(px(MENTION_INSET))
            .pr_3()
            .text_size(px(12.))
            .text_color(palette.header_muted)
            .child(guide(colors.guide_for(false, false)))
            .child(
                self.tick_box(
                    SharedString::from(format!("kept-tick-{start}")),
                    text::replace_word(&word),
                    Tick::Off,
                    cx,
                    move |this, cx| this.retick_mention(id, tick_range.clone(), cx),
                )
                .when(cfg!(test), |v| {
                    v.debug_selector(move || format!("kept-tick-{start}"))
                }),
            )
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .text_ellipsis()
                    .child(format!("{before}{word}{after}")),
            )
            .on_click(cx.listener(move |this, _, window, cx| this.open_kept_row(start, window, cx)))
            .into_any_element()
    }

    fn mention_selected(&self, annotation: u64) -> bool {
        self.active_annotation() == Some(annotation)
            && (self.pii.popup.is_some() || self.pii.mapping.panel_selected)
    }

    /// One mention under an expanded entity: indented, its checkbox and its
    /// snippet on one line.
    fn mention_row(
        &self,
        base: gpui::Stateful<gpui::Div>,
        row: &ReplacementRow,
        annotation: u64,
        range: Range<usize>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme.get();
        let applied = annotation & APPLIED_ID != 0;
        let current = self.mention_selected(annotation);
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
        let colors = RowColors::new(theme);
        let state = if applied {
            theme.applied()
        } else {
            theme.category(row.category)
        };
        base.aria_label(format!(
            "{original}: {before}{word}{after}{}",
            if applied { text::COMMA_APPLIED } else { "" }
        ))
        .when(cfg!(test), |v| {
            v.debug_selector(move || format!("mention-{annotation}"))
        })
        .pl(px(MENTION_INSET))
        .pr_3()
        .text_size(px(12.))
        .when(current, |v| v.bg(colors.selected))
        .child(guide(colors.guide_for(applied, current)))
        .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
            if this.pii.mapping.hover_mention(annotation, *hovered) {
                cx.notify();
            }
        }))
        .child(
            self.tick_box(
                SharedString::from(format!("mention-tick-{annotation}")),
                text::replace_word(&word),
                Tick::On,
                cx,
                move |this, cx| this.toggle_mention(annotation, cx),
            )
            .when(cfg!(test), |v| {
                v.debug_selector(move || format!("mention-tick-{annotation}"))
            }),
        )
        .child(div().min_w_0().flex_1().text_ellipsis().child(
            StyledText::new(format!("{before}{word}{after}")).with_highlights([(
                before.len()..before.len() + word.len(),
                HighlightStyle {
                    color: Some(state),
                    font_weight: Some(gpui::FontWeight::MEDIUM),
                    ..Default::default()
                },
            )]),
        ))
        // A mention row opens the popup on that mention.
        .on_click(cx.listener(move |this, _, window, cx| {
            this.open_from_panel(annotation, window, cx);
        }))
        // Middle click keeps this original; ⌘ same text, ⇧⌘ all.
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

    fn entity_category_menu(
        &self,
        id: u64,
        current: Category,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let enabled = !self.pii.scanning();
        self.menu_surface("panel-category-menu")
            .menu_dismiss(cx, |this, _, _, cx| {
                this.pii.mapping.panel_menu = None;
                cx.notify();
            })
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
                    this.pick_identity_category(id, category, current, cx)
                }))
            }))
            // Not a name for the whole identity, last.
            .child(
                div()
                    .h(px(1.))
                    .my_1()
                    .bg(self.theme.get().pdf_style().border),
            )
            .child(
                self.menu_row("panel-not-a-name", text::NOT_A_NAME, "", None, enabled, cx)
                    .aria_label(text::NOT_A_NAME)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.pii.mapping.panel_menu = None;
                        this.not_a_name_identity(id, cx);
                    })),
            )
            .into_any_element()
    }
}
