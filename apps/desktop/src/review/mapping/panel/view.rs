//! How the Replacements panel draws: its setup card, its header controls,
//! the list and its footer.
use super::{
    DropTarget, PanelDrag, PanelFilter, PanelItem, PanelMenu, ROW_HEIGHT, replacement_summary,
};
use crate::review::text::{self, SetupCopy};
use crate::{document::DocumentView, ui, ui::style};
use gpui::{AnyElement, Context, Hsla, SharedString, Window, div, prelude::*, px, uniform_list};
use mdoc_store::settings;
use std::ops::Range;

impl DocumentView {
    /// The panel while the selected model is missing: consent, progress and
    /// retry in place of the review.
    fn pii_setup_panel(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let model = self
            .preferences
            .borrow()
            .snapshot()
            .ok()
            .map(|p| settings::Model::Pii(p.pseudonymization.model));
        let panel = self.model_panel.read(cx);
        let progress = model.and_then(|m| panel.progress_of(m));
        let cancelling = panel.cancelling();
        let needs_setup = model.is_none_or(|m| panel.needs_setup(m));
        let pending = model.map_or_else(Default::default, |m| {
            panel.pending[crate::settings::Panel::index(m)]
        });
        let idle = panel.working.is_none() && !mdoc_import::model_work::busy();
        let error = self.pii.error.clone();
        let copy = SetupCopy::new(model, pending);
        let primary = if !needs_setup {
            text::PSEUDONYMIZE
        } else if error.is_some() {
            text::RETRY
        } else {
            copy.action
        };
        let card = ui::state_card("pii-setup-card", theme)
            .when(cfg!(test), |v| v.debug_selector(|| "pii-setup-card".into()))
            .max_w_full()
            .child(ui::card_title(copy.title))
            .child(ui::card_text(copy.text, theme));
        let card = if let Some(state) = progress {
            card.child(ui::setup_progress(
                "pii-setup-progress",
                &state,
                cancelling,
                theme,
            ))
            .child(
                ui::card_actions().child(
                    crate::settings::control(
                        "cancel-pii-setup",
                        crate::ui::text::CANCEL,
                        theme,
                        !cancelling,
                    )
                    .track_focus(&self.card_focus.clone().tab_stop(!cancelling))
                    .when(cfg!(test), |v| {
                        v.debug_selector(|| "cancel-pii-setup".into())
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.model_panel
                            .update(cx, |panel, cx| panel.cancel_setup(cx))
                    })),
                ),
            )
        } else {
            card.when_some(error, |v, e| v.child(ui::card_error(e, theme)))
                .child(
                    ui::card_actions()
                        .child(
                            ui::primary_button(
                                "pii-setup-primary",
                                primary,
                                theme,
                                idle || !needs_setup,
                            )
                            .when(cfg!(test), |v| {
                                v.debug_selector(|| "pii-setup-primary".into())
                            })
                            .track_focus(&self.card_focus.clone().tab_stop(idle || !needs_setup))
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    if !needs_setup {
                                        this.start_pii_scan(cx);
                                    } else if idle {
                                        this.setup_pii_model(window, cx);
                                    }
                                },
                            )),
                        )
                        .child(
                            crate::settings::control(
                                "pii-setup-cancel",
                                crate::ui::text::CANCEL,
                                theme,
                                true,
                            )
                            .when(cfg!(test), |v| {
                                v.debug_selector(|| "pii-setup-cancel".into())
                            })
                            .on_click(cx.listener(
                                |this, _, window, cx| this.close_replacements(window, cx),
                            )),
                        )
                        .child(
                            ui::link_button(
                                "choose-pii-model",
                                ui::text::CHOOSE_MODEL,
                                theme,
                                true,
                            )
                            .when(cfg!(test), |v| {
                                v.debug_selector(|| "choose-pii-model".into())
                            })
                            .on_click(
                                cx.listener(|this, _, window, cx| {
                                    this.show_pii_settings(window, cx)
                                }),
                            ),
                        ),
                )
        };
        let note = copy.note;
        ui::panel("identity-panel", theme)
            .when(cfg!(test), |v| v.debug_selector(|| "identity-panel".into()))
            // A card inset in the document, like the content card.
            .m_2()
            .rounded_lg()
            .shadow_none()
            .overflow_hidden()
            .key_context("IdentityPanel UiPanel UiMenu")
            .tab_group()
            .tab_stop(false)
            .track_focus(&self.pii.mapping.focus)
            .on_action(cx.listener(|this, _: &ui::CloseMenu, window, cx| {
                // An open row or filter menu closes before the panel (#92).
                if !this.close_panel_menu(window, cx) {
                    this.close_replacements(window, cx);
                }
                cx.stop_propagation();
            }))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .w(self.replacements_panel_width(window) - px(16.))
            .p_3()
            .child(
                card.child(ui::card_note(note, theme))
                    .when_some(model, |card, model| {
                        card.child(ui::model_attribution("pii-model-attribution", model, theme))
                    }),
            )
            .into_any_element()
    }

    /// Expand or collapse every listed identity; its name follows the state.
    fn expand_all_control(&self, cx: &mut Context<Self>) -> ui::Control {
        let expanded = self.all_identities_expanded(cx);
        let (label, icon) = if expanded {
            (text::COLLAPSE_ALL_IDENTITIES, ui::Icon::CollapseAll)
        } else {
            (text::EXPAND_ALL_IDENTITIES, ui::Icon::ExpandAll)
        };
        ui::header_icon(
            "replacement-expand-all",
            label,
            icon,
            self.theme.get(),
            false,
        )
        .track_focus(&self.pii.mapping.expand_all_focus)
        .when(cfg!(test), |v| {
            v.debug_selector(|| "replacement-expand-all".into())
        })
        .on_click(
            cx.listener(move |this, _, _, cx| this.set_all_identities_expanded(!expanded, cx)),
        )
    }

    /// The panel-only filter; a dot marks anything but All.
    fn filter_control(&self, cx: &mut Context<Self>) -> gpui::Div {
        let theme = self.theme.get();
        let filter = self.pii.mapping.filter;
        let open = self.pii.mapping.panel_menu == Some(PanelMenu::Filter);
        let label = text::show_filter(filter.label());
        let button = ui::header_icon_untipped(
            "replacement-filter",
            label.clone(),
            ui::Icon::Filter,
            theme,
            filter != PanelFilter::All,
        )
        // No tooltip over its own open menu.
        .when(!open, |v| {
            v.tooltip(crate::ui::style::tooltip(label, theme))
        })
        .track_focus(&self.pii.mapping.filter_focus)
        .when(cfg!(test), |v| {
            v.debug_selector(|| "replacement-filter".into())
        })
        .relative()
        .when(filter != PanelFilter::All, |v| {
            v.child(
                div()
                    .absolute()
                    .top(px(5.))
                    .right(px(5.))
                    .size(px(6.))
                    .rounded_full()
                    .bg(theme.applied()),
            )
        })
        .on_click(cx.listener(|this, _, window, cx| {
            let menu = &mut this.pii.mapping.panel_menu;
            *menu = (*menu != Some(PanelMenu::Filter)).then_some(PanelMenu::Filter);
            // The menu takes focus on the current filter.
            if this.pii.mapping.panel_menu.is_some() {
                let row = format!("replacement-filter-{}", this.pii.mapping.filter.label());
                window.focus(
                    &this.pii.popup_control_focus(SharedString::from(row), cx),
                    cx,
                );
            }
            cx.notify();
        }));
        let menu = (self.pii.mapping.panel_menu == Some(PanelMenu::Filter)).then(|| {
            let surface = self.menu_surface("replacement-filter-menu");
            self.focus_menu(
                surface,
                &self.pii.mapping.filter_menu_focus,
                &self.pii.mapping.filter_focus,
                |this, _, _| {
                    this.pii.mapping.panel_menu = None;
                    true
                },
                cx,
            )
            .w(px(160.))
            .children(PanelFilter::ALL.into_iter().map(|option| {
                self.menu_row(
                    SharedString::from(format!("replacement-filter-{}", option.label())),
                    option.label(),
                    "",
                    Some(option == filter),
                    true,
                    cx,
                )
                // A choice hands focus to the list it filters.
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.pii.mapping.filter = option;
                    this.pii.mapping.panel_menu = None;
                    window.focus(&this.pii.mapping.focus, cx);
                    cx.notify();
                }))
            }))
            .into_any_element()
        });
        // Anchored at the button's left; the menu snaps inside the window.
        self.with_dropdown(button, menu)
    }
    pub(crate) fn replacements_panel_width(&self, window: &Window) -> gpui::Pixels {
        if self.pii.mapping.open && self.can_copy_markdown() {
            px((self.chrome_width(window) * 0.4).clamp(260., 380.))
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
        // Notes need no detection set up.
        if self.inspector == crate::document::Inspector::Notes {
            return Some(self.notes_panel(window, cx));
        }
        if self.pii.setup {
            return Some(self.pii_setup_panel(window, cx));
        }
        // Inspector: Review | Share | Notes.
        if self.inspector == crate::document::Inspector::Share {
            return Some(self.share_panel(window, cx));
        }
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let mapping = &self.pii.mapping;
        let entries = self.replacement_entries(cx);
        let count = entries.len();
        let bounds = mapping.bounds.clone();
        let busy = self.pii.scanning();
        let pending = self.pii.review.remaining();
        let history = self.editor.read(cx).history_id();
        // The latest step's after-action notice, newest kind first.
        let notice = mapping
            .notice_visible()
            .then(|| {
                if let Some((added, _)) = mapping.added_at(history) {
                    Some((text::added(&added.original), crate::ui::text::CANCEL, true))
                } else if let Some(kept) = mapping.kept_at(history) {
                    Some((text::kept(kept), text::UNDO, false))
                } else if let Some(wording) = mapping.merged_at(history) {
                    Some((text::merged_into(wording), text::UNDO, false))
                } else {
                    mapping
                        .applied_at(history)
                        .map(|n| (text::applied(n), text::UNDO, false))
                }
            })
            .flatten();
        // A mention of a multi-mention entity dragged over the panel can get a
        // new alias: the next free token of its entity's category.
        let new_alias = ((cx.has_active_drag() && mapping.drag_in_panel) || mapping.key_moving)
            .then_some(mapping.dragging.as_ref())
            .flatten()
            .filter(|d| {
                matches!(d, PanelDrag::Mention { .. })
                    && self.pii.review.identity_count(d.identity()) > 1
            })
            .and_then(|d| self.pii.review.identity(d.identity()))
            .map(|identity| self.pii.review.next_alias(identity.category));
        let rows_height = px(ROW_HEIGHT * count as f32);
        let total_entities = entries
            .iter()
            .filter(|(_, item)| matches!(item, PanelItem::Header))
            .count();
        let kept = self.kept_mention_count(cx);
        let applied = self.pii.review.applied().len();
        let names = self.pending_name_count(cx);
        let summary = replacement_summary(busy, names, pending, applied, kept);
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
            // A card inset in the document, like the content card.
            .m_2()
            .rounded_lg()
            .shadow_none()
            .overflow_hidden()
            .key_context("IdentityPanel UiPanel UiMenu")
            .tab_group()
            .tab_stop(false)
            .track_focus(&mapping.focus)
            .flex_shrink_0()
            .min_h_0()
            .flex()
            .flex_col()
            .text_size(px(13.))
            .w(self.replacements_panel_width(window) - px(16.))
            .relative()
            .child(
                gpui::canvas(move |b, _, _| bounds.set(Some(b)), |_, _, _, _| {})
                    .absolute()
                    .inset_0(),
            )
            // An open row or filter menu keeps Tab inside the panel;
            // otherwise Tab moves on to the next region.
            .on_action(cx.listener(|this, _: &ui::NextControl, window, cx| {
                if this.pii.mapping.panel_menu.is_some() {
                    ui::cycle(window, cx, Some(&this.pii.mapping.focus), false);
                } else {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &ui::PreviousControl, window, cx| {
                if this.pii.mapping.panel_menu.is_some() {
                    ui::cycle(window, cx, Some(&this.pii.mapping.focus), true);
                } else {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &ui::CloseMenu, window, cx| {
                // An open row or filter menu closes before the panel (#92).
                if !this.close_panel_menu(window, cx) {
                    this.close_replacements(window, cx);
                }
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &mdoc_editor::Undo, window, cx| {
                this.undo_last_step(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &mdoc_editor::Redo, window, cx| {
                this.redo_last_step(window, cx);
                cx.stop_propagation();
            }))
            // Releasing ⌥ drops a keyboard move.
            .on_modifiers_changed(cx.listener(|this, e: &gpui::ModifiersChangedEvent, _, cx| {
                if this.pii.mapping.key_moving && !e.modifiers.alt {
                    this.finish_key_move(cx);
                }
            }))
            .child(self.inspector_tabs(total_entities, cx))
            .child(
                div().flex_shrink_0().px_3().pb_2().child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .h(px(30.))
                        .pl_2()
                        .rounded_md()
                        .border_1()
                        .border_color(palette.border)
                        .child(
                            gpui::svg()
                                .data(ui::Icon::Search.data())
                                .size(px(13.))
                                .flex_shrink_0()
                                .text_color(palette.header_muted),
                        )
                        .child(div().flex_1().min_w_0().child(mapping.search.clone()))
                        .child(self.filter_control(cx))
                        .child(self.expand_all_control(cx)),
                ),
            )
            .child(div().h(px(1.)).flex_shrink_0().bg(palette.border))
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
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
                    .when(count == 0, |v| v.flex_grow(0.).flex_basis(px(0.)))
                    .child(
                        uniform_list(
                            "replacement-list",
                            count,
                            cx.processor(move |this, indices: Range<usize>, _, cx| {
                                indices
                                    .map(|index| this.panel_row(index, entries[index].clone(), cx))
                                    .collect()
                            }),
                        )
                        .flex_1()
                        .min_h_0()
                        .track_scroll(&mapping.list_scroll),
                    )
                    .child(self.wheel_steps(cx)),
            )
            .when_some(new_alias, |v, alias| {
                // The free space under the rows becomes a contoured drop area
                // that names the alias a separated mention would get.
                let accent = theme.applied();
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
                                .child(text::DROP_FOR_A_NEW_ALIAS),
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
                let searching = !mapping.search.read(cx).value().trim().is_empty();
                // While scanning: quiet skeleton rows; rows arrive when the
                // scan ends.
                let body = if busy && !searching {
                    div()
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "replacement-skeleton".into())
                        })
                        .flex()
                        .flex_col()
                        .px_3()
                        .pt_3()
                        .gap(px(14.))
                        .children(
                            [0.62, 0.48, 0.7, 0.4, 0.56, 0.66]
                                .into_iter()
                                .enumerate()
                                .map(|(i, width)| {
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .when(i == 0 || i == 4, |v| v.mt_1())
                                        .child(
                                            div()
                                                .size(px(14.))
                                                .rounded(px(4.))
                                                .bg(palette.placeholder_bg),
                                        )
                                        .child(
                                            div()
                                                .h(px(9.))
                                                .w(gpui::relative(width))
                                                .rounded_full()
                                                .bg(palette.placeholder_bg),
                                        )
                                }),
                        )
                        .into_any_element()
                } else {
                    div()
                        .px_3()
                        .py_3()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .text_size(px(12.))
                        .text_color(palette.header_muted)
                        .child(if searching {
                            text::NO_MATCHING_REPLACEMENTS
                        } else {
                            text::NOTHING_FOUND
                        })
                        .into_any_element()
                };
                v.child(body).child(div().flex_1())
            })
            .child(
                div()
                    .flex_shrink_0()
                    .border_t_1()
                    .border_color(palette.border)
                    .px_3()
                    .py_2()
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
                                    .when(cfg!(test), |v| {
                                        v.debug_selector(|| "replacement-summary".into())
                                    })
                                    .flex_1()
                                    .min_w(px(96.))
                                    .text_size(px(12.))
                                    .text_color(palette.header_fg)
                                    .child(summary),
                            )
                            .when(upgrade && pending == 0, |v| {
                                v.child(
                                    ui::link_button(
                                        "apply-identity-map",
                                        text::UPDATE_ALIASES,
                                        theme,
                                        !busy,
                                    )
                                    .when(cfg!(test), |v| {
                                        v.debug_selector(|| "apply-identity-map".into())
                                    })
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.apply_replacements(cx)),
                                    ),
                                )
                            })
                            .child(
                                ui::link_button("review-rescan", text::RESCAN, theme, !busy)
                                    .when(cfg!(test), |v| {
                                        v.debug_selector(|| "review-rescan".into())
                                    })
                                    // A faded accent still reads as a link
                                    // (#309): grey while a scan runs.
                                    .when(busy, |v| v.text_color(palette.header_muted))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if !this.pii.scanning() {
                                            this.start_pii_scan(cx);
                                        }
                                    })),
                            ),
                    )
                    .when_some(notice, |v, (text, action, addition)| {
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
                                        .child(text),
                                )
                                .child(
                                    ui::link_button("replacement-notice-undo", action, theme, true)
                                        .when(cfg!(test), |v| {
                                            v.debug_selector(|| "replacement-notice-undo".into())
                                        })
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            if addition {
                                                this.cancel_addition(window, cx)
                                            } else {
                                                this.undo_last_step(window, cx)
                                            }
                                        })),
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
                    .children(self.project_names_section(cx))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(palette.header_muted)
                            .child(text::REVIEW_FOR_MISSED_IDENTIFIERS),
                    ),
            );
        Some(self.panel_actions(panel, cx).into_any_element())
    }
}
