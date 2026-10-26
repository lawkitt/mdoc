use super::DocumentView;
use crate::{
    document_view::{CopyMarkdown, Save, SaveAs, TogglePreview},
    pii::ui::{CopyForAi, PiiApplyAll},
    spelling::ToggleDocumentSpelling,
    style, ui,
    workspace::{Close, Import, Open, PasteAiResponse},
};
use gpui::{App, Context, Focusable, Window, div, prelude::*, px};

/// The document header's one filled button (ADR 0048).
enum Primary {
    None,
    Convert,
    Pseudonymize,
    Scanning,
    Apply(usize),
    /// Applied: the header opens the Inspector's Share tab.
    Share,
    Copy,
}

impl DocumentView {
    pub(crate) fn chrome_width(&self, window: &Window) -> f32 {
        let width = f32::from(self.view_bounds.get().size.width);
        if width > 0. {
            width
        } else {
            f32::from(window.viewport_size().width) - 40.
        }
    }

    /// The centered start view over an empty Markdown tab (ADR 0028). It has no
    /// hitbox of its own, so clicks outside the button reach the editor.
    pub(super) fn empty_page(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let accent = theme.search_accent();
        let shortcut = if cfg!(target_os = "macos") {
            "⌘O"
        } else {
            "Ctrl+O"
        };
        let body = match self.file_drag {
            Some(true) => div()
                .text_size(px(15.))
                .text_color(accent)
                .child("Drop to open")
                .into_any_element(),
            Some(false) => div()
                .text_size(px(15.))
                .text_color(p.header_muted)
                .child("No supported files")
                .into_any_element(),
            None => div()
                .flex()
                .flex_col()
                .items_center()
                .gap_2()
                .child(
                    ui::primary_button("empty-page-open", "Open files…", theme, true)
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_4()
                        .py_2()
                        .text_size(px(14.))
                        .child(div().opacity(0.7).child(shortcut))
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "empty-page-open".into())
                        })
                        .on_click(cx.listener(|this, _, window, cx| this.open(&Open, window, cx))),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .text_center()
                        .px_4()
                        .text_size(px(12.))
                        .text_color(p.header_muted)
                        .child(crate::import::supported_formats())
                        .child("Select several to open each in its own tab"),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(p.header_muted)
                        .opacity(0.7)
                        .child("or just start typing"),
                )
                .child(local_note(theme))
                .into_any_element(),
        };
        div()
            .absolute()
            .inset_0()
            .m_2()
            .rounded_lg()
            .flex()
            .items_center()
            .justify_center()
            .when(cfg!(test), |v| v.debug_selector(|| "empty-page".into()))
            .when(self.file_drag == Some(true), |v| {
                v.border_2().border_dashed().border_color(accent)
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_3()
                    .max_w(px(420.))
                    .px_4()
                    .text_center()
                    .child(
                        gpui::svg()
                            .data(include_bytes!("../../resources/ui/document.svg"))
                            .size(px(40.))
                            .text_color(if self.file_drag == Some(true) {
                                accent
                            } else {
                                p.header_muted
                            }),
                    )
                    .child(body),
            )
            .into_any_element()
    }

    /// The header's state-driven primary action (ADR 0048).
    fn primary(&self, cx: &App) -> Primary {
        if self.source_only {
            return if self.ocr_required.is_none() && !self.auto_convert_pending && !self.job.busy()
            {
                Primary::Convert
            } else {
                Primary::None
            };
        }
        if self.loading || self.unavailable || !self.can_copy_markdown() {
            return Primary::None;
        }
        if self.pii.restore.is_some() {
            return Primary::Copy;
        }
        if self.pii.scanning() {
            return Primary::Scanning;
        }
        let remaining = self.pii.review.remaining();
        if self.pii.reviewing && remaining > 0 {
            return Primary::Apply(remaining);
        }
        // Inspector (ADR 0048 variant C): sharing lives in the Share tab.
        if self.has_applied_aliases() {
            return Primary::Share;
        }
        if self.editor.read(cx).text().trim().is_empty() {
            return Primary::None;
        }
        Primary::Pseudonymize
    }

    fn primary_button(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let theme = self.theme.get();
        let copied = self.copy_feedback.is_some();
        let modifier = if cfg!(target_os = "macos") {
            "⌘"
        } else {
            "Ctrl+"
        };
        Some(match self.primary(cx) {
            Primary::None => return None,
            Primary::Scanning => self.scan_status(cx)?,
            Primary::Convert => {
                ui::filled_button("primary-action", "Convert to Markdown", theme, true)
                    .when(cfg!(test), |v| {
                        v.debug_selector(|| "primary-convert".into())
                    })
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(Import), cx))
                    .into_any_element()
            }
            Primary::Pseudonymize => {
                ui::filled_button("primary-action", "Pseudonymize", theme, true)
                    .when(cfg!(test), |v| v.debug_selector(|| "Pseudonymize".into()))
                    .on_click(cx.listener(|this, _, window, cx| {
                        // Pseudonymize scans once and opens the panel; it never
                        // closes it (ADR 0048 decision 22).
                        let review = &this.pii.review;
                        let fresh = !this.pii.reviewing
                            && review.variants().is_empty()
                            && review.applied().is_empty();
                        if fresh {
                            this.start_pii_scan(cx);
                        }
                        if !this.replacements_open() {
                            this.toggle_replacements_panel(window, cx);
                        }
                    }))
                    .into_any_element()
            }
            Primary::Apply(n) => {
                // Keep the element's generated focus handle when Apply becomes
                // Share (and back on Undo), so chrome shortcuts still dispatch.
                ui::filled_button("primary-action", format!("Apply {n}"), theme, true)
                    .when(cfg!(test), |v| v.debug_selector(|| "primary-apply".into()))
                    .child(ui::key_hint(format!("{modifier}↩")))
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(PiiApplyAll), cx))
                    .into_any_element()
            }
            Primary::Share => ui::primary_button("primary-action", "Share", theme, true)
                .when(cfg!(test), |v| v.debug_selector(|| "primary-share".into()))
                .h(px(28.))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.inspector_share = true;
                    if !this.replacements_open() {
                        this.toggle_replacements_panel(window, cx);
                    }
                    cx.notify();
                }))
                .into_any_element(),
            Primary::Copy => ui::filled_button(
                "primary-action",
                if copied { "Copied" } else { "Copy" },
                theme,
                true,
            )
            .when(cfg!(test), |v| v.debug_selector(|| "primary-copy".into()))
            .min_w(px(72.))
            .on_click(
                cx.listener(|this, _, window, cx| this.copy_markdown(&CopyMarkdown, window, cx)),
            )
            .into_any_element(),
        })
    }

    /// The title-bar row above the document (ADR 0048): breadcrumb left;
    /// Source, panel toggle, ⋯ and the primary action right.
    pub(super) fn header(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let has_preview = self.preview.pdf.is_some() || self.preview.source.is_some();
        let reviewable = !self.source_only
            && self.pii.restore.is_none()
            && (self.pii.reviewing
                || self.replacements_open()
                || !self.pii.review.variants().is_empty()
                || !self.pii.review.applied().is_empty());
        let name = self.display_name();
        let dirty = self.dirty_cached;
        let crumb = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .min_w_0()
            .flex_shrink(1.)
            .text_size(px(13.))
            .when_some(self.case.clone(), |v, link| {
                v.child(
                    div()
                        .size(px(7.))
                        .rounded_full()
                        .flex_shrink_0()
                        .bg(style::case_color(link.hue())),
                )
                .child(
                    div()
                        .flex_shrink(1.)
                        .min_w(px(24.))
                        .truncate()
                        .text_color(p.header_muted)
                        .child(link.name.clone()),
                )
                .child(
                    gpui::svg()
                        .data(ui::Icon::ChevronRight.data())
                        .size(px(12.))
                        .flex_shrink_0()
                        .text_color(p.header_muted),
                )
            })
            .when(self.pii.restore.is_some(), |v| {
                v.child(
                    gpui::svg()
                        .data(ui::Icon::Reply.data())
                        .size(px(14.))
                        .flex_shrink_0()
                        .text_color(p.header_muted),
                )
            })
            .child(
                div()
                    .min_w(px(40.))
                    .truncate()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child(name),
            )
            .when(dirty, |v| {
                v.child(
                    div()
                        .when(cfg!(test), |v| v.debug_selector(|| "header-dirty".into()))
                        .size(px(6.))
                        .flex_shrink_0()
                        .rounded_full()
                        .bg(p.header_muted),
                )
            });
        let controls = div()
            .flex()
            .items_center()
            .gap_1()
            .flex_shrink_0()
            .when(reviewable, |v| {
                v.child(
                    ui::header_icon(
                        "header-panel",
                        if self.replacements_open() {
                            "Hide replacements"
                        } else {
                            "Show replacements"
                        },
                        ui::Icon::PanelRight,
                        theme,
                        self.replacements_open(),
                    )
                    .when(cfg!(test), |v| v.debug_selector(|| "header-panel".into()))
                    .on_click(
                        cx.listener(|this, _, window, cx| {
                            this.toggle_replacements_panel(window, cx)
                        }),
                    ),
                )
            })
            .when(has_preview && self.compare.is_none(), |v| {
                v.child(
                    ui::header_icon(
                        "header-source",
                        if self.preview.visible {
                            "Hide the original file"
                        } else {
                            "Show the original file"
                        },
                        ui::Icon::Source,
                        theme,
                        self.preview.visible,
                    )
                    .when(cfg!(test), |v| v.debug_selector(|| "header-source".into()))
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(TogglePreview), cx)),
                )
            })
            .children(ui::notice_bell(&self.title, theme))
            .child(self.more_button(cx))
            .children(self.primary_button(cx).map(|b| div().ml_1().child(b)));
        let detached = self.title.detached.get();
        let bar = ui::title_drag(
            div()
                .when(cfg!(test), |v| {
                    v.debug_selector(|| "document-toolbar".into())
                })
                .flex()
                .items_center()
                .gap_2()
                .h(px(style::HEADER_HEIGHT))
                .flex_shrink_0()
                .pl(px(16. + self.title.inset.get()))
                .pr_2()
                // In the title row (sidebar collapsed) it sits on the window
                // ground, level with the traffic lights; in the card it has a rule.
                .when(detached, |v| {
                    v.h(px(style::CARD_MARGIN + style::HEADER_HEIGHT))
                        .pt(px(style::CARD_MARGIN))
                })
                .when(!detached, |v| v.border_b_1().border_color(p.border)),
            self.title.armed.clone(),
        )
        .child(crumb)
        .child(div().flex_1().min_w_0())
        .child(ui::no_drag(controls));
        ui::title_bar(bar, (!detached).then_some(p.border), window, theme)
    }

    /// Close the More menu, focus back on its button (ADR 0062).
    pub(crate) fn close_more(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.more_open = false;
        window.focus(&self.more_focus, cx);
        cx.notify();
    }

    fn more_button(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = self.theme.get();
        let open = self.more_open;
        let menu = open.then(|| self.more_menu(cx));
        let bounds = self.more_bounds.clone();
        div()
            .relative()
            .child(
                gpui::canvas(move |b, _, _| bounds.set(b), |_, _, _, _| {})
                    .absolute()
                    .inset_0(),
            )
            .child(
                ui::header_icon("header-more", "More actions", ui::Icon::More, theme, open)
                    .track_focus(&self.more_focus)
                    .when(cfg!(test), |v| v.debug_selector(|| "header-more".into()))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.more_open = !this.more_open;
                        // The menu takes focus, so Tab and Esc work in it.
                        if this.more_open {
                            window.focus(&this.more_menu_focus, cx);
                        }
                        cx.notify();
                    })),
            )
            .children(menu)
            .into_any_element()
    }

    /// Why Copy for AI and Paste AI reply are off, or `None` when they
    /// work. The menu shows it and screen readers hear it.
    pub(crate) fn alias_actions_reason(&self) -> Option<&'static str> {
        if self.source_only {
            Some("Convert the document to Markdown first.")
        } else if !self.can_copy_markdown() {
            Some("Available once the document has loaded.")
        } else if self.has_applied_aliases() {
            None
        } else if self.pii.review.variants().is_empty() {
            Some("Pseudonymize the document first.")
        } else if self.pii.review.remaining() > 0 {
            Some("Apply replacements first.")
        } else {
            // Every proposal was kept as the original.
            Some("No names in this document are replaced.")
        }
    }

    fn more_menu(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = self.theme.get();
        let modifier = |mac: &'static str, other: &'static str| {
            if cfg!(target_os = "macos") {
                mac
            } else {
                other
            }
        };
        let copy = self.can_copy_markdown() && !self.source_only;
        let has_text = copy && !self.editor.read(cx).text().is_empty();
        let alias_reason = self.alias_actions_reason();
        let aliases = copy && alias_reason.is_none();
        let spelling_on = !self.spelling_control_off() && !self.spelling.off_here;
        let misspelled = self.editor.read(cx).diagnostics().len();
        // A menu choice hands focus back to the document first, so the action
        // and the keys after it land there.
        let run = |this: &mut Self,
                   window: &mut Window,
                   cx: &mut Context<Self>,
                   action: Box<dyn gpui::Action>| {
            this.more_open = false;
            if this.source_only {
                window.focus(&this.focus, cx);
            } else {
                window.focus(&this.editor.read(cx).focus_handle(cx), cx);
            }
            window.dispatch_action(action, cx);
            cx.notify();
        };
        // Disabled rows stay visible but do nothing.
        let guard = |enabled: bool, action: fn() -> Box<dyn gpui::Action>| {
            move |this: &mut Self,
                  _: &gpui::ClickEvent,
                  window: &mut Window,
                  cx: &mut Context<Self>| {
                if enabled {
                    run(this, window, cx, action());
                }
            }
        };
        let row = |id: &'static str,
                   label: String,
                   shortcut: Option<&'static str>,
                   checked: Option<bool>,
                   enabled: bool| {
            ui::menu_row(id, label, shortcut, checked, theme, enabled)
                .when(cfg!(test), move |v| v.debug_selector(move || id.into()))
        };
        gpui::deferred(
            gpui::anchored()
                .anchor(gpui::Anchor::TopRight)
                .snap_to_window_with_margin(px(8.))
                .child(
                    ui::panel("more-menu", theme)
                        .track_focus(&self.more_menu_focus)
                        .key_context("UiPanel UiMenu")
                        .on_action(cx.listener(|this, _: &ui::NextControl, window, cx| {
                            ui::cycle(window, cx, Some(&this.more_menu_focus), false);
                            cx.stop_propagation();
                        }))
                        .on_action(cx.listener(|this, _: &ui::PreviousControl, window, cx| {
                            ui::cycle(window, cx, Some(&this.more_menu_focus), true);
                            cx.stop_propagation();
                        }))
                        .on_action(cx.listener(|this, _: &ui::CloseMenu, window, cx| {
                            this.close_more(window, cx);
                            cx.stop_propagation();
                        }))
                        .mt(px(32.))
                        .p_1()
                        .rounded_lg()
                        .shadow_lg()
                        .on_mouse_down_out(cx.listener(
                            |this, event: &gpui::MouseDownEvent, window, cx| {
                                // A press on the More button is its toggle's.
                                if this.more_bounds.get().contains(&event.position) {
                                    return;
                                }
                                this.more_open = false;
                                // Unless the click focused something, focus goes
                                // back to the button rather than the closed menu.
                                let menu = this.more_menu_focus.clone();
                                let button = this.more_focus.clone();
                                window.defer(cx, move |window, cx| {
                                    if menu.contains_focused(window, cx) {
                                        window.focus(&button, cx);
                                    }
                                });
                                cx.notify();
                            },
                        ))
                        .child(
                            row(
                                "more-save",
                                "Save".into(),
                                Some(modifier("⌘S", "Ctrl+S")),
                                None,
                                copy,
                            )
                            .on_click(cx.listener(guard(copy, || Box::new(Save)))),
                        )
                        .child(
                            row(
                                "more-save-as",
                                "Save as…".into(),
                                Some(modifier("⇧⌘S", "Ctrl+Shift+S")),
                                None,
                                copy,
                            )
                            .on_click(cx.listener(guard(copy, || Box::new(SaveAs)))),
                        )
                        .child(ui::menu_divider(theme))
                        .child(
                            row(
                                "more-copy-markdown",
                                "Copy Markdown".into(),
                                None,
                                None,
                                has_text,
                            )
                            .on_click(cx.listener(guard(has_text, || Box::new(CopyMarkdown)))),
                        )
                        .child(
                            ui::menu_row_because(
                                "more-copy-for-ai",
                                "Copy for AI",
                                None,
                                None,
                                theme,
                                aliases,
                                alias_reason.map(Into::into),
                            )
                            .when(cfg!(test), |v| {
                                v.debug_selector(|| "more-copy-for-ai".into())
                            })
                            .on_click(cx.listener(guard(aliases, || Box::new(CopyForAi)))),
                        )
                        .child(
                            ui::menu_row_because(
                                "more-paste-reply",
                                "Paste AI reply…",
                                Some(modifier("⌥⇧⌘V", "Ctrl+Alt+Shift+V")),
                                None,
                                theme,
                                aliases,
                                alias_reason.map(Into::into),
                            )
                            .when(cfg!(test), |v| {
                                v.debug_selector(|| "more-paste-reply".into())
                            })
                            .on_click(cx.listener(guard(aliases, || Box::new(PasteAiResponse)))),
                        )
                        .child(ui::menu_divider(theme))
                        .child(
                            row(
                                "more-spelling",
                                if spelling_on && misspelled > 0 {
                                    format!("Spelling · {misspelled}")
                                } else {
                                    "Spelling".into()
                                },
                                None,
                                Some(spelling_on),
                                !self.spelling_control_off(),
                            )
                            .on_click(
                                cx.listener(guard(!self.spelling_control_off(), || {
                                    Box::new(ToggleDocumentSpelling)
                                })),
                            ),
                        )
                        .child(
                            row(
                                "more-rescan",
                                "Rescan".into(),
                                None,
                                None,
                                copy && !self.pii.scanning(),
                            )
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    // Focus back to the document, as other rows do.
                                    this.more_open = false;
                                    if this.source_only {
                                        window.focus(&this.focus, cx);
                                    } else {
                                        window.focus(&this.editor.read(cx).focus_handle(cx), cx);
                                    }
                                    this.start_pii_scan(cx);
                                    window.refresh();
                                },
                            )),
                        )
                        .child(ui::menu_divider(theme))
                        .child(
                            row(
                                "more-close",
                                "Close tab".into(),
                                Some(modifier("⌘W", "Ctrl+W")),
                                None,
                                true,
                            )
                            .on_click(cx.listener(guard(true, || Box::new(Close)))),
                        ),
                ),
        )
        .with_priority(1)
        .into_any_element()
    }

    pub(super) fn pane_switch(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = self.theme.get();
        // One choice of two panes: a radio group, not two buttons (ADR 0064).
        div()
            .id("pane-switch")
            .role(gpui::Role::RadioGroup)
            .aria_label("View")
            .flex()
            .gap_1()
            .px_2()
            .py_1()
            .flex_shrink_0()
            .border_b_1()
            .border_color(theme.pdf_style().border)
            .children([(false, "Markdown"), (true, "Source")].into_iter().map(
                |(original, label)| {
                    ui::control(label, label, theme, true)
                        .role(gpui::Role::RadioButton)
                        .aria_toggled(if self.original_selected == original {
                            gpui::Toggled::True
                        } else {
                            gpui::Toggled::False
                        })
                        .when(cfg!(test), |v| v.debug_selector(move || label.into()))
                        .when(self.original_selected == original, |v| {
                            v.bg(theme.sidebar_selected())
                                .text_color(theme.search_accent())
                        })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.original_selected = original;
                            if original {
                                if let Some(pdf) = &this.preview.pdf {
                                    window.focus(&pdf.read(cx).focus_handle(cx), cx);
                                }
                            } else if !this.source_only {
                                window.focus(&this.editor.read(cx).focus_handle(cx), cx);
                            }
                            cx.notify();
                        }))
                },
            ))
            .into_any_element()
    }
}

/// "Documents stay on this Mac", explained on hover (ADR 0046). Literally
/// true: the only network use is the update check and chosen model downloads.
fn local_note(theme: crate::style::Theme) -> impl IntoElement {
    let p = theme.pdf_style();
    let machine = if cfg!(target_os = "macos") {
        "this Mac"
    } else {
        "this computer"
    };
    let detail = format!(
        "Detection, conversion and saved cases run on {machine}. mdoc goes online only \
         to check for updates and to download models you choose."
    );
    div()
        .id("empty-page-local")
        .when(cfg!(test), |v| {
            v.debug_selector(|| "empty-page-local".into())
        })
        .mt_3()
        .flex()
        .items_center()
        .gap_1()
        .text_size(px(12.))
        .text_color(p.header_muted)
        .aria_label(format!("Documents stay on {machine}. {detail}"))
        .tooltip(crate::style::tooltip(detail, theme))
        .child(
            gpui::svg()
                .data(include_bytes!("../../resources/ui/lock.svg"))
                .size(px(13.))
                .text_color(p.header_muted),
        )
        .child(format!("Documents stay on {machine}"))
}
