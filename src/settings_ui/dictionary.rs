//! The user dictionary view inside Settings (ADR 0036): search as you type,
//! add the typed word, remove words with a short Undo. The list is
//! virtualized, so only visible rows are drawn and focusable.
use super::*;
use gpui::{UniformListScrollHandle, uniform_list};

actions!(model_settings, [AddDictionaryWord]);

/// How long "Removed … · Undo" stays offered.
const UNDO_WINDOW: Duration = Duration::from_secs(6);

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([KeyBinding::new(
        "enter",
        AddDictionaryWord,
        Some("DictionarySearch"),
    )]);
}

pub struct DictionaryView {
    pub query: Entity<SearchInput>,
    scroll: UniformListScrollHandle,
    /// The last removed word, offered back until the timer drops it.
    removed: Option<(String, gpui::Task<()>)>,
    _query_subscription: gpui::Subscription,
}

/// What the typed text would do: nothing, add it, or it's already there.
enum Typed {
    Empty,
    Add(String),
    Present,
    Invalid,
}

impl Panel {
    pub fn open_dictionary(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let query = cx.new(|cx| {
            SearchInput::new(cx)
                .with_key_context("SettingsInput")
                .with_placeholder("Search or add a word")
        });
        let subscription = cx.subscribe(
            &query,
            |this, _, _: &markdown_search::SearchInputEvent, cx| {
                if let Some(view) = &this.dictionary {
                    view.scroll.scroll_to_item(0, gpui::ScrollStrategy::Top);
                }
                cx.notify();
            },
        );
        window.focus(&query.read(cx).focus_handle(cx), cx);
        self.dictionary = Some(DictionaryView {
            query,
            scroll: UniformListScrollHandle::new(),
            removed: None,
            _query_subscription: subscription,
        });
        cx.notify();
    }

    /// Keep typing and Escape working after a clicked row or button goes away.
    fn focus_query(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(view) = &self.dictionary {
            window.focus(&view.query.read(cx).focus_handle(cx), cx);
        }
    }

    pub fn close_dictionary(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dictionary = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn typed(&self, cx: &App) -> Typed {
        let Some(view) = &self.dictionary else {
            return Typed::Empty;
        };
        let word = view.query.read(cx).value().trim();
        if word.is_empty() {
            Typed::Empty
        } else if self.shared.borrow().words.words.contains(word) {
            Typed::Present
        } else if valid_word(word) {
            Typed::Add(word.to_owned())
        } else {
            Typed::Invalid
        }
    }

    pub fn add_typed_word(&mut self, cx: &mut Context<Self>) {
        let Typed::Add(word) = self.typed(cx) else {
            return;
        };
        if self.insert_word(word, cx)
            && let Some(view) = &self.dictionary
        {
            view.query.update(cx, |query, cx| query.reset(cx));
        }
        cx.notify();
    }

    /// Add `word` to the dictionary; `false` when the list is full.
    fn insert_word(&mut self, word: String, cx: &mut Context<Self>) -> bool {
        let mut words = (*self.shared.borrow().words.words).clone();
        words.insert(word);
        let saved = Store::set_words(&self.shared, words, cx);
        if saved {
            // Open documents re-read the dictionary when they render.
            cx.emit(Event::Applied);
        }
        saved
    }

    pub fn remove_word(&mut self, word: &str, cx: &mut Context<Self>) {
        let mut words = (*self.shared.borrow().words.words).clone();
        if !words.remove(word) {
            return;
        }
        Store::set_words(&self.shared, words, cx);
        cx.emit(Event::Applied);
        if let Some(view) = &mut self.dictionary {
            let timer = cx.spawn(async move |this, cx| {
                cx.background_executor().timer(UNDO_WINDOW).await;
                let _ = this.update(cx, |this, cx| {
                    if let Some(view) = &mut this.dictionary {
                        view.removed = None;
                    }
                    cx.notify();
                });
            });
            view.removed = Some((word.to_owned(), timer));
        }
        cx.notify();
    }

    fn undo_remove(&mut self, cx: &mut Context<Self>) {
        let Some((word, _)) = self.dictionary.as_mut().and_then(|v| v.removed.take()) else {
            return;
        };
        self.insert_word(word, cx);
        cx.notify();
    }

    /// The Settings → Spelling summary line with Manage….
    pub(super) fn dictionary_summary(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let p = theme.pdf_style();
        let (count, error) = {
            let store = self.shared.borrow();
            (store.words.words.len(), store.words.error.clone())
        };
        let summary = match count {
            0 => "No words yet. Add them with “Add to dictionary” in the editor.".to_string(),
            1 => "1 word".to_string(),
            n => format!("{n} words"),
        };
        div()
            .flex()
            .flex_col()
            .gap_1()
            .mt_1()
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(div().text_size(px(12.)).child("Your dictionary"))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(p.header_muted)
                            .child(summary),
                    )
                    .child(div().flex_1())
                    .child(
                        self.scrolled_quiet("manage-dictionary", "Manage…", theme, true, cx)
                            .when(cfg!(test), |v| {
                                v.debug_selector(|| "manage-dictionary".into())
                            })
                            .on_click(
                                cx.listener(|this, _, window, cx| this.open_dictionary(window, cx)),
                            ),
                    ),
            )
            .children(error.map(|e| {
                div()
                    .text_size(px(11.))
                    .text_color(style::markdown_style(theme).alert_warning)
                    .child(e)
            }))
            .into_any_element()
    }

    /// The dictionary view that replaces the Settings body while open.
    pub(super) fn dictionary_body(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
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
            match all.len() {
                1 => "1 word".to_string(),
                n => format!("{n} words"),
            }
        } else {
            format!("{} of {}", shown.len(), all.len())
        };
        let typed = match self.typed(cx) {
            Typed::Add(word) => Some(
                ui::primary_button("add-dictionary-word", format!("Add “{word}”"), theme, true)
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
                    .child("Already in your dictionary")
                    .into_any_element(),
            ),
            Typed::Invalid => Some(
                div()
                    .text_size(px(11.))
                    .text_color(p.header_muted)
                    .child("Enter a single word with at least one letter, up to 64 characters.")
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
                    "Your dictionary is empty. Type a word above to add it, or use “Add to dictionary” in the editor."
                } else {
                    "No matching words."
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
                                        "Remove",
                                        theme,
                                        true,
                                    )
                                    .aria_label(format!("Remove {word} from your dictionary"))
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
                        .child(format!("Removed “{word}”")),
                )
                .child(
                    quiet_control("undo-remove-word", "Undo", theme, true)
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
