//! A locked document's password cards: on the conversion card, and in the
//! Original pane when the tab holds no unlock for it.
use crate::document::{DocumentView, import_flow, markdown_search, text};
use crate::ui;
use gpui::{Context, Entity, Focusable, Window, div, prelude::*, px};

impl DocumentView {
    /// The conversion card for a locked document: its password, asked for
    /// first or again after a wrong one. Unlocking converts it in memory.
    pub(in crate::document) fn password_card(
        &mut self,
        card: gpui::Stateful<gpui::Div>,
        prompt: import_flow::PasswordPrompt,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let field_focus = self.password_field.read(cx).focus_handle(cx);
        if std::mem::take(&mut self.focus_card)
            && (self.focus.contains_focused(window, cx) || window.focused(cx).is_none())
        {
            window.defer(cx, move |window, cx| window.focus(&field_focus, cx));
        }
        self.password_form(
            card,
            text::PASSWORD_TO_CONVERT,
            prompt == import_flow::PasswordPrompt::Wrong,
            self.password_field.clone(),
            ("document-password", "unlock-document"),
            self.import.can_unlock(),
            Self::unlock_document,
            cx,
        )
    }

    /// The Original's card for a locked DOCX this tab holds no unlock for,
    /// a reopened one (#1024 decision 6): its password shows the Original,
    /// and the unlock stays with the tab. While the conversion card asks for
    /// the same password, the Original only points to it.
    pub(in crate::document) fn original_password_card(
        &mut self,
        password: crate::document::preview::OriginalPassword,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let theme = self.theme.get();
        if self.lifecycle.source_only() && self.import.password_prompt().is_some() {
            return div()
                .id("original-password-pending")
                .max_w(px(360.))
                .px_6()
                .text_center()
                .text_size(px(14.))
                .child(text::PASSWORD_PENDING)
                .into_any_element();
        }
        let field_focus = self.original_password_field.read(cx).focus_handle(cx);
        if std::mem::take(&mut self.focus_original_card) {
            window.defer(cx, move |window, cx| window.focus(&field_focus, cx));
        }
        let card = ui::state_card("original-password-card", theme)
            .when(cfg!(test), |v| {
                v.debug_selector(|| "original-password-card".into())
            })
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(theme.pdf_style().header_muted)
                    .text_ellipsis()
                    .child(
                        self.preview
                            .source()
                            .and_then(|path| path.file_name())
                            .map(|name| name.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                    ),
            );
        div()
            .w_full()
            .px_6()
            .flex()
            .justify_center()
            .child(self.password_form(
                card,
                text::PASSWORD_TO_SHOW,
                password == crate::document::preview::OriginalPassword::Wrong,
                self.original_password_field.clone(),
                ("original-password", "unlock-original"),
                password != crate::document::preview::OriginalPassword::Unlocking,
                Self::unlock_original,
                cx,
            ))
            .into_any_element()
    }

    /// A locked document's password card: `body`, the wrong-password line,
    /// and `field` with an Unlock button (`ids`); Enter unlocks too.
    #[allow(clippy::too_many_arguments)]
    fn password_form(
        &self,
        card: gpui::Stateful<gpui::Div>,
        body: &'static str,
        wrong: bool,
        field: Entity<markdown_search::SearchInput>,
        ids: (&'static str, &'static str),
        enabled: bool,
        unlock: fn(&mut Self, &mut Window, &mut Context<Self>),
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let theme = self.theme.get();
        let (field_id, button_id) = ids;
        card.child(ui::card_title(text::PASSWORD_PROTECTED))
            .child(ui::card_text(body, theme))
            .when(wrong, |v| {
                v.child(ui::card_error(
                    mdoc_import::import::ImportError::WrongPassword.to_string(),
                    theme,
                ))
            })
            .child(
                ui::card_actions()
                    .flex_nowrap()
                    .child(
                        div()
                            .id(field_id)
                            .when(cfg!(test), |v| v.debug_selector(move || field_id.into()))
                            .flex_1()
                            .min_w_0()
                            .border_1()
                            .rounded_md()
                            .border_color(theme.pdf_style().border)
                            .bg(theme.pdf_style().bg)
                            .px_2()
                            .py_1()
                            // Enter unlocks, as the button does.
                            .on_key_down(cx.listener(
                                move |this, event: &gpui::KeyDownEvent, window, cx| {
                                    let keystroke = &event.keystroke;
                                    if keystroke.key == "enter" && !keystroke.modifiers.modified() {
                                        cx.stop_propagation();
                                        unlock(this, window, cx);
                                    }
                                },
                            ))
                            .child(field),
                    )
                    .child(
                        ui::primary_button(button_id, text::UNLOCK, theme, enabled)
                            .when(cfg!(test), |v| v.debug_selector(move || button_id.into()))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                unlock(this, window, cx);
                            })),
                    ),
            )
            .into_any_element()
    }
}
