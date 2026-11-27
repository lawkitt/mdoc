//! The user dictionary view inside Settings: search as you type,
//! add the typed word, remove words with a short Undo. The list is
//! virtualized, so only visible rows are drawn and focusable.
use super::Panel;
use crate::document::markdown_search::SearchInput;
use gpui::{App, Entity, UniformListScrollHandle, actions};
use mdoc_store::settings::valid_word;
use std::time::Duration;

mod actions;
mod view;

use super::Category;

pub(crate) use actions::set_words;

actions!(model_settings, [AddDictionaryWord]);

/// How long "Removed … · Undo" stays offered.
const UNDO_WINDOW: Duration = Duration::from_secs(6);

pub fn bind_keys(cx: &mut App) {
    let bindings =
        crate::commands::shortcuts::bindings(crate::commands::shortcuts::Owner::Dictionary, cx);
    cx.bind_keys(bindings);
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
}
