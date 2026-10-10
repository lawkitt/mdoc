use super::*;

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

    pub(super) fn toolbar(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let copy = self.can_copy_markdown();
        let has_preview = self.preview.pdf.is_some() || self.preview.source.is_some();
        let commands = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_1()
            .min_w_0()
            .child(
                ui::icon_control("Open…", "Open…", ui::Icon::Open, theme, true)
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(Open), cx)),
            )
            .when(!self.source_only, |v| {
                v.child(
                    ui::icon_control("Save", "Save", ui::Icon::Save, theme, copy).on_click(
                        cx.listener(move |this, _, window, cx| {
                            if copy {
                                this.save(false, None, window, cx);
                            }
                        }),
                    ),
                )
                .child(
                    ui::icon_control("Save As…", "Save As…", ui::Icon::SaveAs, theme, copy)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if copy {
                                this.save(true, None, window, cx);
                            }
                        })),
                )
                .children(self.spelling_control(cx))
                .child(self.pii_toolbar_control(cx))
            })
            .when(
                self.source_only
                    && self.ocr_required.is_none()
                    && !self.auto_convert_pending
                    && !self.job.busy(),
                |v| v.child(button("Convert to Markdown", Import, theme)),
            );
        let utilities = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_1()
            .ml_auto()
            .when(has_preview, |v| {
                v.child(button(
                    if self.preview.visible {
                        "Hide original"
                    } else {
                        "Show original"
                    },
                    TogglePreview,
                    theme,
                ))
            })
            .child(
                ui::icon_control(
                    "theme-toggle",
                    if theme == Theme::Dark {
                        "Switch to light theme"
                    } else {
                        "Switch to dark theme"
                    },
                    if theme == Theme::Dark {
                        ui::Icon::Sun
                    } else {
                        ui::Icon::Moon
                    },
                    theme,
                    true,
                )
                .on_click(|_, window, cx| window.dispatch_action(Box::new(ToggleTheme), cx)),
            )
            .child({
                // An amber dot marks an available update (ADR 0038).
                let update = crate::updater::state(cx).available.is_some();
                let label = if update {
                    "Settings — update available"
                } else {
                    "Settings"
                };
                div()
                    .relative()
                    .child(
                        ui::icon_control("Settings", label, ui::Icon::Settings, theme, true)
                            .track_focus(&self.settings_focus)
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(Settings), cx)
                            }),
                    )
                    .when(update, |v| {
                        v.child(
                            div()
                                .when(cfg!(test), |v| v.debug_selector(|| "update-dot".into()))
                                .absolute()
                                .top(px(5.))
                                .right(px(5.))
                                .size(px(7.))
                                .rounded_full()
                                .bg(style::markdown_style(theme).alert_warning),
                        )
                    })
            });
        div()
            .id("document-toolbar")
            .when(cfg!(test), |v| {
                v.debug_selector(|| "document-toolbar".into())
            })
            .flex()
            .flex_wrap()
            .items_center()
            .gap_1()
            .min_h(px(40.))
            .flex_shrink_0()
            .px_2()
            .py_1()
            .border_b_1()
            .border_color(p.border)
            .text_size(px(13.))
            .child(commands)
            .child(utilities)
            .into_any_element()
    }

    /// Copy Markdown floats in the editor's top-right corner, beside the text it
    /// copies; it fades while there is no Markdown to copy.
    pub(super) fn copy_button(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = self.theme.get();
        let enabled = self.can_copy_markdown() && !self.editor.read(cx).text().is_empty();
        let (icon, text) = if self.copy_feedback.is_some() {
            (ui::Icon::Check, "Copied")
        } else {
            (ui::Icon::Copy, "Copy")
        };
        div()
            .absolute()
            .top(px(12.))
            .right(px(20.))
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
