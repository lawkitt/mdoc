use super::*;

/// The document header's one filled button (ADR 0048).
enum Primary {
    None,
    Convert,
    Pseudonymize,
    Scanning,
    Apply(usize),
    CopyForAi,
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
                    div().text_size(px(12.)).text_color(p.header_muted).child(
                        "PDF, DOCX or Markdown · select several to open each in its own tab",
                    ),
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
            return if self.ocr_required.is_none() && !self.auto_convert_pending && !self.job.busy() {
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
        if self.has_applied_aliases() {
            return Primary::CopyForAi;
        }
        if self.editor.read(cx).text().trim().is_empty() {
            return Primary::None;
        }
        Primary::Pseudonymize
    }

    fn primary_button(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let theme = self.theme.get();
        let copied = self.copy_feedback.is_some();
        let modifier = if cfg!(target_os = "macos") { "⌘" } else { "Ctrl+" };
        Some(match self.primary(cx) {
            Primary::None => return None,
            Primary::Scanning => self.scan_status(cx)?,
            Primary::Convert => ui::filled_button("primary-convert", "Convert to Markdown", theme, true)
                .when(cfg!(test), |v| v.debug_selector(|| "primary-convert".into()))
                .on_click(|_, window, cx| window.dispatch_action(Box::new(Import), cx))
                .into_any_element(),
            Primary::Pseudonymize => ui::filled_button("Pseudonymize", "Pseudonymize", theme, true)
                .when(cfg!(test), |v| v.debug_selector(|| "Pseudonymize".into()))
                .on_click(cx.listener(|this, _, window, cx| {
                    // Pseudonymize opens the panel (ADR 0048 decision 22).
                    this.toggle_pii_review(window, cx);
                    if !this.replacements_open() {
                        this.toggle_replacements_panel(window, cx);
                    }
                }))
                .into_any_element(),
            Primary::Apply(n) => ui::filled_button(
                "primary-apply",
                format!("Apply {n}"),
                theme,
                true,
            )
            .when(cfg!(test), |v| v.debug_selector(|| "primary-apply".into()))
            .child(ui::key_hint(format!("{modifier}↩")))
            .on_click(|_, window, cx| window.dispatch_action(Box::new(PiiApplyAll), cx))
            .into_any_element(),
            Primary::CopyForAi => ui::filled_button(
                "Copy for AI",
                if copied && self.copied_for_ai { "Copied" } else { "Copy for AI" },
                theme,
                true,
            )
            .when(cfg!(test), |v| v.debug_selector(|| "Copy for AI".into()))
            .min_w(px(104.))
            .on_click(cx.listener(|this, _, window, cx| this.copy_for_ai(&CopyForAi, window, cx)))
            .into_any_element(),
            Primary::Copy => ui::filled_button(
                "primary-copy",
                if copied { "Copied" } else { "Copy" },
                theme,
                true,
            )
            .when(cfg!(test), |v| v.debug_selector(|| "primary-copy".into()))
            .min_w(px(72.))
            .on_click(cx.listener(|this, _, window, cx| this.copy_markdown(&CopyMarkdown, window, cx)))
            .into_any_element(),
        })
    }

    /// The title-bar row above the document (ADR 0048): breadcrumb left;
    /// Source, panel toggle, ⋯ and the primary action right.
    pub(super) fn header(&mut self, window: &mut Window, cx: &mut Context<Self>) -> gpui::AnyElement {
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
                v.child(div().size(px(7.)).rounded_full().flex_shrink_0().bg(link.color()))
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
            .when(has_preview && self.compare.is_none(), |v| {
                v.child(
                    ui::header_icon(
                        "header-source",
                        if self.preview.visible { "Hide the original file" } else { "Show the original file" },
                        ui::Icon::Source,
                        theme,
                        self.preview.visible,
                    )
                    .when(cfg!(test), |v| v.debug_selector(|| "header-source".into()))
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(TogglePreview), cx)),
                )
            })
            .when(reviewable, |v| {
                v.child(
                    ui::header_icon(
                        "header-panel",
                        if self.replacements_open() { "Hide replacements" } else { "Show replacements" },
                        ui::Icon::PanelRight,
                        theme,
                        self.replacements_open(),
                    )
                    .when(cfg!(test), |v| v.debug_selector(|| "header-panel".into()))
                    .on_click(cx.listener(|this, _, window, cx| this.toggle_replacements_panel(window, cx))),
                )
            })
            .child(self.more_button(cx))
            .children(self.primary_button(cx).map(|b| div().ml_1().child(b)));
        ui::title_drag(
            div()
                .when(cfg!(test), |v| v.debug_selector(|| "document-toolbar".into()))
                .flex()
                .items_center()
                .gap_2()
                .h(px(style::HEADER_HEIGHT))
                .flex_shrink_0()
                .pl(px(16. + self.title.inset.get()))
                .pr_2()
                .border_b_1()
                .border_color(p.border),
            self.title.armed.clone(),
        )
        .child(crumb)
        .child(div().flex_1().min_w_0())
        .child(ui::no_drag(controls))
        .children(ui::caption_buttons(window, theme))
        .into_any_element()
    }

    fn more_button(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = self.theme.get();
        let open = self.more_open;
        let menu = open.then(|| self.more_menu(cx));
        div()
            .relative()
            .child(
                ui::header_icon("header-more", "More actions", ui::Icon::More, theme, open)
                    .when(cfg!(test), |v| v.debug_selector(|| "header-more".into()))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.more_open = !this.more_open;
                        cx.notify();
                    })),
            )
            .children(menu)
            .into_any_element()
    }

    fn more_menu(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = self.theme.get();
        let modifier = |mac: &'static str, other: &'static str| if cfg!(target_os = "macos") { mac } else { other };
        let copy = self.can_copy_markdown() && !self.source_only;
        let aliases = copy && self.has_applied_aliases();
        let spelling_on = !self.spelling_control_off() && !self.spelling.off_here;
        let misspelled = self.editor.read(cx).diagnostics().len();
        let run = |this: &mut Self, window: &mut Window, cx: &mut Context<Self>, action: Box<dyn gpui::Action>| {
            this.more_open = false;
            window.dispatch_action(action, cx);
            cx.notify();
        };
        let row = |id: &'static str, label: String, shortcut: Option<&'static str>, checked: Option<bool>, enabled: bool| {
            ui::menu_row(id, label, shortcut, checked, theme, enabled)
                .when(cfg!(test), move |v| v.debug_selector(move || id.into()))
        };
        gpui::deferred(
            gpui::anchored()
                .anchor(gpui::Anchor::TopRight)
                .snap_to_window_with_margin(px(8.))
                .child(
                    ui::panel("more-menu", theme)
                        .mt(px(32.))
                        .p_1()
                        .rounded_lg()
                        .shadow_lg()
                        .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                            this.more_open = false;
                            cx.notify();
                        }))
                        .child(row("more-save", "Save".into(), Some(modifier("⌘S", "Ctrl+S")), None, copy)
                            .on_click(cx.listener(move |this, _, window, cx| run(this, window, cx, Box::new(Save)))))
                        .child(row("more-save-as", "Save as…".into(), Some(modifier("⇧⌘S", "Ctrl+Shift+S")), None, copy)
                            .on_click(cx.listener(move |this, _, window, cx| run(this, window, cx, Box::new(SaveAs)))))
                        .child(ui::menu_divider(theme))
                        .child(row("more-copy-markdown", "Copy Markdown".into(), None, None, copy)
                            .on_click(cx.listener(move |this, _, window, cx| run(this, window, cx, Box::new(CopyMarkdown)))))
                        .child(row("more-copy-for-ai", "Copy for AI".into(), None, None, aliases)
                            .on_click(cx.listener(move |this, _, window, cx| run(this, window, cx, Box::new(CopyForAi)))))
                        .child(row("more-paste-reply", "Paste AI reply…".into(), Some(modifier("⌥⇧⌘V", "Ctrl+Alt+Shift+V")), None, aliases)
                            .on_click(cx.listener(move |this, _, window, cx| run(this, window, cx, Box::new(PasteAiResponse)))))
                        .child(ui::menu_divider(theme))
                        .child(row(
                            "more-spelling",
                            if spelling_on && misspelled > 0 { format!("Spelling · {misspelled}") } else { "Spelling".into() },
                            None,
                            Some(spelling_on),
                            !self.spelling_control_off(),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| run(this, window, cx, Box::new(ToggleDocumentSpelling)))))
                        .child(row("more-rescan", "Rescan".into(), None, None, copy && !self.pii.scanning())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.more_open = false;
                                this.start_pii_scan(cx);
                                window.refresh();
                            })))
                        .child(ui::menu_divider(theme))
                        .child(row("more-close", "Close tab".into(), Some(modifier("⌘W", "Ctrl+W")), None, true)
                            .on_click(cx.listener(move |this, _, window, cx| run(this, window, cx, Box::new(Close))))),
                ),
        )
        .with_priority(1)
        .into_any_element()
    }

    /// Copy Markdown floats in the editor's top-right corner, beside the text it
    /// copies; it fades while there is no Markdown to copy.
    pub(super) fn copy_button(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = self.theme.get();
        let enabled = self.can_copy_markdown() && !self.editor.read(cx).text().is_empty();
        let copied = self.copy_feedback.is_some();
        let (icon, text) = if copied && !self.copied_for_ai {
            (ui::Icon::Check, "Copied")
        } else {
            (ui::Icon::Copy, "Copy")
        };
        // With aliases in the text, Copy for AI adds the placeholder notice (ADR 0041).
        let for_ai = enabled && self.has_applied_aliases();
        div()
            .absolute()
            .top(px(12.))
            .right(px(20.))
            .flex()
            .gap_2()
            .when(for_ai, |v| {
                v.child(
                    ui::floating_button(
                        "Copy for AI",
                        "Copy with a note asking the AI to keep placeholders",
                        if copied && self.copied_for_ai {
                            ui::Icon::Check
                        } else {
                            ui::Icon::Copy
                        },
                        if copied && self.copied_for_ai {
                            "Copied"
                        } else {
                            "Copy for AI"
                        },
                        theme,
                        true,
                    )
                    .min_w(px(122.))
                    .when(cfg!(test), |v| v.debug_selector(|| "Copy for AI".into()))
                    .on_click(
                        cx.listener(|this, _, window, cx| this.copy_for_ai(&CopyForAi, window, cx)),
                    ),
                )
            })
            .child(
                ui::floating_button("Copy Markdown", "Copy Markdown", icon, text, theme, enabled)
                    // Wide enough for "Copied" so feedback does not resize the button.
                    .min_w(px(92.))
                    .when(cfg!(test), |v| v.debug_selector(|| "Copy Markdown".into()))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if enabled {
                            this.copy_markdown(&CopyMarkdown, window, cx)
                        }
                    })),
            )
            .into_any_element()
    }

    pub(super) fn pane_switch(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = self.theme.get();
        div()
            .flex()
            .gap_1()
            .px_2()
            .py_1()
            .flex_shrink_0()
            .border_b_1()
            .border_color(theme.pdf_style().border)
            .children([(false, "Markdown"), (true, "Original")].into_iter().map(
                |(original, label)| {
                    ui::control(label, label, theme, true)
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
