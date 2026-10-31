//! What the window gives the menu bar shelf (ADR 0084): its open tabs with
//! applied aliases and its cases' closed members, read only while the shelf
//! asks; and, on save, the fingerprint of a ready member's text, kept in its
//! (encrypted) case.

use super::{Workspace, navigation::Arrival};
use crate::{
    document_view::DocumentView,
    shelf::{self, ClosedMember, OpenTab, ShelfDocument, ShelfKey},
};
use gpui::{App, Context, Window};
use mdoc_pii::restore::ExportedIdentity;
use mdoc_store::{session_store, settings};
use std::{path::PathBuf, time::SystemTime};

impl Workspace {
    /// The shelf's documents: open tabs with applied aliases, then closed
    /// members of unlocked cases, at most `shelf::MAX_DOCUMENTS`. Reads up
    /// to that many member files; nothing about them is kept.
    pub(crate) fn shelf_documents(&self, cx: &App) -> Vec<ShelfDocument> {
        let mut open = Vec::new();
        let mut tabs = Vec::new();
        for tab in &self.tabs {
            let Some(view) = tab.view() else {
                continue;
            };
            let view = view.read(cx);
            if let Some(path) = &view.session.document.path {
                open.push(session_store::identity(path));
            }
            if let Some(document) = shelf_tab(tab.id(), view, cx) {
                tabs.push(document);
            }
        }
        let room = shelf::MAX_DOCUMENTS.saturating_sub(tabs.len());
        // Locked cases are not loaded: their closed members stay hidden.
        let members = shelf::closed_members(self.case_coordinator.cases(), &open, room)
            .into_iter()
            .map(|(case, path, edited)| {
                let text = shelf::read_member(path);
                ShelfDocument::of_member(ClosedMember {
                    case,
                    path,
                    text: text.as_deref(),
                    edited,
                })
            })
            .collect();
        shelf::list(tabs, members)
    }

    /// The tab `id` was saved, or its case created: keep the fingerprint of
    /// its saved text in its case while it is ready, or forget it.
    pub(super) fn record_review(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(view) = self.tabs.get(id).and_then(|tab| tab.view().cloned()) else {
            return;
        };
        let (link, path, fingerprint) = {
            let v = view.read(cx);
            (
                v.case.link().cloned(),
                v.session
                    .document
                    .path
                    .as_deref()
                    .map(session_store::identity),
                if !ready(v, cx) {
                    Some(None)
                } else if v.lifecycle.dirty() {
                    // The saved text is the reviewed one only while nothing
                    // is unsaved: keep the fingerprint of the last save.
                    None
                } else {
                    Some(Some(shelf::fingerprint(&v.session.document.saved)))
                },
            )
        };
        let (Some(link), Some(path), Some(fingerprint)) = (link, path, fingerprint) else {
            return;
        };
        if let Some(((), effect)) = self.case_coordinator.edit(&link.id, |case| {
            case.members
                .contains(&path)
                .then(|| case.set_reviewed(&path, fingerprint))
                .filter(|changed| *changed)
                .map(|_| ())
        }) {
            self.carry_out_cases(effect.into_iter().collect(), cx);
        }
    }
}

/// A tab is ready as the shelf counts it (ADR 0084): an applied alias,
/// nothing pending or still being found, and no known name left as written.
fn ready(view: &DocumentView, cx: &App) -> bool {
    shelf_tab(0, view, cx).is_some_and(|row| row.readiness.is_ready())
}

/// An open tab's shelf row; `None` without an applied alias.
fn shelf_tab(id: u64, view: &DocumentView, cx: &App) -> Option<ShelfDocument> {
    let review = view.pii.review();
    if review.applied().is_empty() || !view.lifecycle.ready() {
        return None;
    }
    // Every name the tab knows: its mapping, its case's names and every
    // variant, pending and kept ones included.
    let mut names = review.export_identities();
    names.extend(view.case.names().iter().cloned());
    names.extend(review.variants().iter().map(|variant| ExportedIdentity {
        alias: variant.replacement.clone(),
        previous: Vec::new(),
        category: variant.category,
        custom: false,
        originals: vec![variant.original.clone()],
    }));
    let edited = view
        .lifecycle
        .edited_at()
        .or_else(|| modified(view.session.document.path.as_ref()))
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let title = view.display_name();
    ShelfDocument::of_tab(OpenTab {
        id,
        title: &title,
        text: view.editor.read(cx).text(),
        aliases: review
            .applied()
            .iter()
            .map(|applied| applied.step.after.as_ref())
            .collect(),
        pending: review.remaining(),
        checking: review.awaiting_discovery(),
        names,
        case: view.case.link().cloned(),
        edited,
    })
}

fn modified(path: Option<&PathBuf>) -> Option<SystemTime> {
    std::fs::metadata(path?).ok()?.modified().ok()
}

/// Whose names the shelf's clipboard rows use (ADR 0084): a case, or an
/// open tab without one through its live mapping. Never mixed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ClipboardSource {
    Case {
        id: String,
        name: String,
    },
    /// The tab's title, pseudonymized.
    Tab {
        id: u64,
        title: String,
    },
}

impl ClipboardSource {
    /// The name the row and its picker show.
    pub(crate) fn name(&self) -> &str {
        match self {
            Self::Case { name, .. } => name,
            Self::Tab { title, .. } => title,
        }
    }
}

/// The names a clipboard row may use.
pub(crate) enum ClipboardNames {
    Names(Vec<ExportedIdentity>),
    /// The case's key is not open: "Unlock cases in mdoc…".
    Locked,
    /// The case or tab is gone.
    Gone,
}

impl Workspace {
    /// Every source the picker offers: open tabs without a case that have a
    /// mapping, then every case, loaded or locked.
    pub(crate) fn clipboard_sources(&self, cx: &App) -> Vec<ClipboardSource> {
        let mut sources: Vec<_> = self
            .tabs
            .iter()
            .filter_map(|tab| {
                let view = tab.view()?.read(cx);
                if view.case.link().is_some() || view.pii.review().applied().is_empty() {
                    return None;
                }
                let row = shelf_tab(tab.id(), view, cx)?;
                Some(ClipboardSource::Tab {
                    id: tab.id(),
                    title: row.title,
                })
            })
            .collect();
        let mut cases = self.case_coordinator.summaries();
        cases.sort_by_key(|case| case.name.to_lowercase());
        sources.extend(cases.into_iter().map(|case| ClipboardSource::Case {
            id: case.id,
            name: case.name,
        }));
        sources
    }

    /// The document used last: the active tab's case, or its own mapping.
    pub(crate) fn active_clipboard_source(&self, cx: &App) -> Option<ClipboardSource> {
        let view = self.active_view()?;
        let view = view.read(cx);
        if let Some(link) = view.case.link() {
            return Some(ClipboardSource::Case {
                id: link.id.clone(),
                name: link.name.clone(),
            });
        }
        let id = self.tabs.active();
        self.clipboard_sources(cx)
            .into_iter()
            .find(|source| matches!(source, ClipboardSource::Tab { id: tab, .. } if *tab == id))
    }

    /// The names `source` restores or pseudonymizes with. Never reads the
    /// case key: a case that is not open is locked.
    pub(crate) fn clipboard_names(&self, source: &ClipboardSource, cx: &App) -> ClipboardNames {
        match source {
            ClipboardSource::Case { id, .. } => match self.case_coordinator.case(id) {
                Some(case) => ClipboardNames::Names(case.exported()),
                None if self.case_coordinator.key_wait().is_some() => ClipboardNames::Locked,
                None => ClipboardNames::Gone,
            },
            ClipboardSource::Tab { id, .. } => match self.tabs.get(*id).and_then(|t| t.view()) {
                Some(view) => ClipboardNames::Names(view.read(cx).pii.review().export_identities()),
                None => ClipboardNames::Gone,
            },
        }
    }

    /// A shelf row's click: the tab comes forward in its current view, a
    /// closed member opens as a tab, and a document that is not ready
    /// shows its first pending proposal once it has loaded.
    pub(crate) fn open_from_shelf(
        &mut self,
        key: &ShelfKey,
        ready: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (tab, path) = match key {
            ShelfKey::Tab(id) => {
                if !self.tabs.contains(*id) {
                    return;
                }
                self.activate_as(*id, Arrival::Visit, window, cx);
                (Some(*id), None)
            }
            ShelfKey::Member { path, .. } => {
                self.open_paths(vec![path.clone()], window, cx);
                (None, Some(path.clone()))
            }
        };
        if ready {
            if tab.is_some() {
                self.focus_active(window, cx);
            }
            return;
        }
        // A closed member opens and loads asynchronously, and its proposals
        // appear once it joins its case: wait a little for them.
        cx.spawn_in(window, async move |this, cx| {
            for _ in 0..50 {
                let shown = this
                    .update_in(cx, |this, window, cx| {
                        let id = tab.or_else(|| this.find_path(path.as_deref()?, None, cx));
                        let Some(view) = id
                            .and_then(|id| this.tabs.get(id))
                            .and_then(|tab| tab.view().cloned())
                        else {
                            return false;
                        };
                        if !view.read(cx).lifecycle.ready() {
                            return false;
                        }
                        view.update(cx, |view, cx| view.select_first_proposal(window, cx))
                    })
                    .unwrap_or(true);
                if shown {
                    return;
                }
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(100))
                    .await;
            }
        })
        .detach();
    }

    /// "Open mdoc": Home lists every document.
    pub(crate) fn home_from_shelf(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_home(window, cx);
    }

    /// The shelf's Settings gear.
    pub(crate) fn settings_from_shelf(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_settings(window, cx);
    }

    /// The first-close note's "Quit when windows close" (ADR 0084): the
    /// switch goes off and mdoc quits, as it now would with its last window
    /// closed. The hidden window shows only if the quit asks something; a
    /// quit that cannot start brings it back, since no item is left to.
    pub(crate) fn quit_when_windows_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.update_general(|general| general.menu_bar = false, cx);
        crate::menu_bar::quit_when_windows_close(cx);
        self.quit(window, cx);
        if !self.quit_flow.quitting() {
            crate::menu_bar::reveal_if_hidden(window, cx);
        }
    }

    /// Whether the first-close note has shown (ADR 0084).
    pub(crate) fn menu_bar_note_shown(&self) -> bool {
        self.general()
            .is_some_and(|general| general.menu_bar_note_shown)
    }

    /// Whether a shelf drag has dropped, retiring its hint (ADR 0084).
    pub(crate) fn shelf_drag_hint_shown(&self) -> bool {
        self.general()
            .is_some_and(|general| general.shelf_drag_hint_shown)
    }

    fn general(&self) -> Option<settings::GeneralConfig> {
        self.preferences
            .borrow()
            .current
            .as_ref()
            .map(|preferences| preferences.general)
    }

    /// Change a Settings › General choice from outside the panel (the
    /// shelf's notes): saved at once, kept in memory if the save fails, and
    /// the panel's draft follows so its next save keeps it. Only flags and
    /// switches; never document text.
    pub(crate) fn update_general(
        &mut self,
        change: impl FnOnce(&mut settings::GeneralConfig),
        cx: &mut Context<Self>,
    ) {
        let (prefs, path) = {
            let store = self.preferences.borrow();
            let Some(current) = store.current.clone() else {
                return;
            };
            (current, store.path.clone())
        };
        let mut changed = prefs.clone();
        change(&mut changed.general);
        if changed == prefs {
            return;
        }
        if let Some(path) = path
            && let Err(error) = settings::save(&path, &changed)
        {
            eprintln!("mdoc: could not save a menu bar choice: {error}");
        }
        let general = changed.general;
        self.preferences.borrow_mut().current = Some(changed);
        self.settings
            .update(cx, |panel, cx| panel.general_changed(general, cx));
    }

    /// "Unlock cases in mdoc…": the Keychain explainer, in the window.
    pub(crate) fn unlock_from_shelf(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.unlock_cases(window, cx);
    }

    /// "Restart to update": the ordinary quit, prompts included (ADR 0040).
    pub(crate) fn restart_from_shelf(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.restart_to_update(window, cx);
    }
}
