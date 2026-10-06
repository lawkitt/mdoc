use super::*;

impl Workspace {
    pub(super) fn chrome_width(&self, window: &Window) -> f32 {
        let width = f32::from(self.workspace_bounds.get().size.width);
        if width > 0. {
            width
        } else {
            f32::from(window.viewport_size().width) - 40.
        }
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
                .child(
                    ui::control("Pseudonymize", "Pseudonymize", theme, copy)
                        .when(cfg!(test), |v| v.debug_selector(|| "Pseudonymize".into()))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if copy {
                                this.pseudonymize(&Pseudonymize, window, cx);
                            }
                        })),
                )
                .child(
                    ui::control(
                        "Copy Markdown",
                        if self.copy_feedback.is_some() {
                            "Copied"
                        } else {
                            "Copy Markdown"
                        },
                        theme,
                        copy,
                    )
                    .bg(theme.sidebar_selected())
                    .text_color(theme.search_accent())
                    .when(cfg!(test), |v| v.debug_selector(|| "Copy Markdown".into()))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.copy_markdown(&CopyMarkdown, window, cx)
                    })),
                )
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
            .child(
                ui::icon_control("Settings", "Settings", ui::Icon::Settings, theme, true)
                    .track_focus(&self.settings_focus)
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(Settings), cx)),
            );
        div()
            .id("workspace-toolbar")
            .when(cfg!(test), |v| {
                v.debug_selector(|| "workspace-toolbar".into())
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
