//! What the window gives the menu bar shelf: its open tabs with applied
//! aliases, then the Vault's other documents, each ready only once the
//! Vault's outbound check passes; and its Projects' names for the
//! clipboard rows.

mod actions;
mod clipboard;

pub(crate) use clipboard::{ClipboardNames, ClipboardSource};

use super::{ProjectLink, Workspace, saving::document_writer::Target};
use crate::{
    document::DocumentView,
    review::refusal::{Refusal, Step},
    shelf::{self, OpenTab, Outbound, ShelfDocument, ShelfKey, StoredDocument},
};
use gpui::{App, Context, Window};
use mdoc_pii::restore::ExportedIdentity;
use mdoc_store::{
    document_id::{DocumentId, VaultId},
    settings,
};
use mdoc_vault::{DocumentSummary, Vault, VaultError};
use std::{
    collections::HashSet,
    time::{Duration, SystemTime},
};

/// What the window keeps for the shelf and its Copy for AI; everything
/// else the shelf shows is read fresh from the tabs and the Vault.
#[derive(Default)]
pub(super) struct ShelfSource {
    /// The tab whose Copy for AI waits for its writes to land before the
    /// Vault's outbound check runs, with the clipboard's text when it was
    /// asked.
    pub(super) copy_for_ai_waiting: Option<(u64, Option<String>)>,
}

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

    /// Tab `id`'s row for the editor's Copy for AI when the shelf lists
    /// none: scanned with nothing found to replace (no alias), checked as a
    /// listed row is.
    fn unlisted_tab_row(&self, id: u64, cx: &App) -> Option<ShelfDocument> {
        let view = self.tabs.get(id)?.view()?.read(cx);
        if !view.pii.review().applied().is_empty() {
            return None;
        }
        let names = view.known_identities();
        let title = view.display_name();
        let mut tab = open_tab(id, view, &title, &names, cx)?;
        if tab.review_readiness().is_ready() {
            tab.stored = self.outbound(id, view, cx);
        }
        Some(ShelfDocument::of_any_tab(tab))
    }

    /// The outbound check of tab `id`'s Vault document, once what it shows
    /// is written; `None` until then.
    fn outbound(&self, id: u64, view: &DocumentView, cx: &App) -> Option<Outbound> {
        let vault = self.vault.vault()?;
        let Some(DocumentId::Vault(document)) = view.session.document.id() else {
            return None;
        };
        let unwritten = view.unsaved(cx)
            || self.projects.moving.contains(&document)
            || self.write_pending_for(id, cx)
            || view.saved_review() != view.session.document.review
            || super::saving::autosave::ready_unwritten(view, cx)
            || (view.project.link()).is_some_and(|link| self.project_unwritten(&link.id, cx));
        if unwritten {
            return None;
        }
        Some(checked(vault.text_for_ai(document.as_str())))
    }

    /// The name row `key`'s document leaves under as a drag's file,
    /// nothing of its title (#1208): its `Vault::share_name`. `None` outside
    /// the Vault, where a tab is never ready.
    pub(crate) fn shelf_share_name(&self, key: &ShelfKey, cx: &App) -> Option<String> {
        let vault = self.vault.vault()?;
        match key {
            ShelfKey::Tab(id) => match self.tabs.get(*id)?.view()?.read(cx).session.document.id() {
                Some(DocumentId::Vault(document)) => vault.share_name(document.as_str()).ok(),
                _ => None,
            },
            ShelfKey::Document(id) => vault.share_name(id.as_str()).ok(),
        }
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
        let unwritten = self.projects.moving.contains(&id)
            || (document.project.as_deref()).is_some_and(|p| self.project_unwritten(p, cx));
        ShelfDocument::of_stored(StoredDocument {
            id,
            title: &document.title,
            text: &document.markdown,
            names,
            project,
            edited: SystemTime::UNIX_EPOCH + Duration::from_secs(document.updated),
            scanned: document.scanned || document.review.is_some(),
            moved: document.scanned
                && document.review.is_none()
                && document.ready_fingerprint.is_none(),
            outbound: (!unwritten).then(|| checked(vault.text_for_ai(&summary.id))),
        })
    }

    /// Copy for AI in tab `id`: its text leaves only as a shelf row's
    /// would, once the Vault's outbound check of what it wrote passes
    /// against the names its Project holds now (#1091 Decision 19). What
    /// it shows is written first, and the copy waits for the writes.
    pub(super) fn copy_tab_for_ai(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        self.shelf_source.copy_for_ai_waiting = None;
        self.autosave(id, window, cx);
        let clipboard = clipboard_text(cx);
        self.try_copy_for_ai(id, clipboard, cx);
    }

    /// The writes a waiting Copy for AI waited for have landed: check it
    /// again.
    pub(super) fn copy_for_ai_after_write(&mut self, cx: &mut Context<Self>) {
        if let Some((id, clipboard)) = self.shelf_source.copy_for_ai_waiting.take() {
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
        let outcome = match (self.shelf_row(&ShelfKey::Tab(id), cx))
            .or_else(|| self.unlisted_tab_row(id, cx))
        {
            Some(row) => match (row.text_for_ai(), row.readiness) {
                (Some(text), _) => Ok(text.to_owned()),
                (None, shelf::Readiness::Checking)
                    if self.document_writer.busy() || view.read(cx).finding_names() =>
                {
                    self.shelf_source.copy_for_ai_waiting = Some((id, clipboard));
                    return;
                }
                (None, readiness) => Err(Refusal::of(
                    readiness,
                    row.scanned,
                    self.unsaved.contains(&id),
                )),
            },
            None => Err(self.no_row_refusal(&view, cx)),
        };
        view.update(cx, |view, cx| match outcome {
            Ok(text) => view.copy_checked_for_ai(text, cx),
            Err(why) => view.copy_for_ai_refused(why, cx),
        });
    }

    /// Run a refused Copy for AI's step in tab `id`, unless its document is
    /// kept from AI: then nothing changes in it ("Replace N and copy" would
    /// edit it to copy nothing) and the notice says why. Returns whether the
    /// step found what it shows (`DocumentView::run_copy_step`).
    pub(super) fn run_tab_step(
        &mut self,
        id: u64,
        step: Step,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(view) = self.tabs.get(id).and_then(|tab| tab.view().cloned()) else {
            return true;
        };
        if self.excluded(&view, cx) {
            view.update(cx, |view, cx| {
                view.copy_for_ai_refused(Refusal::excluded(), cx)
            });
            return true;
        }
        view.update(cx, |view, cx| view.run_copy_step(step, window, cx))
    }

    /// Whether tab `view`'s Vault document is kept from AI.
    fn excluded(&self, view: &gpui::Entity<DocumentView>, cx: &App) -> bool {
        match (self.vault.vault(), view.read(cx).session.document.id()) {
            (Some(vault), Some(DocumentId::Vault(document))) => (vault.documents().iter())
                .any(|summary| summary.id == document.as_str() && summary.exclude_from_ai),
            _ => false,
        }
    }

    /// Why a tab has no shelf row to copy, and what resolves it.
    fn no_row_refusal(&self, view: &gpui::Entity<DocumentView>, cx: &App) -> Refusal {
        if self.excluded(view, cx) {
            Refusal::excluded()
        } else {
            view.read(cx).no_alias_refusal()
        }
    }

    /// Project `id`'s names in the window are not all in the Vault yet: its
    /// write waits, runs (on its own or with a member's save) or failed, or
    /// an open member's mapping change waits to be folded into it.
    pub(super) fn project_unwritten(&self, id: &str, cx: &App) -> bool {
        self.project_ui.is_unwritten(id)
            || self.document_writer.has(&Target::Project(id.to_string()))
            || self.projects_saving.contains(id)
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
    // Scanned with nothing found to replace is reviewed too.
    let reviewed = !review.applied().is_empty()
        || !view.pii.scans().is_empty()
        || view.session.document.scanned;
    if !reviewed || !view.lifecycle.ready() {
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
        checking: review.awaiting_discovery() || review.edits_unscanned(),
        names: names.to_vec(),
        project: view.project.link().cloned(),
        edited: view.lifecycle.edited_at().unwrap_or(SystemTime::UNIX_EPOCH),
        stored: None,
    })
}

impl Workspace {
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
            eprintln!("{}", crate::shelf::text::general_not_saved(error));
        }
        self.preferences.borrow_mut().current = Some(changed);
        self.settings
            .update(cx, |panel, cx| panel.general_changed(change, cx));
        saved
    }
}
