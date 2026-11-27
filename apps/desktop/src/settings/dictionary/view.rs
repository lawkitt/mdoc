//! The dictionary as drawn: Spelling's summary line, and the searchable
//! list that replaces the page while it is open.
use super::{AddDictionaryWord, Typed};
use crate::settings::{Panel, text, view::quiet_control};
use crate::ui;
use crate::ui::style::{self, Theme};
use gpui::{AnyElement, Context, div, prelude::*, px, uniform_list};
use std::rc::Rc;

impl Panel {
    /// The Settings → Spelling summary line with Manage….
    pub(in crate::settings) fn dictionary_summary(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (count, error) = {
            let store = self.shared.borrow();
            (store.words.words.len(), store.words.error.clone())
        };
        let summary = match count {
            0 => text::NO_WORDS_YET.to_string(),
            n => text::words(n),
        };
        div()
            .flex()
            .flex_col()
            .child(
                self.setting_row(
                    text::YOUR_DICTIONARY,
                    summary,
                    self.scrolled_quiet("manage-dictionary", text::MANAGE, theme, true, cx)
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "manage-dictionary".into())
                        })
                        .on_click(
                            cx.listener(|this, _, window, cx| this.open_dictionary(window, cx)),
                        ),
                    theme,
                ),
            )
            .children(error.map(|e| {
                div()
                    .text_size(px(11.))
                    .text_color(style::markdown_style(theme).alert_warning)
                    .child(e.to_string())
            }))
            .into_any_element()
    }

    /// The dictionary view that replaces the Settings body while open.
    pub(in crate::settings) fn dictionary_body(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = theme.pdf_style();
        let Some(view) = &self.dictionary else {
            return div().into_any_element();
        };
        let all = self.shared.borrow().words.words.clone();
        let needle = view.query.read(cx).value().trim().to_lowercase();
        let shown: Rc<Vec<String>> = Rc::new(
            all.iter()
                .filter(|w| needle.is_empty() || w.to_lowercase().contains(&needle))
                .cloned()
                .collect(),
        );
        let count = if needle.is_empty() {
            text::words(all.len())
        } else {
            text::words_of(shown.len(), all.len())
        };
        let typed = match self.typed(cx) {
            Typed::Add(word) => Some(
                ui::primary_button("add-dictionary-word", text::add_word(&word), theme, true)
                    .self_start()
                    .when(cfg!(test), |v| {
                        v.debug_selector(|| "add-dictionary-word".into())
                    })
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.add_typed_word(cx);
                        this.focus_query(window, cx);
                    }))
                    .into_any_element(),
            ),
            Typed::Present => Some(
                div()
                    .text_size(px(11.))
                    .text_color(p.header_muted)
                    .child(text::ALREADY_IN_DICTIONARY)
                    .into_any_element(),
            ),
            Typed::Invalid => Some(
                div()
                    .text_size(px(11.))
                    .text_color(p.header_muted)
                    .child(text::NOT_A_WORD)
                    .into_any_element(),
            ),
            Typed::Empty => None,
        };
        let empty = shown.is_empty().then(|| {
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(12.))
                .text_color(p.header_muted)
                .child(if all.is_empty() {
                    text::DICTIONARY_EMPTY
                } else {
                    text::NO_MATCHING_WORDS
                })
        });
        let list = (!shown.is_empty()).then(|| {
            let words = shown.clone();
            uniform_list(
                "dictionary-words",
                words.len(),
                cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                    let theme = this.theme.get();
                    let p = theme.pdf_style();
                    range
                        .map(|index| {
                            let word = words[index].clone();
                            div()
                                .id(gpui::SharedString::from(format!("dictionary-row-{word}")))
                                .w_full()
                                .flex()
                                .items_center()
                                .gap_2()
                                .h(px(30.))
                                .px_2()
                                .rounded_md()
                                .hover(move |s| s.bg(p.placeholder_bg))
                                .child(div().flex_1().min_w_0().text_ellipsis().child(word.clone()))
                                .child({
                                    let target = word.clone();
                                    quiet_control(
                                        gpui::SharedString::from(format!("remove-word-{word}")),
                                        text::REMOVE,
                                        theme,
                                        true,
                                    )
                                    .aria_label(text::remove_from_dictionary(&word))
                                    .when(cfg!(test), |v| {
                                        v.debug_selector(move || format!("remove-word-{index}"))
                                    })
                                    .on_click(cx.listener(
                                        move |this, _, window, cx| {
                                            this.remove_word(&target, cx);
                                            this.focus_query(window, cx);
                                        },
                                    ))
                                })
                        })
                        .collect::<Vec<_>>()
                }),
            )
            .track_scroll(&view.scroll)
            .flex_1()
            .min_h(px(90.))
        });
        let undo = view.removed.as_ref().map(|(word, _)| {
            div()
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .py_1()
                .rounded_md()
                .bg(theme.sidebar_bg())
                .text_size(px(12.))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_ellipsis()
                        .child(text::removed_word(word)),
                )
                .child(
                    quiet_control("undo-remove-word", text::UNDO, theme, true)
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "undo-remove-word".into())
                        })
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.undo_remove(cx);
                            this.focus_query(window, cx);
                        })),
                )
        });
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap_2()
            .px_4()
            .pb_3()
            .key_context("DictionarySearch")
            .on_action(cx.listener(|this, _: &AddDictionaryWord, _, cx| this.add_typed_word(cx)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .id("dictionary-search")
                            .flex_1()
                            .border_1()
                            .rounded_md()
                            .border_color(p.border)
                            .bg(p.bg)
                            .px_2()
                            .py_1()
                            .child(view.query.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(p.header_muted)
                            .child(count),
                    ),
            )
            .children(typed)
            .children(empty)
            .children(list)
            .children(undo)
            .into_any_element()
    }
}
