//! What the window gives the menu bar shelf: its open tabs with applied
//! aliases, then the Vault's other documents, each ready only once the
//! Vault's outbound check passes; and its Projects' names for the
//! clipboard rows.

use super::{ProjectLink, Workspace, document_writer::Target, navigation::Arrival};
use crate::{
    document_view::DocumentView,
    shelf::{self, OpenTab, Outbound, ShelfDocument, ShelfKey, StoredDocument},
};
use gpui::{App, Context, Window};
use mdoc_pii::restore::{ExportedIdentity, RestoreMap};
use mdoc_store::{
    document_id::{DocumentId, VaultId},
    settings,
};
use mdoc_vault::{DocumentSummary, Vault, VaultError};
use std::{
    collections::HashSet,
    time::{Duration, SystemTime},
};

/// The shelf opens at most this many stored documents to fill its list:
/// one whose text holds no alias is not listed.
const STORED_READS: usize = 4 * shelf::MAX_DOCUMENTS;

impl Workspace {
    /// The shelf's documents: open tabs with applied aliases, then the
    /// Vault's documents no tab shows, at most `shelf::MAX_DOCUMENTS`.
    /// Opens up to [`STORED_READS`] stored documents; nothing about them is
    /// kept.
    pub(crate) fn shelf_documents(&self, cx: &App) -> Vec<ShelfDocument> {
        let mut shown = HashSet::new();
        let mut tabs = Vec::new();
        for tab in &self.tabs {
            let Some(view) = tab.view() else {
                continue;
            };
            let view = view.read(cx);
            if let Some(DocumentId::Vault(id)) = view.session.document.id() {
                shown.insert(id.as_str().to_owned());
            }
            tabs.extend(self.shelf_tab(tab.id(), view, cx));
        }
        let room = shelf::MAX_DOCUMENTS.saturating_sub(tabs.len());
        let stored = match self.vault.vault() {
            Some(vault) if room > 0 => {
                let mut summaries: Vec<_> = (vault.documents().into_iter())
                    .filter(|summary| !summary.exclude_from_ai && !shown.contains(&summary.id))
                    .collect();
                summaries.sort_by_key(|summary| std::cmp::Reverse(summary.updated));
                (summaries.iter().take(STORED_READS))
                    .filter_map(|summary| self.stored_row(vault, summary, cx))
                    .take(room)
                    .collect()
            }
            _ => Vec::new(),
        };
        shelf::list(tabs, stored)
    }

    /// Row `key` as it stands now, checked again as its text is about to
    /// leave; `None` when it is gone.
    pub(crate) fn shelf_row(&self, key: &ShelfKey, cx: &App) -> Option<ShelfDocument> {
        match key {
            ShelfKey::Tab(id) => {
                let view = self.tabs.get(*id)?.view()?.read(cx);
                self.shelf_tab(*id, view, cx)
            }
            ShelfKey::Document(id) => {
                let vault = self.vault.vault()?;
                let summary = (vault.documents().into_iter())
                    .find(|summary| summary.id == id.as_str() && !summary.exclude_from_ai)?;
                self.stored_row(vault, &summary, cx)
            }
        }
    }

    /// An open tab's shelf row; `None` without an applied alias. The Vault
    /// is asked only once its review is ready.
    fn shelf_tab(&self, id: u64, view: &DocumentView, cx: &App) -> Option<ShelfDocument> {
        if let (Some(vault), Some(DocumentId::Vault(document))) =
            (self.vault.vault(), view.session.document.id())
            && (vault.documents().iter())
                .any(|summary| summary.id == document.as_str() && summary.exclude_from_ai)
        {
            return None;
        }
        let names = view.known_identities();
        let title = view.display_name();
        let mut tab = open_tab(id, view, &title, &names, cx)?;
        if tab.review_readiness().is_ready() {
            tab.stored = self.outbound(id, view, cx);
        }
        ShelfDocument::of_tab(tab)
    }

    /// The outbound check of tab `id`'s Vault document, once what it shows
    /// is written; `None` until then.
    fn outbound(&self, id: u64, view: &DocumentView, cx: &App) -> Option<Outbound> {
        let vault = self.vault.vault()?;
        let Some(DocumentId::Vault(document)) = view.session.document.id() else {
            return None;
        };
        let unwritten = view.unsaved(cx)
            || self.moving.contains(&document)
            || self.write_pending_for(id, cx)
            || view.saved_review() != view.session.document.review
            || (view.project.link()).is_some_and(|link| self.project_unwritten(&link.id, cx));
        if unwritten {
            return None;
        }
        Some(checked(vault.text_for_ai(document.as_str())))
    }

    /// A stored document's row, with the names it lives with as stored;
    /// `None` when it can't be read or holds no alias.
    fn stored_row(
        &self,
        vault: &Vault,
        summary: &DocumentSummary,
        cx: &App,
    ) -> Option<ShelfDocument> {
        let id = VaultId::parse(&summary.id)?;
        let document = vault.document(&summary.id).ok()?;
        let (names, project) = match &document.project {
            Some(project) => {
                let stored = vault.project(project).ok()?;
                let link =
                    (self.projects.link(project)).unwrap_or_else(|| ProjectLink::of(&stored));
                (stored.mapping.exported(), Some(link))
            }
            None => (document.mapping.exported(), None),
        };
        // Checked against names the Vault doesn't hold yet, it could leave
        // with one of them as written.
        let unwritten = self.moving.contains(&id)
            || (document.project.as_deref()).is_some_and(|p| self.project_unwritten(p, cx));
        ShelfDocument::of_stored(StoredDocument {
            id,
            title: &document.title,
            text: &document.markdown,
            names,
            project,
            edited: SystemTime::UNIX_EPOCH + Duration::from_secs(document.updated),
            outbound: (!unwritten).then(|| checked(vault.text_for_ai(&summary.id))),
        })
    }

    /// Copy for AI in tab `id`: its text leaves only as a shelf row's
    /// would, once the Vault's outbound check of what it wrote passes
    /// against the names its Project holds now (#1091 Decision 19). What
    /// it shows is written first, and the copy waits for the writes.
    pub(super) fn copy_tab_for_ai(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        self.copy_for_ai_waiting = None;
        self.autosave(id, window, cx);
        let clipboard = clipboard_text(cx);
        self.try_copy_for_ai(id, clipboard, cx);
    }

    /// The writes a waiting Copy for AI waited for have landed: check it
    /// again.
    pub(super) fn copy_for_ai_after_write(&mut self, cx: &mut Context<Self>) {
        if let Some((id, clipboard)) = self.copy_for_ai_waiting.take() {
            self.try_copy_for_ai(id, clipboard, cx);
        }
    }

    /// Copy tab `id`'s checked text, say why it may not leave, or wait
    /// while a write may still settle it. `clipboard`: the clipboard's text
    /// when it was asked; a copy since, here or in another app, is newer
    /// and stays.
    fn try_copy_for_ai(&mut self, id: u64, clipboard: Option<String>, cx: &mut Context<Self>) {
        if clipboard_text(cx) != clipboard {
            return;
        }
        let Some(view) = self.tabs.get(id).and_then(|tab| tab.view().cloned()) else {
            return;
        };
        #[cfg(test)]
        if crate::exits::taken_out(crate::exits::Gate::EditorCopyForAi) {
            // As the cut-over's Copy for AI did: the editor's text, unchecked.
            let text = view.read(cx).text_for_ai(cx);
            view.update(cx, |view, cx| view.copy_checked_for_ai(text, cx));
            return;
        }
        let outcome = match self.shelf_row(&ShelfKey::Tab(id), cx) {
            Some(row) => match (row.text_for_ai(), row.readiness) {
                (Some(text), _) => Ok(text.to_owned()),
                (None, shelf::Readiness::Checking) if self.document_writer.busy() => {
                    self.copy_for_ai_waiting = Some((id, clipboard));
                    return;
                }
                (None, readiness) => Err(not_copied(readiness, self.unsaved.contains(&id))),
            },
            None => Err(self.no_row_reason(&view, cx)),
        };
        view.update(cx, |view, cx| match outcome {
            Ok(text) => view.copy_checked_for_ai(text, cx),
            Err(why) => view.copy_for_ai_refused(why, cx),
        });
    }

    /// Why a tab has no shelf row to copy.
    fn no_row_reason(&self, view: &gpui::Entity<DocumentView>, cx: &App) -> String {
        let excluded = match (self.vault.vault(), view.read(cx).session.document.id()) {
            (Some(vault), Some(DocumentId::Vault(document))) => (vault.documents().iter())
                .any(|summary| summary.id == document.as_str() && summary.exclude_from_ai),
            _ => false,
        };
        if excluded {
            "Not copied: this document is excluded from AI.".into()
        } else {
            "Not copied: pseudonymize the document first.".into()
        }
    }

    /// Project `id`'s names in the window are not all in the Vault yet: its
    /// write waits, runs or failed, or an open member's mapping change
    /// waits to be folded into it.
    fn project_unwritten(&self, id: &str, cx: &App) -> bool {
        self.project_ui.is_unwritten(id)
            || self.document_writer.has(&Target::Project(id.to_string()))
            || self.tabs.iter().filter_map(|tab| tab.view()).any(|view| {
                let view = view.read(cx);
                view.project_save_pending() && view.project.link().is_some_and(|l| l.id == id)
            })
    }
}

/// What `Vault::text_for_ai` said, as the shelf shows it: a document that
/// can't be checked can't leave.
fn checked(outbound: Result<String, VaultError>) -> Outbound {
    match outbound {
        Ok(text) => Ok(text),
        Err(VaultError::NotReady(readiness)) => Err(readiness),
        Err(_) => Err(mdoc_vault::Readiness::NotReviewed),
    }
}

/// The clipboard's text now, if it holds any.
fn clipboard_text(cx: &App) -> Option<String> {
    cx.read_from_clipboard().and_then(|item| item.text())
}

/// Why Copy for AI copied nothing, as the shelf's row would say it.
/// `failed`: the tab's last write failed, so the check can't see its text.
fn not_copied(readiness: shelf::Readiness, failed: bool) -> String {
    use shelf::Readiness::*;
    match readiness {
        LeftToReview(1) => "Not copied: 1 proposal is left to review.".into(),
        LeftToReview(n) => format!("Not copied: {n} proposals are left to review."),
        NamesNotReplaced(1) => "Not copied: 1 known name is still written out in the text.                                 It may be a name another document added to the Project."
            .into(),
        NamesNotReplaced(n) => format!(
            "Not copied: {n} known names are still written out in the text. They may be              names another document added to the Project."
        ),
        ChangedSinceReview => "Not copied: the text changed since its review.".into(),
        Checking if failed => "Not copied: its last change could not be saved, so its names                                can't be checked."
            .into(),
        Checking => "Not copied: mdoc is still checking its names. Try again in a moment.".into(),
        Ready => "Not copied.".into(),
    }
}

/// A tab's review is ready as a write records it: an applied alias,
/// nothing pending or still being found, and no known name left as
/// written. The Vault's own check is the shelf's, as text leaves.
pub(super) fn ready(view: &DocumentView, cx: &App) -> bool {
    let names = view.known_identities();
    open_tab(0, view, "", &names, cx).is_some_and(|tab| tab.review_readiness().is_ready())
}

/// An open tab as the shelf reads it, the Vault not asked yet; `None`
/// without an applied alias or before it has loaded.
fn open_tab<'a>(
    id: u64,
    view: &'a DocumentView,
    title: &'a str,
    names: &[ExportedIdentity],
    cx: &'a App,
) -> Option<OpenTab<'a>> {
    let review = view.pii.review();
    if review.applied().is_empty() || !view.lifecycle.ready() {
        return None;
    }
    Some(OpenTab {
        id,
        title,
        text: view.editor.read(cx).text(),
        aliases: review
            .applied()
            .iter()
            .map(|applied| applied.step.after.as_ref())
            .collect(),
        pending: review.remaining(),
        checking: review.awaiting_discovery(),
        names: names.to_vec(),
        project: view.project.link().cloned(),
        edited: view.lifecycle.edited_at().unwrap_or(SystemTime::UNIX_EPOCH),
        stored: None,
    })
}

/// Whose names the shelf's clipboard rows use: a Project, or an open tab
/// outside one through its live mapping. Never mixed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ClipboardSource {
    Project {
        id: String,
        name: String,
    },
    /// The tab's title, pseudonymized, without `.md`.
    Tab {
        id: u64,
        title: String,
    },
}

impl ClipboardSource {
    /// The name the row and its picker show.
    pub(crate) fn name(&self) -> &str {
        match self {
            Self::Project { name, .. } => name,
            Self::Tab { title, .. } => title,
        }
    }
}

/// The names a clipboard row may use.
pub(crate) enum ClipboardNames {
    Names(Vec<ExportedIdentity>),
    /// The Vault's key is not open: "Unlock in mdoc…".
    Locked,
    /// The Project or tab is gone.
    Gone,
}

impl Workspace {
    /// Every source the picker offers: open tabs outside a Project that
    /// have a mapping, then every Project.
    pub(crate) fn clipboard_sources(&self, cx: &App) -> Vec<ClipboardSource> {
        let mut sources: Vec<_> = self
            .tabs
            .iter()
            .filter_map(|tab| {
                let view = tab.view()?.read(cx);
                if view.project.link().is_some() || view.pii.review().applied().is_empty() {
                    return None;
                }
                // Only its title is shown: the Vault is not asked.
                let (names, title) = (view.known_identities(), view.display_name());
                let row = ShelfDocument::of_tab(open_tab(tab.id(), view, &title, &names, cx)?)?;
                Some(ClipboardSource::Tab {
                    id: tab.id(),
                    title: row.display_title().to_string(),
                })
            })
            .collect();
        let mut projects = self.projects.summaries();
        projects.sort_by_key(|project| project.name.to_lowercase());
        sources.extend(
            projects
                .into_iter()
                .map(|project| self.project_source(&project.id, &project.name)),
        );
        sources
    }

    /// A Project as a clipboard source, named as the shelf may show it:
    /// with its own names pseudonymized, as a Project is often named after
    /// a person.
    fn project_source(&self, id: &str, name: &str) -> ClipboardSource {
        let name = match self.projects.project(id) {
            Some(project) => {
                RestoreMap::new(project.mapping.exported())
                    .pseudonymize(name)
                    .0
            }
            None => name.to_string(),
        };
        ClipboardSource::Project {
            id: id.to_string(),
            name,
        }
    }

    /// The document used last: the active tab's Project, or its own
    /// mapping.
    pub(crate) fn active_clipboard_source(&self, cx: &App) -> Option<ClipboardSource> {
        let view = self.active_view()?;
        let view = view.read(cx);
        if let Some(link) = view.project.link() {
            return Some(self.project_source(&link.id, &link.name));
        }
        let id = self.tabs.active();
        self.clipboard_sources(cx)
            .into_iter()
            .find(|source| matches!(source, ClipboardSource::Tab { id: tab, .. } if *tab == id))
    }

    /// The names `source` restores or pseudonymizes with. Never reads the
    /// key: while the Vault is locked its Projects are.
    pub(crate) fn clipboard_names(&self, source: &ClipboardSource, cx: &App) -> ClipboardNames {
        match source {
            ClipboardSource::Project { id, .. } => match self.projects.project(id) {
                Some(project) => ClipboardNames::Names(project.mapping.exported()),
                None if self.vault.needs_key() => ClipboardNames::Locked,
                None => ClipboardNames::Gone,
            },
            ClipboardSource::Tab { id, .. } => match self.tabs.get(*id).and_then(|t| t.view()) {
                Some(view) => ClipboardNames::Names(view.read(cx).pii.review().export_identities()),
                None => ClipboardNames::Gone,
            },
        }
    }

    /// A shelf row's click: the tab comes forward in its current view, a
    /// stored document opens in a tab, and a document that is not ready
    /// shows its first pending proposal once it has loaded.
    pub(crate) fn open_from_shelf(
        &mut self,
        key: &ShelfKey,
        ready: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tab = match key {
            ShelfKey::Tab(tab) => {
                if !self.tabs.contains(*tab) {
                    return;
                }
                self.activate_as(*tab, Arrival::Visit, window, cx);
                *tab
            }
            ShelfKey::Document(id) => {
                self.open_vault_document(id.clone(), window, cx);
                let document = DocumentId::Vault(id.clone());
                match self.find_document(&document, None, cx) {
                    Some(tab) => tab,
                    None => return,
                }
            }
        };
        if ready {
            self.focus_active(window, cx);
            return;
        }
        // A tab still loading shows its proposals once it is ready: wait a
        // little for them.
        cx.spawn_in(window, async move |this, cx| {
            for _ in 0..50 {
                let shown = this
                    .update_in(cx, |this, window, cx| {
                        let Some(view) = (this.tabs.get(tab)).and_then(|tab| tab.view().cloned())
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

    /// The first-close note's "Quit when windows close": the
    /// switch goes off and mdoc quits, as it now would with its last window
    /// closed. The hidden window shows only if the quit asks something; a
    /// quit that cannot start brings it back, since no item is left to.
    pub(crate) fn quit_when_windows_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let saved = self.update_general(|general| general.menu_bar = false, cx);
        crate::menu_bar::quit_when_windows_close(cx);
        if let Err(error) = saved {
            // Quitting now would bring the item back at the next launch with
            // no note: show the window and say so instead.
            crate::menu_bar::reveal_if_hidden(window, cx);
            self.notices.post(format!(
                "Could not save \u{201c}Quit when windows close\u{201d}: {error}. \
                 Turn off Settings \u{203a} General \u{203a} Show mdoc in the menu bar to try again."
            ));
            cx.notify();
            return;
        }
        self.quit(window, cx);
        if !self.quit_flow.quitting() {
            crate::menu_bar::reveal_if_hidden(window, cx);
        }
    }

    /// Whether the first-close note has shown.
    pub(crate) fn menu_bar_note_shown(&self) -> bool {
        self.general()
            .is_some_and(|general| general.menu_bar_note_shown)
    }

    /// Whether a shelf drag has dropped, retiring its hint.
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
    /// shelf's notes, the sidebar's dragged width): saved at once, kept in memory if the save fails (the
    /// error is returned), and made in the panel's draft too, so its next
    /// save keeps it beside any choice still saving. Only flags and
    /// switches; never document text.
    pub(crate) fn update_general(
        &mut self,
        change: impl Fn(&mut settings::GeneralConfig) + 'static,
        cx: &mut Context<Self>,
    ) -> Result<(), settings::SettingsError> {
        let (prefs, path) = {
            let store = self.preferences.borrow();
            let Some(current) = store.current.clone() else {
                return Ok(());
            };
            (current, store.path.clone())
        };
        let mut changed = prefs.clone();
        change(&mut changed.general);
        if changed == prefs {
            return Ok(());
        }
        // Taken even without a file, so a Settings save in flight knows it
        // is older.
        let write = settings::next_write();
        let saved = match path {
            Some(path) => settings::save_in_order(&path, &changed, write).map(|_| ()),
            None => Ok(()),
        };
        if let Err(error) = &saved {
            eprintln!("mdoc: could not save a General choice: {error}");
        }
        self.preferences.borrow_mut().current = Some(changed);
        self.settings
            .update(cx, |panel, cx| panel.general_changed(change, cx));
        saved
    }

    /// "Unlock in mdoc…": the Keychain explainer, in the window.
    pub(crate) fn unlock_from_shelf(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.unlock_projects(window, cx);
    }

    /// "Restart to update": the ordinary quit, prompts included.
    pub(crate) fn restart_from_shelf(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.restart_to_update(window, cx);
    }
}
