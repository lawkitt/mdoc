//! Adding and removing the dictionary's words, with a short Undo, and
//! saving them.
use super::{DictionaryView, Typed, UNDO_WINDOW};
use crate::document::markdown_search::{self, SearchInput};
use crate::settings::{Event, Panel, text};
use gpui::{App, Context, Focusable, UniformListScrollHandle, Window, prelude::*};
use mdoc_store::settings::{Shared, Store};

/// Replace the user's words and save them in the background. Returns
/// `false` (changing nothing) when the list is full.
pub(crate) fn set_words(
    shared: &Shared,
    words: std::collections::BTreeSet<String>,
    cx: &mut App,
) -> bool {
    let write = match Store::set_words(shared, words) {
        Ok(Some(write)) => write,
        Ok(None) => return true,
        Err(_) => return false,
    };
    let task = cx.background_executor().spawn(async move { write.run() });
    let shared = shared.clone();
    cx.spawn(async move |_| {
        shared.borrow_mut().words.error = task.await.err();
    })
    .detach();
    true
}

impl Panel {
    pub fn open_dictionary(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.category = super::Category::Spelling;
        let query = cx.new(|cx| {
            SearchInput::new(cx)
                .with_theme(self.theme.clone())
                .with_key_context("SettingsInput")
                .with_placeholder(text::SEARCH_OR_ADD_WORD)
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
    pub(super) fn focus_query(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(view) = &self.dictionary {
            window.focus(&view.query.read(cx).focus_handle(cx), cx);
        }
    }

    pub fn close_dictionary(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dictionary = None;
        if let Some(focus) = self
            .controls
            .borrow()
            .get(&gpui::ElementId::from("manage-dictionary"))
        {
            window.focus(focus, cx);
        } else {
            window.focus(&self.category_focus, cx);
        }
        cx.notify();
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
        let saved = set_words(&self.shared, words, cx);
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
        set_words(&self.shared, words, cx);
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

    pub(super) fn undo_remove(&mut self, cx: &mut Context<Self>) {
        let Some((word, _)) = self.dictionary.as_mut().and_then(|v| v.removed.take()) else {
            return;
        };
        self.insert_word(word, cx);
        cx.notify();
    }
}
