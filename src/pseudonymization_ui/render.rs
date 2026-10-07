//! PII toolbar, status and review rendering. State stays in the parent controller.
use super::{PopupTarget, ReviewUi};
use crate::{
    AcceptAllPseudonyms, AcceptPseudonymCandidate, AddPseudonymCandidate, Anonymize,
    ClosePseudonymPopup, KeepPseudonymCandidate, NextCandidate, PreviousCandidate, Pseudonymize,
    Settings, Workspace, button, model_work,
    pseudonymization::{Category, Mode},
    pseudonymization_detector as detector, settings, settings_ui, style, ui,
};
use gpui::{
    AnyElement, App, Context, Focusable, Pixels, Point, Window, anchored, deferred, div,
    prelude::*, px,
};
use std::ops::Range;

impl ReviewUi {
    pub(super) fn reveal_popup_control(
        &self,
        id: impl Into<gpui::ElementId>,
        control: gpui::Stateful<gpui::Div>,
        cx: &App,
    ) -> gpui::Stateful<gpui::Div> {
        let focus = self
            .popup_controls
            .borrow_mut()
            .entry(id.into())
            .or_insert_with(|| cx.focus_handle().tab_stop(true))
            .clone();
        ui::reveal_focus(
            control.track_focus(&focus),
            focus,
            self.popup_scroll.clone(),
        )
    }
}

impl Workspace {
    pub(crate) fn pii_toolbar_control(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let mode = self.pseudonymization.review.mode;
        let enabled = self.can_copy_markdown() && !self.pseudonymization.scanning();
        div()
            .id("pii-toolbar")
            .relative()
            .flex()
            .items_center()
            .child(
                ui::icon_control(
                    mode.label(),
                    mode.label(),
                    ui::Icon::Anonymous,
                    theme,
                    enabled,
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    if !enabled {
                        return;
                    }
                    match mode {
                        Mode::Anonymize => this.anonymize(&Anonymize, window, cx),
                        Mode::Pseudonymize => this.pseudonymize(&Pseudonymize, window, cx),
                    }
                })),
            )
            .child(
                ui::icon_control(
                    "pii-mode-menu",
                    "Choose PII action",
                    ui::Icon::ChevronDown,
                    theme,
                    true,
                )
                .w(px(20.))
                .on_click(cx.listener(|this, _, window, cx| {
                    if this.pseudonymization.mode_menu_open {
                        this.close_pii_mode_menu(window, cx);
                    } else {
                        this.pseudonymization.mode_menu_previous = window.focused(cx);
                        this.pseudonymization.mode_menu_open = true;
                        window.focus(&this.pseudonymization.mode_menu_focus, cx);
                        cx.notify();
                    }
                })),
            )
            .when(self.pseudonymization.mode_menu_open, |v| {
                v.child(deferred(
                    ui::panel("pii-mode-options", theme)
                        .absolute()
                        .top(px(34.))
                        .left_0()
                        .w(px(180.))
                        .p_1()
                        .key_context("UiPanel UiMenu")
                        .track_focus(&self.pseudonymization.mode_menu_focus)
                        .tab_group()
                        .tab_stop(false)
                        .flex()
                        .flex_col()
                        .on_action(cx.listener(|this, _: &ui::NextControl, window, cx| {
                            ui::cycle(
                                window,
                                cx,
                                Some(&this.pseudonymization.mode_menu_focus),
                                false,
                            );
                            cx.stop_propagation();
                        }))
                        .on_action(cx.listener(|this, _: &ui::PreviousControl, window, cx| {
                            ui::cycle(
                                window,
                                cx,
                                Some(&this.pseudonymization.mode_menu_focus),
                                true,
                            );
                            cx.stop_propagation();
                        }))
                        .on_action(cx.listener(|this, _: &ui::CloseMenu, window, cx| {
                            this.close_pii_mode_menu(window, cx)
                        }))
                        .on_mouse_down_out(cx.listener(
                            |this, _: &gpui::MouseDownEvent, window, cx| {
                                this.close_pii_mode_menu(window, cx)
                            },
                        ))
                        .children([Mode::Anonymize, Mode::Pseudonymize].into_iter().map(
                            |choice| {
                                let id = if choice == Mode::Anonymize {
                                    "choose-anonymize"
                                } else {
                                    "choose-pseudonymize"
                                };
                                ui::control(id, choice.label(), theme, true)
                                    .when(choice == mode, |v| {
                                        v.bg(theme.sidebar_selected())
                                            .text_color(theme.search_accent())
                                    })
                                    .when(cfg!(test), |v| v.debug_selector(move || id.into()))
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.close_pii_mode_menu(window, cx);
                                        this.select_pii_mode(choice, cx);
                                    }))
                            },
                        )),
                ))
            })
            .into_any_element()
    }
    fn anonymization_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let scanning = self.pseudonymization.scanning();
        let message = if scanning {
            "Scanning…".to_owned()
        } else if let Some((count, _)) = self.pseudonymization.completion {
            format!("Replaced {count} occurrences")
        } else {
            "Anonymize".to_owned()
        };
        div()
            .id("anonymization-status")
            .flex()
            .flex_col()
            .gap_1()
            .px_2()
            .py_1()
            .text_size(px(12.))
            .border_b_1()
            .border_color(theme.pdf_style().border)
            .when(cfg!(test), |v| {
                v.debug_selector(|| "anonymization-status".into())
            })
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_1()
                    .child(div().flex_1().min_w_0().child(if scanning {
                        ui::activity("anonymization-activity", message, theme).into_any_element()
                    } else {
                        div().child(message).into_any_element()
                    }))
                    .when(scanning, |v| {
                        v.child(
                            ui::control("cancel-anonymize", "Cancel", theme, true)
                                .when(cfg!(test), |v| {
                                    v.debug_selector(|| "cancel-anonymize".into())
                                })
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.pseudonymization.cancel();
                                    this.pseudonymization.error =
                                        Some("Scan cancelled. No replacements applied.".into());
                                    cx.notify();
                                })),
                        )
                    })
                    .when(!scanning, |v| {
                        v.child(
                            ui::control("review-anonymization", "Review", theme, true)
                                .when(cfg!(test), |v| {
                                    v.debug_selector(|| "review-anonymization".into())
                                })
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.pseudonymization.manual_review = true;
                                    this.sync_annotations(cx);
                                    cx.notify();
                                })),
                        )
                        .when(self.pseudonymization.error.is_some(), |v| {
                            v.child(
                                ui::control("anonymization-settings", "Settings", theme, true)
                                    .on_click(|_, window, cx| {
                                        window.dispatch_action(Box::new(Settings), cx)
                                    }),
                            )
                        })
                        .child(
                            ui::control("close-anonymization", "×", theme, true)
                                .aria_label("Close anonymization status")
                                .on_click(cx.listener(|this, _, _, cx| this.leave_pseudonyms(cx))),
                        )
                    }),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(theme.pdf_style().header_muted)
                    .child("Experimental · Check for missed identifiers before copying."),
            )
            .when(self.pseudonymization.review.skipped_syntax_spans > 0, |v| {
                v.child(div().text_size(px(11.)).child(
                    "Some spans cross Markdown syntax. Use Review to add a narrower selection.",
                ))
            })
            .when_some(self.pseudonymization.error.clone(), |v, error| {
                v.child(
                    div()
                        .text_color(style::markdown_style(theme).alert_warning)
                        .child(error),
                )
            })
            .into_any_element()
    }
    pub(crate) fn pseudonym_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.pseudonymization.review.open || !self.can_copy_markdown() {
            return None;
        }
        if self.pseudonymization.review.mode == Mode::Anonymize
            && !self.pseudonymization.manual_review
        {
            return Some(self.anonymization_bar(cx));
        }
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let scanning = self.pseudonymization.scanning();
        let remaining = self.pseudonymization.review.remaining();
        let can_accept_all = remaining > 0 && !scanning;
        let accept_all_bounds = self.pseudonymization.accept_all_bounds.clone();
        let menu_bounds = self.pseudonymization.menu_bounds.clone();
        let selected = self
            .preferences
            .borrow()
            .snapshot()
            .ok()
            .map(|p| p.pseudonymization.model);
        let needs_setup = selected.is_some_and(|model| {
            let model = settings::Model::Pii(model);
            let index = settings::Model::ALL
                .iter()
                .position(|m| *m == model)
                .unwrap();
            !matches!(
                self.model_panel.read(cx).statuses[index],
                settings_ui::Status::Ready
            )
        });
        Some(div().id("pseudonym-review-bar").relative().flex().flex_col().gap_1().px_2().py_1().text_size(px(12.)).border_b_1().border_color(p.border)
            .child(div().flex().items_center().gap_1()
                .child(div().flex_1().min_w_0().child(if scanning { ui::activity("review-activity", "Scanning…", theme).into_any_element() } else { div().child(format!("{remaining} candidates")).into_any_element() }))
                .child(button("Previous", PreviousCandidate, theme))
                .child(button("Next", NextCandidate, theme))
                .child(ui::control("review-rescan", if scanning { "Cancel" } else { "Rescan" }, theme, true)
                    .on_click(cx.listener(|this, _, _, cx| {
                        if this.pseudonymization.scanning() { this.pseudonymization.cancel(); cx.notify(); }
                        else { this.scan_pseudonyms(cx); }
                    })))
                .child(ui::control("review-menu", "More", theme, true)
                    .child(gpui::canvas(move |bounds, _, _| menu_bounds.set(bounds), |_, _, _, _| {}).absolute().inset_0())
                    .relative()
                    .when(cfg!(test), |v| v.debug_selector(|| "review-menu".into()))
                    .on_click(cx.listener(|this, _, window, cx| {
                        if this.pseudonymization.menu_open { this.close_review_menu(window, cx); } else {
                            this.pseudonymization.menu_previous = window.focused(cx);
                            this.pseudonymization.menu_open = true;
                            window.focus(&this.pseudonymization.menu_focus, cx); cx.notify();
                        }
                    })))
                .child(ui::control("leave-pseudonyms", "Close review", theme, true)
                    .when(cfg!(test), |v| v.debug_selector(|| "leave-pseudonyms".into()))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.leave_pseudonyms(cx);
                        window.focus(&this.editor.read(cx).focus_handle(cx), cx);
                    }))))
            .child(div().flex().items_center().gap_1().text_color(p.header_muted)
                .child(div().flex_1().min_w_0().text_size(px(11.)).child("Experimental · May miss identifying details. Review all Markdown before sharing."))
                .child(ui::control("pseudonym-details", if self.pseudonymization.details { "Less" } else { "Details" }, theme, true)
                    .on_click(cx.listener(|this, _, _, cx| { this.pseudonymization.details = !this.pseudonymization.details; cx.notify(); }))))
            .when(self.pseudonymization.details, |v| v.child(div().text_size(px(11.)).text_color(p.header_muted)
                .child(format!("Russian and hidden-source details may be missed. Completed scans: {}{}", self.pseudonymization.scans.iter().map(|c| format!("{} (threshold {})", c.model.name(), c.threshold)).collect::<Vec<_>>().join(", "), if self.pseudonymization.review.skipped_syntax_spans > 0 { ". Some spans cross Markdown syntax; add a narrower selection manually." } else { "" }))))
            .when_some(self.pseudonymization.error.clone(), |v, error| v.child(div().text_color(style::markdown_style(theme).alert_warning).child(error)))
            .when(self.pseudonymization.menu_open, |v| v.child(deferred(ui::panel("review-command-menu", theme).absolute().top(px(34.)).right(px(8.)).w(px(230.)).p_1().key_context("UiPanel UiMenu").track_focus(&self.pseudonymization.menu_focus).tab_group().tab_stop(false)
                .flex().flex_col()
                .on_action(cx.listener(|this, _: &ui::NextControl, window, cx| { ui::cycle(window, cx, Some(&this.pseudonymization.menu_focus), false); cx.stop_propagation(); }))
                    .on_action(cx.listener(|this, _: &ui::PreviousControl, window, cx| { ui::cycle(window, cx, Some(&this.pseudonymization.menu_focus), true); cx.stop_propagation(); }))
                    .on_action(cx.listener(|this, _: &ui::CloseMenu, window, cx| this.close_review_menu(window, cx)))
                .on_mouse_down_out(cx.listener(|this, _: &gpui::MouseDownEvent, window, cx| this.close_review_menu(window, cx)))
                .child(ui::control("pseudonym-category", format!("Category: {}", self.pseudonymization.category.label()), theme, true)
                    .on_click(cx.listener(|this, _, _, cx| {
                        let index = Category::ALL.iter().position(|c| *c == this.pseudonymization.category).unwrap_or(0);
                        this.pseudonymization.category = Category::ALL[(index + 1) % Category::ALL.len()]; cx.notify();
                    })))
                .child(ui::control("add-pseudonym-selection", "Add selection", theme, true)
                    .on_click(cx.listener(|this, _, window, cx| { this.close_review_menu(window, cx); this.add_pseudonym(&AddPseudonymCandidate, window, cx); })))
                .child(ui::control("accept-all-pseudonyms", "Accept all", theme, can_accept_all).relative()
                    .when(cfg!(test), |v| v.debug_selector(|| "accept-all-pseudonyms".into()))
                    .child(gpui::canvas(move |bounds, _, _| accept_all_bounds.set(Some(bounds)), |_, _, _, _| {}).absolute().inset_0())
                    .on_click(cx.listener(move |this, _, window, cx| { if can_accept_all { this.close_review_menu(window, cx); this.accept_all_pseudonyms(&AcceptAllPseudonyms, window, cx); } })))
                .child(ui::control("review-settings", "Settings", theme, true)
                    .on_click(cx.listener(|this, _, window, cx| { this.close_review_menu(window, cx); this.model_panel.update(cx, |panel, cx| panel.show(window, cx)); })))
                .when(needs_setup, |v| v.child(ui::control("review-download-model", format!("Download model ({} MB)", selected.map(detector::download_megabytes).unwrap_or(0)), theme, !model_work::busy())
                    .when(cfg!(test), |v| v.debug_selector(|| "review-download-model".into()))
                    .on_click(cx.listener(|this, _, window, cx| { if !model_work::busy() { this.close_review_menu(window, cx); this.setup_pseudonyms(window, cx); } }))))
            )))
            .into_any_element())
    }
    pub(crate) fn pseudonym_popup(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let anonymous = self.pseudonymization.review.mode == Mode::Anonymize;
        let popup = self.pseudonymization.popup.as_ref()?;
        if let PopupTarget::Choose { ids, selected } = &popup.target {
            return Some(self.annotation_chooser(ids.clone(), *selected, cx));
        }
        if let PopupTarget::Applied(id) = popup.target {
            return self.applied_popup(id, window, cx);
        }
        let (group_id, mention) = popup.candidate()?;
        let group = self.pseudonymization.review.group(group_id)?;
        let original = group.original.to_string();
        let count = group.mentions.len();
        let annotation = self
            .pseudonymization
            .review
            .annotation_id(group.id, &mention)?;
        let anchor = self.editor.read(cx).annotation_bounds(annotation);
        let position: Point<Pixels> = anchor
            .map(|bounds| gpui::point(bounds.left(), bounds.bottom() + px(4.)))
            .unwrap_or_else(|| self.scroll.bounds().origin + gpui::point(px(24.), px(24.)));
        let palette = self.theme.get().pdf_style();
        let all = popup.all;
        let mappings = self.pseudonymization.review.mappings();
        Some(
            deferred(
                anchored().position(position).snap_to_window().child(
                    ui::panel("pseudonym-popup", self.theme.get())
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "pseudonym-popup".into())
                        })
                        .track_scroll(&self.pseudonymization.popup_scroll)
                        .key_context("PseudonymReview UiPanel")
                        .tab_group()
                        .tab_stop(false)
                        .track_focus(&self.pseudonymization.focus)
                        .w(px(340.))
                        .max_w_full()
                        .max_h((window.viewport_size().height - px(40.)).max(px(120.)))
                        .overflow_y_scroll()
                        .p_3()
                        .on_action(cx.listener(|this, _: &ui::NextControl, window, cx| {
                            ui::cycle(window, cx, Some(&this.pseudonymization.focus), false);
                            cx.stop_propagation();
                        }))
                        .on_action(cx.listener(|this, _: &ui::PreviousControl, window, cx| {
                            ui::cycle(window, cx, Some(&this.pseudonymization.focus), true);
                            cx.stop_propagation();
                        }))
                        .on_action(cx.listener(Self::accept_pseudonym))
                        .on_action(cx.listener(Self::keep_pseudonym))
                        .on_action(cx.listener(Self::close_pseudonym_popup))
                        .on_mouse_down_out(cx.listener(
                            |this, event: &gpui::MouseDownEvent, window, cx| {
                                // Keep the replacement draft until the bulk button's
                                // click handler can validate and apply it.
                                if this.pseudonymization.menu_open
                                    || this
                                        .pseudonymization
                                        .menu_bounds
                                        .get()
                                        .contains(&event.position)
                                    || this
                                        .pseudonymization
                                        .accept_all_bounds
                                        .get()
                                        .is_some_and(|bounds| bounds.contains(&event.position))
                                {
                                    return;
                                }
                                this.close_pseudonym_popup(&ClosePseudonymPopup, window, cx);
                            },
                        ))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .text_size(px(12.))
                                        .text_color(palette.header_muted)
                                        .child(group.category.label()),
                                )
                                .child(div().flex_1())
                                .child(self.pseudonymization.reveal_popup_control(
                                    "popup-close",
                                    button("×", ClosePseudonymPopup, self.theme.get()),
                                    cx,
                                )),
                        )
                        .child(ui::replacement_transition(
                            div()
                                .id("pseudonym-original")
                                .when(cfg!(test), |v| {
                                    v.debug_selector(|| "pseudonym-original".into())
                                })
                                .child(original)
                                .into_any_element(),
                            if anonymous {
                                div()
                                    .id("anonymous-replacement")
                                    .when(cfg!(test), |v| {
                                        v.debug_selector(|| "anonymous-replacement".into())
                                    })
                                    .child(group.category.token())
                                    .into_any_element()
                            } else {
                                div()
                                    .id("pseudonym-replacement")
                                    .min_w_0()
                                    .w_full()
                                    .border_b_1()
                                    .border_color(self.theme.get().search_accent())
                                    .child(self.pseudonymization.input.clone())
                                    .map(|v| {
                                        ui::reveal_focus(
                                            v,
                                            self.pseudonymization.input.read(cx).focus_handle(cx),
                                            self.pseudonymization.popup_scroll.clone(),
                                        )
                                    })
                                    .into_any_element()
                            },
                            self.theme.get(),
                        ))
                        .when_some(self.pseudonymization.error.clone(), |view, error| {
                            view.child(div().py_1().text_color(palette.header_muted).child(error))
                        })
                        .child(
                            div()
                                .id("pseudonym-scope")
                                .map(|v| {
                                    self.pseudonymization.reveal_popup_control(
                                        "pseudonym-scope",
                                        v,
                                        cx,
                                    )
                                })
                                .aria_label("Apply to all exact occurrences")
                                .focus_visible(|s| s.bg(palette.placeholder_bg))
                                .key_context("UiControl")
                                .tab_index(0)
                                .role(gpui::Role::CheckBox)
                                .aria_toggled(if all {
                                    gpui::Toggled::True
                                } else {
                                    gpui::Toggled::False
                                })
                                .cursor_pointer()
                                .py_2()
                                .child(if all {
                                    format!("☑ All {count} exact matches")
                                } else {
                                    format!("☐ All {count} exact matches")
                                })
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(popup) = &mut this.pseudonymization.popup {
                                        popup.all = !popup.all;
                                    }
                                    cx.notify();
                                })),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap_2()
                                .child(
                                    self.pseudonymization.reveal_popup_control(
                                        "Accept",
                                        ui::action_control(
                                            "Accept",
                                            "Replace",
                                            self.theme.get(),
                                            true,
                                            true,
                                        )
                                        .when(cfg!(test), |v| v.debug_selector(|| "Accept".into()))
                                        .on_click(
                                            |_, window, cx| {
                                                window.dispatch_action(
                                                    Box::new(AcceptPseudonymCandidate),
                                                    cx,
                                                )
                                            },
                                        ),
                                        cx,
                                    ),
                                )
                                .child(
                                    self.pseudonymization.reveal_popup_control(
                                        "Keep",
                                        ui::action_control(
                                            "Keep",
                                            "Keep",
                                            self.theme.get(),
                                            true,
                                            false,
                                        )
                                        .on_click(
                                            |_, window, cx| {
                                                window.dispatch_action(
                                                    Box::new(KeepPseudonymCandidate),
                                                    cx,
                                                )
                                            },
                                        ),
                                        cx,
                                    ),
                                ),
                        )
                        .child(
                            ui::control(
                                "candidate-context",
                                if popup.context_open {
                                    "Hide source context"
                                } else {
                                    "Source context"
                                },
                                self.theme.get(),
                                true,
                            )
                            .map(|v| {
                                self.pseudonymization.reveal_popup_control(
                                    "candidate-context",
                                    v,
                                    cx,
                                )
                            })
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(popup) = &mut this.pseudonymization.popup {
                                    popup.context_open = !popup.context_open;
                                }
                                cx.notify();
                            })),
                        )
                        .when(popup.context_open, |v| {
                            v.child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(palette.header_muted)
                                    .child("Source fragment (includes hidden Markdown):"),
                            )
                            .child(
                                div()
                                    .py_1()
                                    .child(source_fragment(self.editor.read(cx).text(), &mention)),
                            )
                        })
                        .when(!anonymous && mappings.len() > 1, |v| {
                            v.child(
                                ui::control(
                                    "candidate-links",
                                    if popup.links_open {
                                        "Hide linking"
                                    } else {
                                        "Link to existing placeholder"
                                    },
                                    self.theme.get(),
                                    true,
                                )
                                .map(|v| {
                                    self.pseudonymization.reveal_popup_control(
                                        "candidate-links",
                                        v,
                                        cx,
                                    )
                                })
                                .on_click(cx.listener(
                                    |this, _, _, cx| {
                                        if let Some(popup) = &mut this.pseudonymization.popup {
                                            popup.links_open = !popup.links_open;
                                        }
                                        cx.notify();
                                    },
                                )),
                            )
                        })
                        .when(
                            !anonymous && popup.links_open && mappings.len() > 1,
                            |view| {
                                view.child(
                                    div()
                                        .id("pseudonym-links")
                                        .text_size(px(11.))
                                        .child("Link variant to existing placeholder:")
                                        .children(
                                            mappings
                                                .into_iter()
                                                .filter(|(_, replacement)| {
                                                    *replacement != group.replacement
                                                })
                                                .map(|(original, replacement)| {
                                                    div()
                                                        .id(gpui::SharedString::from(format!(
                                                            "link-{replacement}"
                                                        )))
                                                        .when(
                                                            cfg!(test)
                                                                && replacement == "PERSON_30",
                                                            |v| {
                                                                v.debug_selector(|| {
                                                                    "last-candidate-link".into()
                                                                })
                                                            },
                                                        )
                                                        .role(gpui::Role::Button)
                                                        .aria_label(format!(
                                                            "Link to {replacement}"
                                                        ))
                                                        .focus_visible(|s| {
                                                            s.bg(palette.placeholder_bg)
                                                        })
                                                        .key_context("UiControl")
                                                        .tab_index(0)
                                                        .map(|v| {
                                                            self.pseudonymization
                                                                .reveal_popup_control(
                                                                    gpui::SharedString::from(
                                                                        format!(
                                                                            "link-{replacement}"
                                                                        ),
                                                                    ),
                                                                    v,
                                                                    cx,
                                                                )
                                                        })
                                                        .py_1()
                                                        .cursor_pointer()
                                                        .child(format!(
                                                            "{replacement} · {original}"
                                                        ))
                                                        .on_click(cx.listener(
                                                            move |this, _, _, cx| {
                                                                this.pseudonymization.input.update(
                                                                    cx,
                                                                    |input, cx| {
                                                                        input.set_value(
                                                                            replacement.clone(),
                                                                            cx,
                                                                        )
                                                                    },
                                                                );
                                                            },
                                                        ))
                                                }),
                                        ),
                                )
                            },
                        ),
                ),
            )
            .into_any_element(),
        )
    }
}

fn source_fragment(source: &str, range: &Range<usize>) -> String {
    let start = source.floor_char_boundary(range.start.min(source.len()).saturating_sub(32));
    let end = source.ceil_char_boundary(range.end.saturating_add(32).min(source.len()));
    source[start..end].replace(['\n', '\r'], " ")
}
