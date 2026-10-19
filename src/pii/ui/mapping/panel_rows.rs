//! Replacements panel rows: group headers and mention rows (ADR 0033).
use super::checklist::Tick;
use super::panel::{PanelEntry, PanelItem, ROW_HEIGHT, ReplacementRow};
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
/// selected mention, and a guide line tying mentions to their header. The
/// guide is blue only for applied mentions, dimmed unless selected (ADR 0044).
struct RowColors {
    hover: Hsla,
    selected: Hsla,
    guide: Hsla,
    guide_selected: Hsla,
    guide_applied: Hsla,
    guide_applied_selected: Hsla,
}
impl RowColors {
    fn new(theme: crate::style::Theme) -> Self {
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
/// starts [`MENTION_INSET`] after (one indent level, ADR 0044).
const ROW_INSET: f32 = 12.;
const MENTION_INSET: f32 = ROW_INSET + 22.;

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
        }
    }

    /// A category label over its entities, "● Person 3 (7)": entities, then
    /// mentions. Inert: no hover, focus or drop (ADR 0046, ADR 0048).
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
            .aria_label(format!(
                "{}, {entities} {}, {mentions} {}",
                category.label(),
                if entities == 1 { "name" } else { "names" },
                if mentions == 1 { "mention" } else { "mentions" },
            ))
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
    /// dashed means some mentions are kept (ADR 0048). `name` says what it
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
                .child(format!("Link to {}", row.alias))
                .into_any_element()
        } else if kept_only {
            div()
                .flex_shrink_0()
                .text_size(px(12.))
                .text_color(palette.header_muted)
                .child("kept")
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
                "Hide mentions"
            } else {
                "Show mentions"
            })
            .flex_shrink_0()
            .w(px(28.))
            .h(px(20.))
            .flex()
            .items_center()
            .justify_end()
            .gap(px(2.))
            .rounded_sm()
            .text_size(px(11.))
            .text_color(palette.header_muted)
            .hover(move |s| s.bg(palette.placeholder_bg))
            .when(total > 1, |v| v.child(total.to_string()))
            .when(total > 0, |v| {
                v.child(
                    gpui::svg()
                        .data(if expanded {
                            ui::Icon::ChevronDown.data()
                        } else {
                            ui::Icon::ChevronRight.data()
                        })
                        .size(px(12.))
                        .text_color(palette.header_muted),
                )
            })
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.toggle_group(id, cx);
            }));
        base.aria_label(format!(
            "{} to {}, {} mentions, {} applied, {} kept, {}",
            row.original,
            row.alias,
            row.mentions,
            row.applied,
            row.kept,
            if expanded { "expanded" } else { "collapsed" }
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
                format!("Replace {} with {}", row.original, row.alias),
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
            cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                let menu = &mut this.pii.mapping.panel_menu;
                *menu = (*menu != Some(PanelMenu::Category(id))).then_some(PanelMenu::Category(id));
                cx.notify();
            }),
        )
        // A click toggles the group: opening shows its mentions and the popup
        // on the first one; closing folds it away (ADR 0048).
        .on_click(cx.listener(move |this, _, window, cx| {
            this.pii.mapping.cursor = Some(PanelCursor::Header(id));
            if this.pii.mapping.expanded.remove(&id) {
                this.pii.dismiss_popup();
                this.pii.mapping.panel_selected = false;
                window.focus(&this.pii.mapping.focus, cx);
                cx.notify();
                return;
            }
            this.pii.mapping.expanded.insert(id);
            match this.identity_occurrences(id).into_iter().next() {
                Some((annotation, _)) => {
                    this.open_from_panel(annotation, window, cx);
                    this.pii.mapping.cursor = Some(PanelCursor::Header(id));
                }
                None => window.focus(&this.pii.mapping.focus, cx),
            }
            cx.notify();
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

    /// Reveal a mention from the panel and open its popup there (ADR 0048):
    /// the panel's own scroll lands it in view, then the popup anchors to it.
    fn open_from_panel(&mut self, annotation: u64, window: &mut Window, cx: &mut Context<Self>) {
        // A narrow window showing the source switches back to the Markdown.
        self.original_selected = false;
        self.panel_select_mention(annotation, cx);
        self.activate_annotation(annotation, window, cx);
    }

    /// A kept mention under its entity: unticked, muted; ticking proposes it
    /// again, a click shows it in the text (ADR 0048).
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
        base.aria_label(format!("{word}: original kept"))
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
                    format!("Replace {word}"),
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
            .on_click(cx.listener(move |this, _, window, cx| {
                // Like a kept marker in the text: the popup opens on it, unticked.
                this.original_selected = false;
                this.reveal_replacement_offset(start, cx);
                this.activate_annotation(KEPT_ID | start as u64, window, cx);
                cx.notify();
            }))
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
            if applied { ", applied" } else { "" }
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
                format!("Replace {word}"),
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
        // A mention row opens the popup on that mention (ADR 0048).
        .on_click(cx.listener(move |this, _, window, cx| {
            this.open_from_panel(annotation, window, cx);
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
