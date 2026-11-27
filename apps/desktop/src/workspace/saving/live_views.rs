//! The window's live views, most recently viewed first: each one's id, the
//! document it names, its saved record and its document view, which one is
//! active, and whether Home covers it. At most `LIMIT` documents stay live;
//! the least recently viewed beyond them is evicted. Generic over the view,
//! so the recency rules run without a window; the workspace view creates,
//! releases and focuses the documents.
use crate::document::{Background, DocumentView, DocumentViewDependencies};
use crate::workspace::{
    SidebarStatus, Workspace, saving, sidebar, sidebar::navigation::Arrival, tab_label,
};
use gpui::prelude::*;
use gpui::{App, Context, Entity, Focusable, Window, px};
use mdoc_store::{document::Document, session_store::Session};
use std::path::PathBuf;

use mdoc_store::{document_id::DocumentId, session_store::TabRecord};
use std::collections::HashSet;

/// How many documents stay live for their undo and scroll.
pub(in crate::workspace) const LIMIT: usize = 8;

/// What the workspace view carries out for a live-view transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::workspace) enum LiveViewsEffect {
    /// Activate this view: the next most recent, or a new placeholder.
    Activate(u64),
}

/// How eviction weighs a live view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::workspace) enum Weight {
    /// Not a document (Home's placeholder): never counted nor evicted.
    Ignored,
    /// Counted, but kept while it works (a conversion, a prompt, a write).
    Busy,
    /// Counted, and evicted when it is the least recent beyond the limit.
    Idle,
}

pub(in crate::workspace) struct LiveView<V> {
    id: u64,
    record: TabRecord,
    /// The document this view holds, for finding it again.
    document: Option<DocumentId>,
    view: Option<V>,
}

impl<V> LiveView<V> {
    /// A tab outside any set, for tests of how tabs are labelled.
    #[cfg(test)]
    pub(in crate::workspace) fn detached(id: u64, record: TabRecord) -> Self {
        Self {
            id,
            record,
            document: None,
            view: None,
        }
    }

    pub(in crate::workspace) fn id(&self) -> u64 {
        self.id
    }

    /// What the session saves for this tab.
    pub(in crate::workspace) fn record(&self) -> &TabRecord {
        &self.record
    }

    pub(in crate::workspace) fn document(&self) -> Option<&DocumentId> {
        self.document.as_ref()
    }

    /// The document view, once the tab was first shown.
    pub(in crate::workspace) fn view(&self) -> Option<&V> {
        self.view.as_ref()
    }
}

pub(in crate::workspace) struct LiveViews<V> {
    tabs: Vec<LiveView<V>>,
    active: u64,
    next_id: u64,
    /// Home fills the card instead of the active tab.
    home: bool,
}

impl<V> Default for LiveViews<V> {
    fn default() -> Self {
        Self {
            tabs: Vec::new(),
            active: 0,
            next_id: 1,
            home: false,
        }
    }
}

/// Recency order: `tabs[0]` is the most recently viewed.
impl<V> std::ops::Index<usize> for LiveViews<V> {
    type Output = LiveView<V>;

    fn index(&self, index: usize) -> &LiveView<V> {
        &self.tabs[index]
    }
}

impl<'a, V> IntoIterator for &'a LiveViews<V> {
    type Item = &'a LiveView<V>;
    type IntoIter = std::slice::Iter<'a, LiveView<V>>;

    fn into_iter(self) -> Self::IntoIter {
        self.tabs.iter()
    }
}

impl<V> LiveViews<V> {
    pub(in crate::workspace) fn len(&self) -> usize {
        self.tabs.len()
    }

    pub(in crate::workspace) fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    pub(in crate::workspace) fn iter(&self) -> std::slice::Iter<'_, LiveView<V>> {
        self.tabs.iter()
    }

    #[cfg(test)]
    pub(in crate::workspace) fn last(&self) -> Option<&LiveView<V>> {
        self.tabs.last()
    }

    pub(in crate::workspace) fn get(&self, id: u64) -> Option<&LiveView<V>> {
        self.tabs.iter().find(|tab| tab.id == id)
    }

    fn get_mut(&mut self, id: u64) -> Option<&mut LiveView<V>> {
        self.tabs.iter_mut().find(|tab| tab.id == id)
    }

    pub(in crate::workspace) fn position(&self, id: u64) -> Option<usize> {
        self.tabs.iter().position(|tab| tab.id == id)
    }

    pub(in crate::workspace) fn contains(&self, id: u64) -> bool {
        self.tabs.iter().any(|tab| tab.id == id)
    }

    pub(in crate::workspace) fn active(&self) -> u64 {
        self.active
    }

    pub(in crate::workspace) fn active_tab(&self) -> Option<&LiveView<V>> {
        self.get(self.active)
    }

    /// Whether Home was asked for. A placeholder tab also shows Home; the
    /// view knows which tabs are placeholders.
    pub(in crate::workspace) fn home(&self) -> bool {
        self.home
    }

    /// Add a view for `record` as the least recent; activating it brings it
    /// to the front. Returns its id.
    pub(in crate::workspace) fn push(&mut self, record: TabRecord) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let document = record.document();
        self.tabs.push(LiveView {
            id,
            record,
            document,
            view: None,
        });
        id
    }

    /// Name new blanks and older unnamed session records.
    /// The view supplies current titles, including unopened restored tabs,
    /// before any unnamed blank is initialized. Existing names stay intact.
    pub(in crate::workspace) fn name_blank_tabs(
        &mut self,
        names: impl IntoIterator<Item = String>,
    ) {
        let mut used: HashSet<_> = names.into_iter().collect();
        let mut number = 1;
        for tab in &mut self.tabs {
            if !tab.record.blank || tab.record.name.is_some() {
                continue;
            }
            loop {
                let name = if number == 1 {
                    "Untitled.md".to_string()
                } else {
                    format!("Untitled {number}.md")
                };
                number += 1;
                if used.insert(name.clone()) {
                    tab.record.name = Some(name);
                    break;
                }
            }
        }
    }

    /// The tab was first shown: keep its document view.
    pub(in crate::workspace) fn open(&mut self, id: u64, view: V) {
        if let Some(tab) = self.get_mut(id) {
            tab.view = Some(view);
        }
    }

    /// The user acted on blank tab `id` (none: on every tab), so it is no
    /// longer a placeholder that Home may replace.
    pub(in crate::workspace) fn keep_blank(&mut self, id: Option<u64>) {
        for tab in &mut self.tabs {
            if id.is_none_or(|id| id == tab.id) {
                tab.record.blank_disposable = false;
            }
        }
    }

    /// Tab `id` is shown as `name` until its document loads.
    pub(in crate::workspace) fn set_name(&mut self, id: u64, name: Option<String>) {
        if let Some(tab) = self.get_mut(id) {
            tab.record.name = name;
        }
    }

    /// Tab `id` now holds `document`. Returns the document it held.
    pub(in crate::workspace) fn set_document(
        &mut self,
        id: u64,
        document: Option<DocumentId>,
    ) -> Option<DocumentId> {
        let tab = self.get_mut(id)?;
        std::mem::replace(&mut tab.document, document)
    }

    /// Make `id` the active view, the most recent, and leave Home. False for
    /// an unknown view.
    pub(in crate::workspace) fn activate(&mut self, id: u64) -> bool {
        let Some(index) = self.position(id) else {
            return false;
        };
        let tab = self.tabs.remove(index);
        self.tabs.insert(0, tab);
        self.active = id;
        self.home = false;
        true
    }

    /// View `id` becomes the most recent after the active one, which
    /// stays active: the next eviction takes it last but one.
    pub(in crate::workspace) fn behind_active(&mut self, id: u64) {
        let Some(index) = self.position(id) else {
            return;
        };
        let tab = self.tabs.remove(index);
        let at = (self.position(self.active)).map_or(0, |active| active + 1);
        self.tabs.insert(at, tab);
    }

    /// The views `weigh` counts, most recent first.
    pub(in crate::workspace) fn recent(&self, weigh: impl Fn(&LiveView<V>) -> Weight) -> Vec<u64> {
        (self.tabs.iter())
            .filter(|tab| weigh(tab) != Weight::Ignored)
            .map(|tab| tab.id)
            .collect()
    }

    /// The views to evict so that at most `limit` counted ones stay: the
    /// least recent idle ones, never the active one.
    pub(in crate::workspace) fn evictable(
        &self,
        limit: usize,
        weigh: impl Fn(&LiveView<V>) -> Weight,
    ) -> Vec<u64> {
        let weights: Vec<_> = self.tabs.iter().map(|tab| (tab.id, weigh(tab))).collect();
        let mut over = (weights.iter())
            .filter(|(_, weight)| *weight != Weight::Ignored)
            .count()
            .saturating_sub(limit);
        let mut evicted = Vec::new();
        for (id, weight) in weights.iter().rev() {
            if over == 0 {
                break;
            }
            if *weight == Weight::Idle && *id != self.active {
                evicted.push(*id);
                over -= 1;
            }
        }
        evicted
    }

    pub(in crate::workspace) fn show_home(&mut self) {
        self.home = true;
    }

    /// Release view `id`. The last one leaves a placeholder for Home;
    /// releasing the active view activates the next most recent.
    pub(in crate::workspace) fn remove(
        &mut self,
        id: u64,
    ) -> Option<(LiveView<V>, Option<LiveViewsEffect>)> {
        let index = self.position(id)?;
        let tab = self.tabs.remove(index);
        let effect = if self.tabs.is_empty() {
            Some(LiveViewsEffect::Activate(self.push(TabRecord {
                blank: true,
                blank_disposable: true,
                ..TabRecord::default()
            })))
        } else if self.active == id {
            Some(LiveViewsEffect::Activate(
                self.tabs[index.min(self.tabs.len() - 1)].id,
            ))
        } else {
            None
        };
        Some((tab, effect))
    }
}

/// A document as it loaded.
pub(in crate::workspace) struct Loaded {
    document: Document,
    vault: Option<VaultLoaded>,
}

/// What a Vault document brings beside its text.
pub(in crate::workspace) struct VaultLoaded {
    title: String,
    /// A new document's title follows its first line.
    follows: bool,
    /// Its original's viewing copy, when it has a Source to show.
    copy: Option<PathBuf>,
    /// Why its original didn't open, shown in the Source pane.
    source_error: Option<String>,
    /// Admitted and not converted yet.
    unconverted: bool,
    warning: Option<String>,
    left_out: Option<String>,
    /// An Unfiled document's own names, for a load without its review.
    names: Option<LocalNames>,
    /// Its private notes.
    notes: String,
    /// The AI Reply it was promoted from.
    from_reply: Option<String>,
}

/// An Unfiled document's names, as its mapping keeps them.
pub(in crate::workspace) struct LocalNames {
    identities: Vec<mdoc_pii::restore::ExportedIdentity>,
    reserved: Vec<String>,
    not_names: Vec<(String, String)>,
}

/// Read Vault document `id`; a document with a Source gets a viewing copy
/// in `viewing`. Off the main thread.
pub(in crate::workspace) fn load_from_vault(
    vault: &mdoc_vault::Vault,
    id: mdoc_store::document_id::VaultId,
    viewing: Option<PathBuf>,
) -> Result<Loaded, String> {
    let stored = vault
        .document(id.as_str())
        .map_err(|error| format!("Could not open the document. {error}"))?;
    let original = (stored.original.as_ref()).filter(|original| {
        saving::intake::needs_conversion(std::path::Path::new(&original.info.file_name))
    });
    // A source that doesn't open leaves its saved text and review to show,
    // with the error where the Source would be; only a document with
    // nothing else to show fails.
    let (copy, source_error) = match (original, viewing) {
        (Some(original), Some(dir)) => {
            match vault.copy_original_to(id.as_str(), &dir, &original.info.file_name) {
                Ok(copy) => (Some(copy), None),
                Err(error) if stored.pending_conversion() => {
                    return Err(format!("Could not open the document's source. {error}"));
                }
                Err(mdoc_vault::VaultError::NotFound { .. }) => (
                    None,
                    Some(
                        "The original file is missing from the Vault, so it can't be shown. \
                         The Markdown is not affected."
                            .to_string(),
                    ),
                ),
                Err(error) => (
                    None,
                    Some(format!(
                        "The original file could not be opened, so it can't be shown. The \
                         Markdown is not affected. {error}"
                    )),
                ),
            }
        }
        _ => (None, None),
    };
    // Admitted and its text never written, though a move may have changed
    // its place: a text emptied by its user stays so.
    let unconverted = copy.is_some() && stored.pending_conversion();
    let (warning, left_out) = original.map_or((None, None), |original| {
        (
            original.info.import_warning.clone(),
            original.info.left_out.clone(),
        )
    });
    let names = (stored.project.is_none() && !stored.mapping.is_empty()).then(|| LocalNames {
        identities: stored.mapping.exported(),
        reserved: stored.mapping.reserved_aliases_for(&stored.id),
        not_names: stored.mapping.not_names_of(&stored.id),
    });
    let mut document = Document::in_vault(id, stored.markdown);
    document.review = stored.review;
    document.ready = stored.ready_fingerprint.is_some();
    document.scanned = stored.scanned;
    Ok(Loaded {
        document,
        vault: Some(VaultLoaded {
            title: stored.title,
            follows: stored.origin == mdoc_vault::DocumentOrigin::New,
            copy,
            source_error,
            unconverted,
            warning,
            left_out,
            names,
            notes: stored.notes,
            from_reply: stored.from_reply,
        }),
    })
}

impl Workspace {
    pub(crate) fn restore(
        &mut self,
        session: Session,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sidebar
            .restore(session.sidebar_visible, session.sidebar_choice);
        self.theme_choice = session.theme_choice;
        self.sync_app_appearance(cx);
        self.apply_theme(window, cx);
        self.tree.restore(session.expanded_projects);
        self.load_projects(cx);
        let selected = session.active;
        // Session paths were normalized when opened/saved. Do not stat every file
        // during startup; missing files are diagnosed when activated.
        for record in session.tabs {
            if self.tabs.iter().any(|tab| {
                if record.source_only {
                    tab.record().attachment == record.attachment
                } else {
                    !tab.record().source_only && record.same_document(tab.record())
                }
            }) {
                continue;
            }
            let vault = record.document.is_some();
            let id = self.push(record);
            if vault {
                self.name_from_vault(id);
            }
        }
        self.ready = true;
        // Nothing to restore: Home shows over a hidden placeholder tab until
        // it gets content.
        if self.tabs.is_empty() {
            self.push(TabRecord {
                blank: true,
                blank_disposable: true,
                ..TabRecord::default()
            });
        }
        let active = self.tabs[selected.min(self.tabs.len() - 1)].id();
        self.activate(active, window, cx);
        // Sealed with another key: Home asks for the recovery key first.
        self.recovery_screen_opened(window, cx);
        for (paths, project) in std::mem::take(&mut self.pending_open) {
            self.open_paths_into(paths, project, window, cx);
        }
        cx.notify();
    }

    pub(in crate::workspace) fn push(&mut self, record: TabRecord) -> u64 {
        self.tabs.push(record)
    }

    pub(crate) fn active_view(&self) -> Option<Entity<DocumentView>> {
        self.tabs.active_tab()?.view().cloned()
    }

    pub(in crate::workspace) fn initialize(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.tabs.position(id) else {
            return;
        };
        if self.tabs[index].view().is_some() {
            return;
        }
        let names: Vec<_> = self
            .tabs
            .iter()
            .filter(|tab| !tab.record().blank || tab.record().name.is_some())
            .map(|tab| tab_label(tab, cx).name)
            .collect();
        self.tabs.name_blank_tabs(names);
        let record = self.tabs[index].record().clone();
        let document = record.document();
        let theme = self.theme.clone();
        let import_busy = self.import_busy.clone();
        let view = cx.new(|cx| {
            let mut view = DocumentView::new(
                DocumentViewDependencies {
                    closing: self.quit_flow.watch(),
                    preferences: self.preferences.clone(),
                    model_panel: self.settings.clone(),
                    theme,
                    import_busy,
                    ocr: self.ocr_state.clone(),
                    title: self.title.clone(),
                    input_languages: self.input_languages,
                    projects: self.projects.count(),
                },
                window,
                cx,
            );
            view.active = id == self.tabs.active();
            view.session.name = record.blank.then(|| record.name.clone()).flatten();
            view.lifecycle.restore(
                document.is_some(),
                record.source_only,
                record.blank_disposable,
            );
            view.import.queue_auto(
                record.source_only
                    && record.attachment.as_ref().is_some_and(|path| {
                        path.extension().is_some_and(|ext| {
                            ext.eq_ignore_ascii_case("pdf") || ext.eq_ignore_ascii_case("docx")
                        })
                    }),
            );
            view.preview.restore(
                record.attachment.clone(),
                record.preview_visible,
                record.preview_split,
                record
                    .preview_zoom
                    .map(|zoom| (record.preview_page, zoom, record.preview_fit)),
            );
            view
        });
        let events = cx.subscribe_in(&view, window, move |this, _, event, window, cx| {
            this.event(id, event, window, cx);
        });
        let mut status = SidebarStatus::of(view.read(cx));
        let changes = cx.observe(&view, move |_, view, cx| {
            let next = SidebarStatus::of(view.read(cx));
            if next != status {
                status = next;
                cx.notify();
            }
        });
        self.tabs.open(id, view.clone());
        self.tab_subscriptions.insert(id, vec![events, changes]);
        if let Some(document) = document {
            self.load_document(id, document, record, window, cx);
        } else if record.source_only
            && let Some(path) = record.attachment
        {
            view.update(cx, |view, cx| view.open_source(path, window, cx));
        }
    }

    /// Load tab `id`'s document from the Vault; the load's generation when
    /// one started.
    pub(in crate::workspace) fn load_document(
        &mut self,
        id: u64,
        document: DocumentId,
        record: TabRecord,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<u64> {
        #[cfg(test)]
        {
            self.project_ui.member_reads += 1;
        }
        let view = self.tabs.get(id).and_then(|tab| tab.view().cloned())?;
        let (generation, notes_writes) = view.update(cx, |view, cx| {
            let generation = view.lifecycle.begin_load();
            view.error = None;
            cx.notify();
            // Notes written after this read are newer than what it reads.
            (generation, view.notes.writes())
        });
        let weak = view.downgrade();
        let loading = document.vault().clone();
        let task: gpui::Task<Result<Loaded, String>> = match document {
            DocumentId::Vault(document) => match self.vault.vault().cloned() {
                Some(vault) => {
                    let viewing = self.viewing_dir(id);
                    cx.background_executor()
                        .spawn(async move { load_from_vault(&vault, document, viewing) })
                }
                None => {
                    match self.vault.unavailable() {
                        // No Vault this run: say why where the text would be.
                        Some(why) => {
                            let error = format!("This document can't open. {why}");
                            view.update(cx, |view, cx| {
                                view.lifecycle.finish_load(generation, false);
                                view.error = Some(error);
                                cx.notify();
                            });
                        }
                        // It loads once the key opens the Vault.
                        None => self.vault.wait(id),
                    }
                    return None;
                }
            },
        };
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                let moved = this.reloading.get(&loading) == Some(&generation);
                this.move_reload_ended(&loading, generation, window, cx);
                if !this.tabs.contains(id) {
                    // Its viewing copy may have been made after the tab went.
                    if let Some(dir) = this.viewing_dir(id) {
                        let _ = std::fs::remove_dir_all(dir);
                    }
                    return;
                }
                if let Ok(loaded) = &result {
                    this.tabs.set_document(id, loaded.document.id());
                }
                // A dialog, or the recency switcher, keeps the keyboard while
                // the document loads behind it.
                let dialog = this.dialog_up(window) || this.switcher.is_some();
                let _ = weak.update(cx, |view, cx| {
                    if !view.lifecycle.finish_load(generation, result.is_ok()) {
                        return;
                    }
                    match result {
                        Ok(Loaded { document, vault }) => {
                            let review = document.review.clone();
                            view.session.replace(document);
                            let mut copy = None;
                            let mut names = None;
                            let mut source_error = None;
                            if let Some(vault) = vault {
                                source_error = vault.source_error;
                                names = vault.names;
                                view.load_notes(loading.as_str(), vault.notes, notes_writes, cx);
                                view.from_reply = (vault.from_reply).map(|id| {
                                    crate::document::notes::FromReply { id, gone: false }
                                });
                                view.session.name = Some(vault.title);
                                view.session.follows_first_line = vault.follows;
                                view.session.warning = vault.warning;
                                view.session.left_out = vault.left_out;
                                copy = vault.copy;
                                // Admitted but not converted: its source shows
                                // and converts as an opened file does.
                                if vault.unconverted
                                    && let Some(source) = copy.clone()
                                {
                                    view.reset_pii(cx);
                                    view.import
                                        .queue_auto(source.extension().is_some_and(|ext| {
                                            ext.eq_ignore_ascii_case("pdf")
                                                || ext.eq_ignore_ascii_case("docx")
                                        }));
                                    view.open_source(source, window, cx);
                                    if view.active {
                                        view.try_auto_convert(window, cx);
                                    }
                                    cx.notify();
                                    return;
                                }
                                view.session.source = copy.clone();
                            }
                            view.reset_pii(cx);
                            view.editor.update(cx, |editor, cx| {
                                editor.set_text(view.session.document.saved.clone(), cx);
                                let mut caret = record.caret.min(editor.text().len());
                                while !editor.text().is_char_boundary(caret) {
                                    caret -= 1;
                                }
                                editor.set_cursor(caret, cx);
                            });
                            // Its review comes back as it was: no rescan.
                            // Without one (a move cleared it), an Unfiled
                            // document's own names come back.
                            if let Some(review) = review {
                                view.restore_review(review, cx);
                            } else if let Some(names) = names {
                                view.open_names(&names.identities, &names.reserved, cx);
                                view.pii.review_mut().leave_off(&names.not_names);
                            }
                            view.owe_if_left_mid_edit(cx);
                            view.replace_images(view.save_directory(), window, cx);
                            view.scroll
                                .set_offset(gpui::point(px(0.), px(record.scroll_y.min(0.))));
                            // A Vault document's Source is its viewing copy.
                            let from_vault = copy.is_some();
                            let attachment = copy.or(record.attachment);
                            if from_vault {
                                view.preview.restore(
                                    attachment.clone(),
                                    record.preview_visible,
                                    record.preview_split,
                                    record.preview_zoom.map(|zoom| {
                                        (record.preview_page, zoom, record.preview_fit)
                                    }),
                                );
                            }
                            if record.preview_visible
                                && view.preview.generation() == 0
                                && let Some(path) = attachment
                            {
                                view.open_path(path, window, cx);
                            }
                            if let Some(error) = source_error {
                                view.preview.unavailable(error);
                            }
                            if view.active && !dialog {
                                window.focus(&view.editor.read(cx).focus_handle(cx), cx);
                            }
                        }
                        Err(error) => view.error = Some(error),
                    }
                    cx.notify();
                });
                this.attach_project(id, cx);
                // Move to New Project's reload: its name is ready to edit.
                if moved {
                    this.name_moved_project(Some(id), &loading, window, cx);
                }
                // Opened from an AI notification: for review.
                this.review_if_asked(id, window, cx);
                this.refresh_saved_replies(cx);
                // A view kept while it loaded may be evicted now.
                this.evict(window, cx);
                cx.notify();
            });
        })
        .detach();
        Some(generation)
    }

    /// Where tab `id` keeps its viewing copy.
    pub(in crate::workspace) fn viewing_dir(&self, id: u64) -> Option<PathBuf> {
        Some(self.viewing_root.as_ref()?.join(id.to_string()))
    }

    pub(in crate::workspace) fn activate(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.activate_as(id, Arrival::Visit, window, cx);
    }

    /// Activate tab `id`, recording it in Back/Forward as `arrival` says.
    pub(in crate::workspace) fn activate_as(
        &mut self,
        id: u64,
        arrival: Arrival,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.quit_flow.finishing() || !self.tabs.contains(id) {
            return;
        }
        self.project_ui.page = None;
        self.reply_page = None;
        self.end_rename(true, cx);
        // Viewed again before its eviction's write ended: it stays. An
        // explicit close (a Project's Close all) goes on.
        if self.evicting.remove(&id) {
            self.closing_after_write.remove(&id);
        }
        let left = self.tabs.active();
        if let Some(old) = self.active_view() {
            old.update(cx, |view, _| {
                view.active = false;
                view.pii.dismiss_popup();
            });
        }
        self.tabs.activate(id);
        self.initialize(id, window, cx);
        if let Some(view) = self.active_view() {
            view.update(cx, |view, cx| {
                view.active = true;
                view.try_auto_convert(window, cx);
                if view.lifecycle.source_only() {
                    window.focus(&view.focus, cx);
                } else if view.lifecycle.loading() || view.lifecycle.unavailable() {
                    window.focus(&self.focus, cx);
                } else {
                    window.focus(&view.editor.read(cx).focus_handle(cx), cx);
                }
            });
        }
        // Home covers a placeholder's editor; keys go to the workspace, or
        // they would reach nothing.
        if self.showing_overview(cx) {
            window.focus(&self.focus, cx);
        }
        self.note_destination(arrival, cx);
        if left != id {
            self.drop_if_empty_new(left, window, cx);
        }
        self.evict(window, cx);
        cx.notify();
    }

    pub(crate) fn new_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) -> u64 {
        // From Home, the hidden placeholder becomes the new tab.
        if let Some(id) = self
            .tabs
            .iter()
            .find(|tab| self.is_placeholder(tab, cx))
            .map(|tab| tab.id())
        {
            self.tabs.keep_blank(Some(id));
            if let Some(view) = self.tabs.get(id).and_then(|tab| tab.view()) {
                view.update(cx, |view, _| view.lifecycle.keep_blank());
            }
            self.activate(id, window, cx);
            return id;
        }
        self.tabs.keep_blank(None);
        for tab in &self.tabs {
            if let Some(view) = tab.view() {
                view.update(cx, |view, _| view.lifecycle.keep_blank());
            }
        }
        let id = self.push(TabRecord {
            blank: true,
            ..TabRecord::default()
        });
        self.activate(id, window, cx);
        id
    }

    pub fn resolving_close(&self) -> bool {
        self.quit_flow.resolving()
    }

    pub(in crate::workspace) fn close_tab(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.quit_flow.may_close() {
            return;
        }
        if !self.tabs.contains(id) {
            return;
        }
        // A member's latest mapping reaches its Project before the tab goes.
        self.project_changed(id, cx);
        let Some(tab) = self.tabs.get(id) else {
            return;
        };
        if tab
            .view()
            .is_some_and(|view| view.read(cx).lifecycle.prompting())
        {
            return;
        }
        // Its last text is written before the tab goes; a failed write keeps
        // the tab, and its edit, open.
        self.autosave(id, window, cx);
        if self.write_pending_for(id, cx) {
            self.closing_after_write.insert(id);
            return;
        }
        // Notes whose write just failed keep the tab, as its text would.
        if (self.tabs.get(id).and_then(|tab| tab.view()))
            .is_some_and(|view| view.read(cx).notes.unsaved())
        {
            return;
        }
        self.remove(id, window, cx);
    }

    pub(in crate::workspace) fn release(
        view: &Entity<DocumentView>,
        window: &mut Window,
        cx: &mut App,
    ) {
        view.update(cx, |view, cx| {
            view.session.generation = view.session.generation.wrapping_add(1);
            view.import.cancel();
            view.close_preview(window, cx);
            view.images.release(window, cx);
        });
    }

    pub(in crate::workspace) fn remove(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(switcher) = &mut self.switcher
            && !switcher.forget(id)
        {
            self.switcher = None;
        }
        self.closing_after_write.remove(&id);
        self.evicting.remove(&id);
        self.unsaved.remove(&id);
        self.vault.forget(id);
        self.admitted_from.remove(&id);
        if let Some(dir) = self.viewing_dir(id) {
            let _ = std::fs::remove_dir_all(dir);
        }
        let Some(tab) = self.tabs.get(id) else {
            return;
        };
        if let Some(view) = tab.view().cloned() {
            // A closed document can't report its running jobs; their end
            // still frees the import slot and the DOCX worker.
            let background = view.update(cx, |view, _| view.take_background());
            for task in background {
                cx.spawn_in(window, async move |this, cx| {
                    let done = task.await;
                    let _ = this.update_in(cx, |this, window, cx| match done {
                        Background::Import => this.refresh_import(window, cx),
                        Background::Docx { generation } => {
                            this.docx_finished(id, generation, window, cx)
                        }
                    });
                })
                .detach();
            }
            Self::release(&view, window, cx);
        }
        let Some((_, effect)) = self.tabs.remove(id) else {
            return;
        };
        self.tab_subscriptions.remove(&id);
        self.docx_queue.forget(id);
        // The tab that takes a closed tab's page takes its stop too; it is
        // no new stop when the closed tab wasn't the current one.
        let gone = sidebar::navigation::Stop::Tab(id);
        let arrival = if self.navigation.current() == Some(&gone) {
            Arrival::Replace
        } else {
            Arrival::Unrecorded
        };
        if let Some(LiveViewsEffect::Activate(next)) = effect {
            if self.tabs.home() || self.project_ui.page.is_some() {
                // Home, or a Project page, stays over the next most recent
                // view.
                let home = self.tabs.home();
                self.tabs.activate(next);
                if home {
                    self.tabs.show_home();
                }
            } else {
                self.activate_as(next, arrival, window, cx);
            }
        }
        self.navigation.forget(&gone);
        self.checkpoint(window, cx);
        cx.notify();
    }
}

#[cfg(test)]
impl Workspace {
    /// Release the shown live view, as eviction does.
    pub(crate) fn release_active(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_tab(self.tabs.active(), window, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vault(hex: &str) -> DocumentId {
        DocumentId::Vault(mdoc_store::document_id::VaultId::parse(&hex.repeat(16)).unwrap())
    }

    fn named(hex: &str) -> TabRecord {
        let mut record = TabRecord::default();
        record.set_document(vault(hex));
        record
    }

    #[test]
    fn tabs_get_new_ids_in_order_and_a_record_names_its_document() {
        let mut tabs = LiveViews::<u8>::default();
        let a = tabs.push(named("0a"));
        let b = tabs.push(TabRecord::default());
        assert!(b > a);
        assert_eq!(tabs.get(a).unwrap().document(), Some(&vault("0a")));
        assert_eq!(tabs.get(b).unwrap().document(), None);
        assert_eq!(tabs.get(a).unwrap().view(), None);
        tabs.open(a, 7);
        assert_eq!(tabs.get(a).unwrap().view(), Some(&7));
        assert!(tabs.activate(b));
        assert!(!tabs.activate(99));
        assert_eq!(tabs.active(), b);
        tabs.show_home();
        assert!(tabs.home());
        assert!(tabs.activate(a));
        assert!(!tabs.home());
        assert_eq!(tabs.active_tab().map(LiveView::id), Some(a));
    }

    #[test]
    fn releasing_activates_the_next_most_recent_and_the_last_leaves_a_placeholder() {
        let mut tabs = LiveViews::<u8>::default();
        let [a, b, c] = [(); 3].map(|_| tabs.push(TabRecord::default()));
        tabs.activate(a);
        tabs.activate(c);
        tabs.activate(b);
        let (closed, effect) = tabs.remove(b).unwrap();
        assert_eq!(closed.id(), b);
        assert_eq!(effect, Some(LiveViewsEffect::Activate(c)));
        tabs.activate(c);
        assert_eq!(
            tabs.remove(c).unwrap().1,
            Some(LiveViewsEffect::Activate(a))
        );
        tabs.activate(a);
        assert!(tabs.remove(99).is_none());
        let Some((_, Some(LiveViewsEffect::Activate(placeholder)))) = tabs.remove(a) else {
            panic!("the last tab leaves a placeholder");
        };
        assert_eq!(tabs.len(), 1);
        let record = tabs.get(placeholder).unwrap().record();
        assert!(record.blank && record.blank_disposable);
    }

    #[test]
    fn records_change_only_through_named_transitions() {
        let mut tabs = LiveViews::<u8>::default();
        let [a, b] = [(); 2].map(|_| {
            tabs.push(TabRecord {
                blank: true,
                blank_disposable: true,
                ..TabRecord::default()
            })
        });
        tabs.keep_blank(Some(a));
        assert!(!tabs.get(a).unwrap().record().blank_disposable);
        assert!(tabs.get(b).unwrap().record().blank_disposable);
        tabs.keep_blank(None);
        assert!(!tabs.get(b).unwrap().record().blank_disposable);
        assert_eq!(tabs.set_document(a, Some(vault("0a"))), None);
        assert_eq!(tabs.set_document(a, Some(vault("0b"))), Some(vault("0a")));
        assert_eq!(tabs.set_document(99, None), None);
    }

    #[test]
    fn closing_an_inactive_tab_keeps_the_active_one() {
        let mut tabs = LiveViews::<u8>::default();
        let [a, b] = [(); 2].map(|_| tabs.push(TabRecord::default()));
        tabs.activate(a);
        assert_eq!(tabs.remove(b).unwrap().1, None);
        assert_eq!(tabs.active(), a);
    }

    #[test]
    fn viewing_brings_a_view_to_the_front_and_eviction_takes_the_least_recent_idle() {
        let mut tabs = LiveViews::<u8>::default();
        let ids: Vec<u64> = (0..10).map(|_| tabs.push(TabRecord::default())).collect();
        for id in &ids {
            tabs.activate(*id);
        }
        let all = |_: &LiveView<u8>| Weight::Idle;
        assert_eq!(
            tabs.recent(all),
            ids.iter().rev().copied().collect::<Vec<_>>()
        );
        // Ten views, a limit of eight: the two viewed first go.
        assert_eq!(tabs.evictable(LIMIT, all), [ids[0], ids[1]]);
        // A busy one is kept, so the next least recent goes in its place.
        let busy = ids[0];
        let weigh = |tab: &LiveView<u8>| {
            if tab.id() == busy {
                Weight::Busy
            } else {
                Weight::Idle
            }
        };
        assert_eq!(tabs.evictable(LIMIT, weigh), [ids[1], ids[2]]);
        // Uncounted views neither count nor go; the active one never goes.
        let placeholder = ids[1];
        let weigh = |tab: &LiveView<u8>| {
            if tab.id() == placeholder {
                Weight::Ignored
            } else {
                Weight::Idle
            }
        };
        assert_eq!(tabs.evictable(LIMIT, weigh), [ids[0]]);
        tabs.activate(ids[0]);
        assert_eq!(tabs.evictable(LIMIT, all), [ids[1], ids[2]]);
        assert_eq!(tabs.evictable(1, all).len(), 9);
        assert!(!tabs.evictable(0, all).contains(&ids[0]));
        assert_eq!(tabs.position(ids[0]), Some(0));
        assert_eq!(tabs.position(ids[9]), Some(1));
    }
}
