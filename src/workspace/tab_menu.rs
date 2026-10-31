//! The sidebar tab menu's file and close rows (UI polish grill decision 13):
//! Reveal in Finder (Show in Explorer on Windows) and Copy path for a tab
//! with a file, then Close tab and Close others. Close others closes the
//! other tabs in turn, as a case's Close all does, so each at-risk tab asks
//! with ADR 0071's prompt and Cancel stops the rest.
use super::Workspace;
use crate::ui;
use gpui::{AnyElement, App, Context, Window, prelude::*};
use std::{
    path::{Path, PathBuf},
    rc::Rc,
};

/// A row below the tab's case rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TabMenuItem {
    Reveal,
    CopyPath,
    Close,
    CloseOthers,
}

impl TabMenuItem {
    /// Sentence case, as the menu's other rows.
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Reveal if cfg!(windows) => "Show in Explorer",
            Self::Reveal if cfg!(target_os = "macos") => "Reveal in Finder",
            Self::Reveal => "Show in folder",
            Self::CopyPath => "Copy path",
            Self::Close => "Close tab",
            Self::CloseOthers => "Close others",
        }
    }

    fn id(self) -> &'static str {
        match self {
            Self::Reveal => "context-reveal",
            Self::CopyPath => "context-copy-path",
            Self::Close => "context-close-tab",
            Self::CloseOthers => "context-close-others",
        }
    }

    /// The file rows come first, then the close rows, with a divider between.
    fn closes(self) -> bool {
        matches!(self, Self::Close | Self::CloseOthers)
    }
}

/// The rows for a tab: Reveal and Copy path only when it has a file (never
/// for an untitled tab), then Close tab, and Close others while another tab
/// is open.
pub(super) fn items(file: Option<&Path>, others: bool) -> Vec<TabMenuItem> {
    let mut items = Vec::new();
    if file.is_some() {
        items.extend([TabMenuItem::Reveal, TabMenuItem::CopyPath]);
    }
    items.push(TabMenuItem::Close);
    if others {
        items.push(TabMenuItem::CloseOthers);
    }
    items
}

/// Shows a path in the platform's file manager.
type Revealer = Rc<dyn Fn(&Path, &App)>;

/// What Reveal in Finder does with a path. Tests record the path instead:
/// the test platform has no file manager.
#[derive(Clone)]
pub(super) struct FileActions {
    reveal: Revealer,
}

impl Default for FileActions {
    fn default() -> Self {
        Self {
            reveal: Rc::new(|path, cx| cx.reveal_path(path)),
        }
    }
}

impl FileActions {
    pub(super) fn reveal(&self, path: &Path, cx: &App) {
        (self.reveal)(path, cx)
    }

    /// The full path as text, as the file manager shows it.
    pub(super) fn copy_path(&self, path: &Path, cx: &mut App) {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(
            path.to_string_lossy().into_owned(),
        ));
    }

    /// Reveals into a list the test reads.
    #[cfg(test)]
    pub(super) fn recording() -> (Self, Rc<std::cell::RefCell<Vec<PathBuf>>>) {
        let seen = Rc::new(std::cell::RefCell::new(Vec::new()));
        let log = seen.clone();
        let actions = Self {
            reveal: Rc::new(move |path, _| log.borrow_mut().push(path.to_path_buf())),
        };
        (actions, seen)
    }
}

impl Workspace {
    /// The file tab `id` holds: its saved document, else the source it was
    /// converted from. None for an untitled tab.
    pub(super) fn tab_file(&self, id: u64, cx: &App) -> Option<PathBuf> {
        let tab = self.tabs.get(id)?;
        match tab.view() {
            Some(view) => {
                let view = view.read(cx);
                view.session
                    .document
                    .path
                    .clone()
                    .or_else(|| view.preview.source().cloned())
            }
            // Not shown since launch: the restored record names it.
            None => {
                let record = tab.record();
                (!record.blank && !record.markdown.as_os_str().is_empty())
                    .then(|| record.markdown.clone())
            }
        }
    }

    /// The tabs Close others would close, in sidebar order.
    fn other_tabs(&self, keep: u64, cx: &App) -> Vec<u64> {
        self.tabs
            .iter()
            .filter(|tab| tab.id() != keep && !self.is_placeholder(tab, cx))
            .map(|tab| tab.id())
            .collect()
    }

    /// The file and close rows of tab `id`'s menu.
    pub(super) fn tab_menu_rows(&self, id: u64, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let file = self.tab_file(id, cx);
        let others = !self.other_tabs(id, cx).is_empty();
        let mut rows = Vec::new();
        let mut file_rows = false;
        for item in items(file.as_deref(), others) {
            if item.closes() && file_rows {
                rows.push(ui::menu_divider(theme).into_any_element());
                file_rows = false;
            }
            file_rows |= !item.closes();
            let label = item.label();
            let id_name = item.id();
            let file = file.clone();
            rows.push(
                ui::focus_ring(id_name, theme)
                    .role(gpui::Role::Button)
                    .aria_label(label)
                    .key_context("UiControl")
                    .tab_index(0)
                    .when(cfg!(test), |v| v.debug_selector(move || id_name.into()))
                    .px_3()
                    .py_1()
                    .rounded_sm()
                    .cursor_pointer()
                    .hover(move |v| v.bg(palette.placeholder_bg))
                    .child(label)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.tab_menu = None;
                        this.choose_tab_menu(item, id, file.as_deref(), window, cx);
                    }))
                    .into_any_element(),
            );
        }
        rows
    }

    /// Carry out a file or close row for tab `id`, whose file is `file`.
    pub(super) fn choose_tab_menu(
        &mut self,
        item: TabMenuItem,
        id: u64,
        file: Option<&Path>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(focus) = self.menu_return.take() {
            window.focus(&focus, cx);
        }
        match (item, file) {
            (TabMenuItem::Reveal, Some(path)) => self.file_actions.reveal(path, cx),
            (TabMenuItem::CopyPath, Some(path)) => self.file_actions.copy_path(path, cx),
            (TabMenuItem::Close, _) => self.close_tab(id, window, cx),
            (TabMenuItem::CloseOthers, _) => self.close_others(id, window, cx),
            _ => {}
        }
        cx.notify();
    }

    /// Close every tab but `keep`, in sidebar order. Each at-risk tab asks
    /// first, as closing it alone would (ADR 0071); once a tab is removed the
    /// next follows, and Cancel at any question stops the rest. The queue is
    /// the one a case's Close all uses.
    pub(super) fn close_others(&mut self, keep: u64, window: &mut Window, cx: &mut Context<Self>) {
        if !self.quit_flow.may_close() || self.mapping_prompt_open() {
            return;
        }
        let others = self.other_tabs(keep, cx);
        if let Some(first) = self.cases.closing_members.begin(others) {
            self.close_tab(first, window, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use TabMenuItem::*;

    #[test]
    fn a_saved_tab_offers_its_file_and_an_untitled_one_does_not() {
        let file = Path::new("/tmp/contract.md");
        assert_eq!(
            items(Some(file), true),
            [Reveal, CopyPath, Close, CloseOthers]
        );
        assert_eq!(items(None, true), [Close, CloseOthers]);
        assert_eq!(items(Some(file), false), [Reveal, CopyPath, Close]);
        assert_eq!(items(None, false), [Close]);
    }

    #[test]
    fn reveal_names_the_platforms_file_manager() {
        let expected = if cfg!(windows) {
            "Show in Explorer"
        } else if cfg!(target_os = "macos") {
            "Reveal in Finder"
        } else {
            "Show in folder"
        };
        assert_eq!(Reveal.label(), expected);
        assert_eq!(CopyPath.label(), "Copy path");
        assert_eq!(CloseOthers.label(), "Close others");
    }
}
