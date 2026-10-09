//! Searchable replacement overview with inline occurrence context.
use super::*;
use crate::ui;
use gpui::{
    AnyElement, HighlightStyle, SharedString, StyledText, anchored, deferred, div, prelude::*,
    uniform_list,
};

#[derive(Clone)]
struct ReplacementRow {
    id: u64,
    original: Arc<str>,
    alias: String,
    mentions: usize,
    applied: usize,
}
/// One panel line: an entity row, or one of the expanded entity's mentions.
type PanelEntry = (Arc<ReplacementRow>, Option<(u64, Range<usize>)>);

impl Workspace {
    fn replacement_control(
        &self,
        id: impl Into<gpui::ElementId>,
        label: impl Into<SharedString>,
        enabled: bool,
        cx: &App,
    ) -> gpui::Stateful<gpui::Div> {
        let id = id.into();
        let debug = id.to_string();
        let focus = self
            .pii
            .mapping
            .controls
            .borrow_mut()
            .entry(id.clone())
            .or_insert_with(|| cx.focus_handle().tab_stop(true))
            .clone();
        ui::reveal_focus(
            ui::control(id, label, self.theme.get(), enabled)
                .when(cfg!(test), |v| v.debug_selector(move || debug.clone()))
                .track_focus(&focus),
            focus,
            self.pii.mapping.scroll.clone(),
        )
    }

    fn replacement_rows(&self, cx: &App) -> Arc<[ReplacementRow]> {
        let review = &self.pii.review;
        let query = self
            .pii
            .mapping
            .search
            .read(cx)
            .value()
            .trim()
            .to_lowercase();
        let mut rows = std::collections::BTreeMap::<u64, ReplacementRow>::new();
        let mut matches = std::collections::HashSet::new();
        let mut add = |id, original: Arc<str>, applied: bool| {
            let Some(identity) = review.identity(id) else {
                return;
            };
            if query.is_empty()
                || original.to_lowercase().contains(&query)
                || identity.alias.to_lowercase().contains(&query)
            {
                matches.insert(id);
            }
            let row = rows.entry(id).or_insert_with(|| ReplacementRow {
                id,
                original,
                alias: identity.alias.clone(),
                mentions: 0,
                applied: 0,
            });
            row.mentions += 1;
            row.applied += applied as usize;
        };
        for c in review.candidates() {
            if let (Some(id), Some(group)) = (
                review.occurrence_identity(c.variant, &c.range),
                review.variant(c.variant),
            ) {
                add(id, group.original.clone(), false);
            }
        }
        for a in review.applied() {
            add(a.step.identity, a.step.original_shared().clone(), true);
        }
        rows.into_values()
            .filter(|row| matches.contains(&row.id))
            .collect::<Vec<_>>()
            .into()
    }

    /// Entity rows in order, each followed by its mentions while it is expanded.
    fn replacement_entries(&self, cx: &App) -> Vec<PanelEntry> {
        let selected = self.selected_entity();
        let mut entries = Vec::new();
        for row in self.replacement_rows(cx).iter() {
            let row = Arc::new(row.clone());
            entries.push((row.clone(), None));
            if selected == Some(row.id) {
                for mention in self.identity_occurrences(row.id) {
                    entries.push((row.clone(), Some(mention)));
                }
            }
        }
        entries
    }

    /// Keep the active mention's panel row in view, e.g. after ‹ › navigation.
    pub(super) fn reveal_active_row(&self, cx: &App) {
        let Some(annotation) = self.active_annotation() else {
            return;
        };
        if let Some(index) = self
            .replacement_entries(cx)
            .iter()
            .position(|(_, mention)| mention.as_ref().is_some_and(|(a, _)| *a == annotation))
        {
            self.pii
                .mapping
                .list_scroll
                .scroll_to_item(index, gpui::ScrollStrategy::Nearest);
        }
    }

    fn replacement_commands(&self, cx: &mut Context<Self>) -> AnyElement {
        let busy = self.pii.scanning();
        let selected = self
            .preferences
            .borrow()
            .snapshot()
            .ok()
            .map(|p| p.pseudonymization.model);
        let needs_setup = selected.is_some_and(|m| {
            let index = settings::Model::ALL
                .iter()
                .position(|model| *model == settings::Model::Pii(m))
                .unwrap();
            !matches!(
                self.model_panel.read(cx).statuses[index],
                settings_ui::Status::Ready
            )
        });
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                self.replacement_control(
                    "review-rescan",
                    if busy { "Cancel scan" } else { "Rescan" },
                    true,
                    cx,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    if this.pii.scanning() {
                        this.pii.cancel();
                        this.pii.error = Some("Scan cancelled.".into());
                        cx.notify();
                    } else {
                        this.start_pii_scan(cx);
                    }
                })),
            )
            .child(
                self.replacement_control("review-settings", "Model settings…", true, cx)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.pii.mapping.close_actions();
                        this.model_panel
                            .update(cx, |panel, cx| panel.show(window, cx));
                    })),
            )
            .when(needs_setup, |v| {
                v.child(
                    self.replacement_control(
                        "review-download-model",
                        format!(
                            "Set up model ({} MB)",
                            selected.map(detector::download_megabytes).unwrap_or(0)
                        ),
                        !crate::model_work::busy(),
                        cx,
                    )
                    .when(cfg!(test), |v| {
                        v.debug_selector(|| "review-download-model".into())
                    })
                    .on_click(cx.listener(|this, _, window, cx| {
                        if !crate::model_work::busy() {
                            this.setup_pii_model(window, cx);
                        }
                    })),
                )
            })
            .text_size(px(12.))
            .into_any_element()
    }

    /// Collapse the open group: deselect it and close its popup.
    fn collapse_replacement(&mut self, cx: &mut Context<Self>) {
        self.pii.mapping.deselect();
        self.pii.dismiss_popup();
        self.editor
            .update(cx, |e, cx| e.set_active_annotation(None, cx));
        cx.notify();
    }
    /// A panel drop leaves the popup as it was: a mapping change otherwise
    /// reopens it on the previously active mention.
    fn after_drop(&mut self, had_popup: bool, cx: &mut Context<Self>) {
        if !had_popup && self.pii.popup.is_some() {
            self.pii.dismiss_popup();
            self.editor
                .update(cx, |e, cx| e.set_active_annotation(None, cx));
        }
    }
    /// Drop a dragged row on a group: link one mention, or merge an entity.
    pub(super) fn drop_on_entity(&mut self, drag: &PanelDrag, target: u64, cx: &mut Context<Self>) {
        self.pii.mapping.dragging = None;
        self.pii.mapping.drop_target = None;
        self.pii.mapping.drag_in_panel = false;
        let had_popup = self.pii.popup.is_some();
        match drag.clone() {
            PanelDrag::Mention {
                identity, range, ..
            } if identity != target => self.change_mapping(
                MappingAction::Scoped {
                    identity,
                    ranges: vec![range],
                    target: Some(target),
                    alias: None,
                    category: None,
                },
                cx,
            ),
            PanelDrag::Entity { identity, .. } if identity != target => {
                self.change_mapping(MappingAction::Merge(identity, target), cx)
            }
            _ => {}
        }
        self.after_drop(had_popup, cx);
        cx.notify();
    }
    /// Drop a dragged mention on "New entity": separate it with a new alias.
    pub(super) fn drop_as_new_entity(&mut self, drag: &PanelDrag, cx: &mut Context<Self>) {
        self.pii.mapping.dragging = None;
        self.pii.mapping.drop_target = None;
        self.pii.mapping.drag_in_panel = false;
        let had_popup = self.pii.popup.is_some();
        if let PanelDrag::Mention {
            identity, range, ..
        } = drag.clone()
            && self.pii.review.identity_count(identity) > 1
        {
            self.change_mapping(
                MappingAction::Scoped {
                    identity,
                    ranges: vec![range],
                    target: None,
                    alias: None,
                    category: None,
                },
                cx,
            );
        }
        self.after_drop(had_popup, cx);
        cx.notify();
    }
    /// Track the row under a drag so it can show "Link to ALIAS".
    fn drag_over_target(&mut self, target: DropTarget, cx: &mut Context<Self>) {
        if self.pii.mapping.drop_target != Some(target) {
            self.pii.mapping.drop_target = Some(target);
            cx.notify();
        }
    }

    fn undo_last_step(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.editor.read(cx).focus_handle(cx), cx);
        window.dispatch_action(Box::new(mdoc_editor::Undo), cx);
    }

    pub(crate) fn replacements_panel_width(&self, window: &Window) -> gpui::Pixels {
        if self.pii.mapping.open && self.can_copy_markdown() {
            px((self.chrome_width(window) * 0.4).clamp(260., 340.))
        } else {
            px(0.)
        }
    }
    pub(crate) fn replacements_panel(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.pii.mapping.open || !self.can_copy_markdown() {
            return None;
        }
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let mapping = &self.pii.mapping;
        let entries = Arc::new(self.replacement_entries(cx));
        let count = entries.len();
        let bounds = mapping.bounds.clone();
        let busy = self.pii.scanning();
        let pending = self.pii.review.remaining();
        let kept = mapping.kept_at(self.editor.read(cx).history_id());
        let added = mapping
            .added_at(self.editor.read(cx).history_id())
            .map(|(added, _)| added.original.clone());
        // A mention of a multi-mention entity dragged over the panel can get a
        // new alias: the next free token of its entity's category.
        let new_alias = (cx.has_active_drag() && mapping.drag_in_panel)
            .then_some(mapping.dragging.as_ref())
            .flatten()
            .filter(|d| {
                matches!(d, PanelDrag::Mention { .. })
                    && self.pii.review.identity_count(d.identity()) > 1
            })
            .and_then(|d| self.pii.review.identity(d.identity()))
            .map(|identity| self.pii.review.next_alias(identity.category));
        let rows_height = px(48. * count as f32);
        let upgrade = self.pii.review.applied().iter().any(|a| {
            self.pii
                .review
                .identity(a.step.identity)
                .is_some_and(|i| a.step.after.as_ref() != i.alias)
        });
        let panel = ui::panel("identity-panel", theme)
            .when(cfg!(test), |v| v.debug_selector(|| "identity-panel".into()))
            .on_drag_move::<PanelDrag>(cx.listener(
                |this, e: &gpui::DragMoveEvent<PanelDrag>, _, cx| {
                    let inside = e.bounds.contains(&e.event.position);
                    let mapping = &mut this.pii.mapping;
                    if mapping.drag_in_panel != inside {
                        mapping.drag_in_panel = inside;
                        if !inside && mapping.drop_target == Some(DropTarget::NewEntity) {
                            mapping.drop_target = None;
                        }
                        cx.notify();
                    }
                },
            ))
            .rounded_none()
            .shadow_none()
            .key_context("IdentityPanel UiPanel UiMenu")
            .tab_group()
            .tab_stop(false)
            .track_focus(&mapping.focus)
            .flex_shrink_0()
            .min_h_0()
            .flex()
            .flex_col()
            .text_size(px(13.))
            .w(self.replacements_panel_width(window))
            .h_full()
            .relative()
            .child(
                gpui::canvas(move |b, _, _| bounds.set(Some(b)), |_, _, _, _| {})
                    .absolute()
                    .inset_0(),
            )
            .on_action(cx.listener(|this, _: &ui::NextControl, window, cx| {
                ui::cycle(window, cx, Some(&this.pii.mapping.focus), false);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &ui::PreviousControl, window, cx| {
                ui::cycle(window, cx, Some(&this.pii.mapping.focus), true);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &ui::CloseMenu, window, cx| {
                if this.pii.mapping.close_actions() {
                    cx.notify();
                } else {
                    this.close_replacements(window, cx);
                }
                cx.stop_propagation();
            }))
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_1()
                    .child(div().flex_1().min_w_0().child("Replacements"))
                    .child(
                        self.replacement_control("replacement-commands", "⋯", true, cx)
                            .aria_label("Scan and model actions")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.pii.mapping.toggle_actions();
                                cx.notify();
                            })),
                    )
                    .child(
                        self.replacement_control("close-replacements", "×", true, cx)
                            .aria_label("Close replacements workspace")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.close_replacements(window, cx)
                            })),
                    ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .px_3()
                    .pb_2()
                    .child(mapping.search.clone()),
            )
            .when(mapping.actions_open, |v| {
                let bounds = mapping.bounds.get().unwrap_or_default();
                let height = px(230.).min(window.viewport_size().height - px(32.));
                let y = bounds.top() + px(38.);
                v.child(
                    deferred(
                        anchored()
                            .position(gpui::point(bounds.right() - px(280.), y))
                            .snap_to_window()
                            .child(
                                ui::panel("replacement-secondary-actions", theme)
                                    .w(px(280.))
                                    .max_h(height)
                                    .overflow_y_scroll()
                                    .child(self.replacement_commands(cx)),
                            ),
                    )
                    .with_priority(2),
                )
            })
            .child(
                uniform_list(
                    "replacement-list",
                    count,
                    cx.processor(move |this, indices: Range<usize>, _, cx| {
                        indices
                            .map(|index| {
                                let (row, mention) = entries[index].clone();
                                let id = row.id;
                                let theme = this.theme.get();
                                let palette = theme.pdf_style();
                                let expanded = this.selected_entity() == Some(id);
                                let current = mention.as_ref().is_some_and(|(annotation, _)| {
                                    this.pii.popup.is_some()
                                        && this.active_annotation() == Some(*annotation)
                                });
                                let weak = cx.entity().downgrade();
                                let accent = theme.search_accent();
                                // "Link to ALIAS" while another group's row is dragged here.
                                let link_hint = cx.has_active_drag()
                                    && this.pii.mapping.drop_target == Some(DropTarget::Entity(id))
                                    && this
                                        .pii
                                        .mapping
                                        .dragging
                                        .as_ref()
                                        .is_some_and(|d| d.identity() != id);
                                let start_drag = move |drag: &PanelDrag, cx: &mut App| {
                                    let _ = weak.update(cx, |this, _| {
                                        this.pii.mapping.dragging = Some(drag.clone());
                                        this.pii.mapping.drop_target = None;
                                        this.pii.mapping.drag_in_panel = false;
                                    });
                                    let label = match drag {
                                        PanelDrag::Mention { original, .. }
                                        | PanelDrag::Entity { original, .. } => original.clone(),
                                    };
                                    cx.new(|_| DragGhost { label, theme })
                                };
                                let control = this
                                    .replacement_control(
                                        SharedString::from(format!("replacement-entry-{index}")),
                                        "",
                                        true,
                                        cx,
                                    )
                                    .w_full()
                                    .h(px(48.))
                                    .line_height(px(17.))
                                    .overflow_hidden()
                                    .relative()
                                    .flex_col()
                                    .items_start()
                                    .justify_center()
                                    .gap_1()
                                    .px_3()
                                    .when(expanded && !current, |v| {
                                        v.bg(Hsla {
                                            a: 0.5,
                                            ..palette.placeholder_bg
                                        })
                                    })
                                    // Every row of a group accepts drops for that group.
                                    .drag_over::<PanelDrag>(move |s, drag, _, _| {
                                        if drag.identity() == id {
                                            s
                                        } else {
                                            s.border_1().border_color(accent)
                                        }
                                    })
                                    .on_drag_move::<PanelDrag>(cx.listener(
                                        move |this, e: &gpui::DragMoveEvent<PanelDrag>, _, cx| {
                                            if e.bounds.contains(&e.event.position) {
                                                this.drag_over_target(DropTarget::Entity(id), cx);
                                            }
                                        },
                                    ))
                                    .on_drop(cx.listener(move |this, drag: &PanelDrag, _, cx| {
                                        this.drop_on_entity(drag, id, cx)
                                    }))
                                    .when(current, |v| {
                                        // The accent bar keeps it identifiable under hover.
                                        v.bg(theme.sidebar_selected()).child(
                                            div()
                                                .absolute()
                                                .left_0()
                                                .top_0()
                                                .bottom_0()
                                                .w(px(3.))
                                                .bg(theme.search_accent()),
                                        )
                                    });
                                if let Some((annotation, range)) = mention {
                                    let original = if annotation & APPLIED_ID != 0 {
                                        this.pii
                                            .review
                                            .applied_occurrence(annotation & !APPLIED_ID)
                                            .unwrap()
                                            .step
                                            .original_shared()
                                            .clone()
                                    } else {
                                        let candidate =
                                            this.pii.review.candidate(annotation).unwrap();
                                        this.pii
                                            .review
                                            .variant(candidate.variant)
                                            .unwrap()
                                            .original
                                            .clone()
                                    };
                                    let (before, word, after) = this.readable_mention(&range, cx);
                                    let drag = PanelDrag::Mention {
                                        identity: id,
                                        range: range.clone(),
                                        original: original.clone(),
                                    };
                                    let applied = annotation & APPLIED_ID != 0;
                                    let accent = theme.search_accent();
                                    let undo = applied
                                        && (current
                                            || this.pii.mapping.hovered == Some(annotation));
                                    control
                                        .aria_label(format!(
                                            "{original}: {before}{word}{after}{}",
                                            if applied { ", applied" } else { "" }
                                        ))
                                        .when(cfg!(test), |v| {
                                            v.debug_selector(move || {
                                                format!("mention-{annotation}")
                                            })
                                        })
                                        .on_hover(cx.listener(
                                            move |this, hovered: &bool, _, cx| {
                                                if this
                                                    .pii
                                                    .mapping
                                                    .hover_mention(annotation, *hovered)
                                                {
                                                    cx.notify();
                                                }
                                            },
                                        ))
                                        .when(undo, |v| v.pr(px(36.)))
                                        .child(
                                            div()
                                                .w_full()
                                                .flex()
                                                .items_center()
                                                .gap_1()
                                                .text_size(px(11.))
                                                .child(
                                                    div()
                                                        .min_w_0()
                                                        .flex_1()
                                                        .text_ellipsis()
                                                        .text_color(palette.header_muted)
                                                        .child(original.to_string()),
                                                )
                                                .when(applied, |v| {
                                                    v.child(
                                                        div()
                                                            .flex_shrink_0()
                                                            .text_color(accent)
                                                            .child("✓ applied"),
                                                    )
                                                }),
                                        )
                                        .when(undo, |v| {
                                            let id = annotation & !APPLIED_ID;
                                            v.child(
                                                ui::icon_button(
                                                    SharedString::from(format!(
                                                        "mention-undo-{annotation}"
                                                    )),
                                                    "Undo this replacement (back to proposed)",
                                                    ui::Icon::Undo,
                                                    theme,
                                                    !this.pii.scanning(),
                                                )
                                                .when(cfg!(test), |v| {
                                                    v.debug_selector(move || {
                                                        format!("mention-undo-{annotation}")
                                                    })
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
                                        .child(
                                            div().w_full().text_ellipsis().child(
                                                StyledText::new(format!("{before}{word}{after}"))
                                                    .with_highlights([(
                                                        before.len()..before.len() + word.len(),
                                                        HighlightStyle {
                                                            color: Some(
                                                                this.theme.get().search_accent(),
                                                            ),
                                                            ..Default::default()
                                                        },
                                                    )]),
                                            ),
                                        )
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.navigate_identity_mention(
                                                annotation,
                                                range.start,
                                                window,
                                                cx,
                                            )
                                        }))
                                        .on_drag(drag, move |drag, _, _, cx| start_drag(drag, cx))
                                        .into_any_element()
                                } else {
                                    control
                                        .aria_label(format!(
                                            "{} to {}, {} mentions",
                                            row.original, row.alias, row.mentions
                                        ))
                                        .when(cfg!(test), |v| {
                                            v.debug_selector(move || format!("identity-{id}"))
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
                                                        .child(if expanded {
                                                            "▾"
                                                        } else {
                                                            "▸"
                                                        }),
                                                )
                                                .child(
                                                    div()
                                                        .min_w_0()
                                                        .flex_1()
                                                        .text_ellipsis()
                                                        .child(row.original.to_string()),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .w_full()
                                                .flex()
                                                .items_center()
                                                .gap_2()
                                                .child(
                                                    div()
                                                        .min_w_0()
                                                        .flex_1()
                                                        .text_ellipsis()
                                                        .text_color(
                                                            this.theme.get().search_accent(),
                                                        )
                                                        .child(format!("→ {}", row.alias)),
                                                )
                                                .child(
                                                    div()
                                                        .text_size(px(11.))
                                                        .text_color(
                                                            this.theme
                                                                .get()
                                                                .pdf_style()
                                                                .header_muted,
                                                        )
                                                        .when(link_hint, |v| v.text_color(accent))
                                                        .child(match row.applied {
                                                            _ if link_hint => {
                                                                format!("Link to {}", row.alias)
                                                            }
                                                            0 => format!(
                                                                "{} · proposed",
                                                                row.mentions
                                                            ),
                                                            n if n == row.mentions => {
                                                                format!("{n} · applied")
                                                            }
                                                            n => format!(
                                                                "{n} of {} applied",
                                                                row.mentions
                                                            ),
                                                        }),
                                                ),
                                        )
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            // A click toggles the open group.
                                            if this.selected_entity() == Some(id) {
                                                this.collapse_replacement(cx)
                                            } else {
                                                this.reveal_replacement(id, window, cx)
                                            }
                                        }))
                                        .on_drag(
                                            PanelDrag::Entity {
                                                identity: id,
                                                original: row.original.clone(),
                                            },
                                            move |drag, _, _, cx| start_drag(drag, cx),
                                        )
                                        .into_any_element()
                                }
                            })
                            .collect()
                    }),
                )
                .map(|v| {
                    // While the new-alias area shows, the list keeps its rows'
                    // height (shrinking if needed) and the area takes the rest.
                    if new_alias.is_some() {
                        v.flex_basis(rows_height).flex_shrink(1.)
                    } else {
                        v.flex_1()
                    }
                })
                .min_h_0()
                .track_scroll(&mapping.list_scroll),
            )
            .when_some(new_alias, |v, alias| {
                // The free space under the rows becomes a contoured drop area
                // that names the alias a separated mention would get.
                let accent = theme.search_accent();
                let over = mapping.drop_target == Some(DropTarget::NewEntity);
                v.child(
                    div()
                        .id("replacement-new-entity")
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "replacement-new-entity".into())
                        })
                        .flex_grow(1.)
                        .min_h(px(72.))
                        .mx_3()
                        .my_2()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap_1()
                        .rounded_lg()
                        .border_1()
                        .border_dashed()
                        .border_color(if over {
                            accent
                        } else {
                            Hsla {
                                a: 0.5,
                                ..palette.header_muted
                            }
                        })
                        .when(over, |v| v.bg(Hsla { a: 0.08, ..accent }))
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(palette.header_muted)
                                .child("Drop to give it a new alias"),
                        )
                        .child(
                            div()
                                .text_size(px(13.))
                                .text_color(accent)
                                .child(format!("→ {alias}")),
                        )
                        .on_drag_move::<PanelDrag>(cx.listener(
                            |this, e: &gpui::DragMoveEvent<PanelDrag>, _, cx| {
                                if e.bounds.contains(&e.event.position) {
                                    this.drag_over_target(DropTarget::NewEntity, cx);
                                }
                            },
                        ))
                        .on_drop(cx.listener(|this, drag: &PanelDrag, _, cx| {
                            this.drop_as_new_entity(drag, cx)
                        })),
                )
            })
            .when(count == 0, |v| {
                v.child(div().px_3().text_size(px(12.)).child(if busy {
                    "Scanning…"
                } else {
                    "No matching replacements. Review the document for missed identifiers."
                }))
            })
            .child(
                div()
                    .flex_shrink_0()
                    .border_t_1()
                    .border_color(palette.border)
                    .px_3()
                    .py_1()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(12.))
                                    .text_color(palette.header_muted)
                                    .child(if busy {
                                        "Scanning…".into()
                                    } else if pending > 0 {
                                        format!("{pending} mentions to apply")
                                    } else {
                                        format!(
                                            "{} applied mentions",
                                            self.pii.review.applied().len()
                                        )
                                    }),
                            )
                            .when(pending > 0 || upgrade, |v| {
                                v.child(
                                    ui::action_control(
                                        "apply-identity-map",
                                        "Apply replacements",
                                        theme,
                                        !busy,
                                        true,
                                    )
                                    .when(cfg!(test), |v| {
                                        v.debug_selector(|| "apply-identity-map".into())
                                    })
                                    .aria_label("Apply remaining replacements as one undo step")
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.apply_replacements(cx)),
                                    ),
                                )
                            })
                            .when(
                                pending == 0 && !upgrade && !self.pii.review.applied().is_empty(),
                                |v| {
                                    v.child(
                                        ui::icon_button(
                                            "replacement-undo",
                                            "Undo applying replacements",
                                            ui::Icon::Undo,
                                            theme,
                                            true,
                                        )
                                        .when(cfg!(test), |v| {
                                            v.debug_selector(|| "replacement-undo".into())
                                        })
                                        .on_click(
                                            cx.listener(|this, _, window, cx| {
                                                this.undo_last_step(window, cx)
                                            }),
                                        ),
                                    )
                                },
                            )
                            .when(busy, |v| {
                                v.child(
                                    self.replacement_control(
                                        "cancel-replacement-scan",
                                        "Cancel",
                                        true,
                                        cx,
                                    )
                                    .on_click(cx.listener(
                                        |this, _, _, cx| {
                                            this.pii.cancel();
                                            cx.notify();
                                        },
                                    )),
                                )
                            }),
                    )
                    .when_some(kept, |v, kept| {
                        v.child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .flex_1()
                                        .text_size(px(12.))
                                        .text_color(palette.header_muted)
                                        .child(format!("Kept {kept}")),
                                )
                                .child(
                                    ui::icon_button(
                                        "replacement-undo-keep",
                                        format!("Undo keeping {kept} originals"),
                                        ui::Icon::Undo,
                                        theme,
                                        true,
                                    )
                                    .when(cfg!(test), |v| {
                                        v.debug_selector(|| "replacement-undo-keep".into())
                                    })
                                    .on_click(cx.listener(
                                        |this, _, window, cx| this.undo_last_step(window, cx),
                                    )),
                                ),
                        )
                    })
                    .when_some(added, |v, original| {
                        v.child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .text_ellipsis()
                                        .text_size(px(12.))
                                        .text_color(palette.header_muted)
                                        .child(format!("Added “{original}”")),
                                )
                                .child(
                                    ui::icon_button(
                                        "replacement-cancel-addition",
                                        "Cancel addition",
                                        ui::Icon::Undo,
                                        theme,
                                        true,
                                    )
                                    .when(cfg!(test), |v| {
                                        v.debug_selector(|| "replacement-cancel-addition".into())
                                    })
                                    .on_click(cx.listener(
                                        |this, _, window, cx| this.cancel_addition(window, cx),
                                    )),
                                ),
                        )
                    })
                    .when_some(self.pii.error.clone(), |v, error| {
                        v.child(
                            div()
                                .text_size(px(11.))
                                .text_color(style::markdown_style(theme).alert_warning)
                                .child(error),
                        )
                    })
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(palette.header_muted)
                            .child("Experimental · Review for missed identifiers before sharing."),
                    ),
            );
        Some(panel.into_any_element())
    }
}
