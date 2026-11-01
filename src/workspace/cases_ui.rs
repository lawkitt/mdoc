//! Saved cases in the window (ADR 0041, phase 2): create a case from a tab's
//! mapping, save changes in the background, give reopened members their
//! mapping back, rename, remove, delete, and the Open Case list.
use super::{
    CloseEvent, TabDrag, TabEvent, Workspace,
    case_coordinator::{CaseEffect, CaseLoad, Flush, KeyWait},
};
use crate::{
    document_view::{DocumentView, Next},
    markdown_search, style, ui,
};
use gpui::{AnyElement, App, Context, Entity, Focusable, PromptLevel, Window, div, prelude::*, px};
use mdoc_store::{
    cases::{Case, CaseError, CaseLink, Store},
    session_store,
};
use std::{collections::HashSet, path::PathBuf, sync::Arc};

gpui::actions!(
    cases,
    [ConfirmCaseName, CancelCaseName, AddCaseName, ClearCaseName]
);

pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys(crate::shortcuts::bindings(crate::shortcuts::Owner::Cases));
}

#[derive(Default)]
pub(super) struct CaseState {
    pub(super) page: Option<super::case_page::CasePage>,
    pub(super) header_menu: Option<(String, gpui::Point<gpui::Pixels>)>,
    pub(super) closing_members: super::case_page::CaseClose,
    pub(super) renaming: Option<(String, Entity<markdown_search::SearchInput>)>,
    /// Saves the rename when focus leaves its field.
    rename_blur: Option<gpui::Subscription>,
    pub(super) list_open: bool,
    /// The link the Open Case list hangs from; none centers it.
    list_anchor: Option<gpui::Bounds<gpui::Pixels>>,
    /// Where Home's Open a case… link was last painted.
    pub(super) home_link: std::rc::Rc<std::cell::Cell<Option<gpui::Bounds<gpui::Pixels>>>>,
    /// Cases whose member rows are folded under their header.
    pub(super) collapsed: HashSet<String>,
    /// The Keychain explainer is shown (ADR 0043).
    pub(super) explainer: bool,
    /// What runs once the explainer's Continue opened the key.
    then: Option<AfterUnlock>,
    /// The tab whose Save as Case, answered while quitting, waits for its
    /// document to save; the quit goes on once its case is queued (ADR 0071).
    quit_case: Option<u64>,
    /// Tabs whose mapping this quit already asked about; a cancelled quit
    /// asks again.
    quit_asked: Vec<u64>,
    /// The quit waits on the Keychain explainer to save this tab's case.
    quit_key_wait: Option<u64>,
    /// The user-wide lists' last saved revision (ADR 0092).
    pub(super) user_lists_revision: u64,
    /// A close or quit question about a mapping is up; a quit waits for it.
    mapping_prompt: bool,
    /// Stands in for "reading the key may ask" in tests.
    #[cfg(test)]
    pub(super) test_may_ask: bool,
    /// What opening the key yields in tests: a store, or an error.
    #[cfg(test)]
    pub(super) test_open: Option<Result<Store, CaseError>>,
    /// How often the key was read, quietly or not, in tests.
    #[cfg(test)]
    pub(super) key_reads: usize,
    /// Member loader invocations, including attempts that would fail to read.
    #[cfg(test)]
    pub(super) member_reads: usize,
}

type AfterUnlock = Box<dyn FnOnce(&mut Workspace, &mut Window, &mut Context<Workspace>)>;

impl Workspace {
    /// The store, opening the OS-held key on first use.
    pub(super) fn case_store(&mut self) -> Result<Arc<Store>, CaseError> {
        if let Some(store) = self.case_coordinator.store() {
            return Ok(store.clone());
        }
        #[cfg(test)]
        {
            self.cases.key_reads += 1;
        }
        // Tests inject a store; they never reach the real key or app data.
        let open = if cfg!(test) {
            self.test_open()
        } else {
            Store::default_dir()
                .ok_or(CaseError::NoAppData)
                .and_then(Store::open)
        };
        let store = Arc::new(open?);
        self.case_coordinator.open(store.clone());
        Ok(store)
    }
    #[cfg(test)]
    fn test_open(&mut self) -> Result<Store, CaseError> {
        self.cases
            .test_open
            .take()
            .unwrap_or(Err(CaseError::NoKeyStore))
    }
    #[cfg(not(test))]
    fn test_open(&mut self) -> Result<Store, CaseError> {
        unreachable!()
    }
    /// Opening the key will ask (as on a Mac that has not allowed mdoc yet)
    /// and then yield `open`.
    #[cfg(test)]
    pub(super) fn stage_case_key(&mut self, open: Result<Store, CaseError>) {
        self.case_coordinator.reopen(None);
        self.cases.test_may_ask = true;
        self.cases.test_open = Some(open);
    }
    #[cfg(any(
        test,
        all(
            debug_assertions,
            feature = "headless-renders",
            any(target_os = "macos", target_os = "windows")
        )
    ))]
    pub(super) fn use_case_store(&mut self, store: Store) {
        self.case_coordinator.reopen(Some(Arc::new(store)));
    }
    /// Whether reading the key now would show the macOS password dialog.
    /// The key is read with dialogs off first, so a read that needs no
    /// dialog just opens the store (ADR 0043).
    fn key_may_ask(&mut self) -> bool {
        if self.case_coordinator.store().is_some() {
            return false;
        }
        #[cfg(test)]
        let staged = {
            self.cases.key_reads += 1;
            self.cases.test_may_ask
        };
        #[cfg(not(test))]
        let staged = false;
        if cfg!(test) {
            return staged;
        }
        let Some(dir) = Store::default_dir() else {
            return false;
        };
        match Store::open_quietly(dir) {
            Ok(Some(store)) => {
                self.case_coordinator.open(Arc::new(store));
                false
            }
            Ok(None) => true,
            // macOS, unsure: explain first. Continue's own attempt reports the
            // error, and launch leaves cases locked rather than read loudly
            // (ADR 0045). Elsewhere the quiet read is the real one, so its
            // error is reported by the action's own attempt, without a
            // Keychain explainer.
            Err(_) => cfg!(target_os = "macos"),
        }
    }
    /// True when a case action can use the key now. Otherwise the Keychain
    /// explainer opens and `then` runs after its Continue (ADR 0043).
    pub(super) fn case_key_ready(
        &mut self,
        then: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.key_may_ask() {
            return true;
        }
        self.cases.explainer = true;
        self.explainer_focus_pending = true;
        // An action already waiting (open explainer, or declined and awaiting
        // Try again) still runs, before this one.
        let earlier = self.cases.then.take();
        self.cases.then = Some(Box::new(move |this, window, cx| {
            if let Some(earlier) = earlier {
                earlier(this, window, cx);
            }
            then(this, window, cx);
        }));
        cx.notify();
        false
    }
    /// Show the explainer for locked or denied cases; Continue loads them.
    /// An action whose dialog was declined is still waiting and runs once
    /// the key opens.
    pub(super) fn unlock_cases(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.key_may_ask() {
            self.cases.explainer = true;
            self.explainer_focus_pending = true;
            cx.notify();
        } else {
            self.continue_unlock(window, cx);
        }
    }
    /// The explainer's Continue: open the key (macOS may ask now), then load
    /// locked members and run the waiting action.
    pub(super) fn continue_unlock(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_explainer(window, cx);
        let then = self.cases.then.take();
        match self.case_store() {
            Ok(_) => {
                self.case_coordinator.deny(false);
                self.resume_locked(window, cx);
                if let Some(then) = then {
                    then(self, window, cx);
                }
            }
            Err(error) => {
                // Declined: Try again still owes the user the action they
                // asked for; Not now cancels it.
                if matches!(error, CaseError::KeyDenied) {
                    self.cases.then = then;
                }
                // A folder sheet waiting on the key opens its files
                // without a case, and its notice says why (ADR 0075).
                let folder = self.folder_key_failed(&error, window, cx);
                if !folder || matches!(error, CaseError::KeyDenied) {
                    self.key_failed(error);
                }
                self.stop_quit_key_wait(window, cx);
            }
        }
        cx.notify();
    }
    /// Once the store is open, members left locked at launch load and get
    /// their mappings, however the key became readable.
    pub(super) fn resume_locked(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(CaseLoad::Load) = self.case_coordinator.unlock() {
            self.load_cases(window, cx);
        } else if self.case_coordinator.store().is_some() && !self.case_coordinator.loaded() {
            self.load_cases(window, cx);
        }
    }
    /// The explainer's Not now: the waiting action is cancelled.
    pub(super) fn cancel_unlock(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_explainer(window, cx);
        self.cases.then = None;
        // A folder sheet's case action returns to its sheet.
        self.folder_key_cancelled();
        self.stop_quit_key_wait(window, cx);
        cx.notify();
    }
    /// Hide the explainer and give focus back to where it was.
    fn close_explainer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.cases.explainer {
            return;
        }
        self.cases.explainer = false;
        let focus = self
            .explainer_return
            .take()
            .unwrap_or_else(|| self.focus.clone());
        window.focus(&focus, cx);
    }
    /// A declined dialog keeps members locked behind Try again; other errors
    /// are reported as before.
    fn key_failed(&mut self, error: CaseError) {
        if matches!(error, CaseError::KeyDenied) {
            self.case_coordinator.deny(true);
            // The action the user asked for is waiting: show Try again.
            self.carry_out_notices(self.notices.show_locked_cases());
        } else {
            self.notices.post(error.to_string());
        }
    }
    /// The banner for members waiting on the key, or a declined dialog.
    pub(super) fn case_key_banner(&self) -> Option<(gpui::SharedString, &'static str)> {
        Some(match self.case_coordinator.key_wait()? {
            KeyWait::Denied => (
                "Saved cases need Keychain access to open their replacements.".into(),
                "Try again…",
            ),
            // Any opened document may be a member: a manually opened one is
            // only matched to its case by path, which needs the cases loaded.
            KeyWait::Locked => (
                match self.case_coordinator.locked_count() {
                    Some(1) => {
                        "1 saved case is locked. Unlock it to restore replacements in its documents."
                            .into()
                    }
                    Some(n) if n > 1 => format!(
                        "{n} saved cases are locked. Unlock them to restore replacements in their documents."
                    )
                    .into(),
                    _ => "Saved cases are locked. Unlock them to restore replacements in their documents."
                        .into(),
                },
                "Unlock…",
            ),
        })
    }
    /// The Open Case list's line above Unlock, when cases wait for the key;
    /// none when the folder holds no case files.
    fn case_list_locked_line(&self) -> Option<String> {
        self.case_coordinator.key_wait()?;
        match self.case_coordinator.locked_count() {
            Some(0) => None,
            Some(1) => Some("1 saved case is locked.".into()),
            Some(n) => Some(format!("{n} saved cases are locked.")),
            None => Some("Saved cases are locked.".into()),
        }
    }
    /// Count the locked cases by file name; never the key. Tests skip it:
    /// the default folder is not theirs.
    fn count_locked_cases(&mut self) {
        if !cfg!(test) {
            let files = Store::default_dir().and_then(|dir| Store::count(&dir));
            self.case_coordinator.count_locked(files);
        }
    }
    /// Whether the locked-cases notice should open by itself: the key was
    /// declined after an action, or an open tab belongs to a case whose
    /// mapping is locked. Otherwise it waits behind the bell (ADR 0043).
    pub(super) fn case_key_needed(&self) -> bool {
        self.case_key_banner().is_some()
            && (self.case_coordinator.denied()
                || self.tabs.iter().any(|tab| tab.record().case.is_some()))
    }
    /// Read every case in the background, then attach loaded member tabs.
    /// Nothing touches the key until a cases folder exists, and launch never
    /// shows the Mac password dialog: it reads the key with dialogs off and
    /// otherwise leaves members locked (ADR 0043, ADR 0045).
    pub(super) fn load_cases(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.case_coordinator.may_load() {
            return;
        }
        let store = match self.case_coordinator.store().cloned() {
            Some(store) => store,
            None => {
                let exists = Store::default_dir().is_some_and(|d| d.exists());
                // A cases folder: the key exists, but launch only reads it
                // when that needs no dialog.
                if (exists || cfg!(test)) && self.key_may_ask() {
                    self.case_coordinator.lock();
                    self.count_locked_cases();
                    cx.notify();
                    return;
                }
                if !exists || cfg!(test) {
                    self.case_coordinator.mark_loaded();
                    // No cases folder: no lists saved yet either.
                    self.user_lists_read(Ok(Default::default()), cx);
                    return;
                }
                // On macOS the quiet read has opened the store by now, so this
                // never reads loudly at launch (ADR 0045); elsewhere it reports
                // the key store's own error.
                match self.case_store() {
                    Ok(store) => store,
                    Err(error) => {
                        // Stay unloaded so members keep their case link.
                        self.case_coordinator.lock();
                        self.count_locked_cases();
                        self.key_failed(error);
                        cx.notify();
                        return;
                    }
                }
            }
        };
        let CaseLoad::List = self.case_coordinator.begin_load() else {
            return;
        };
        let task = cx
            .background_executor()
            .spawn(async move { (store.list(), store.load_lists()) });
        cx.spawn_in(window, async move |this, cx| {
            let ((cases, failed), lists) = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.case_coordinator.finish_load(cases);
                this.user_lists_read(lists, cx);
                if this
                    .cases
                    .page
                    .as_ref()
                    .is_some_and(|page| this.case_coordinator.case(page.id()).is_none())
                {
                    this.show_home(window, cx);
                    this.notices.post(
                        "The saved case could not be opened. It may have been removed or damaged.",
                    );
                }
                if !failed.is_empty() {
                    this.notices.post(format!(
                        "{} saved {} could not be opened. {} damaged or saved with another key.",
                        failed.len(),
                        if failed.len() == 1 { "case" } else { "cases" },
                        if failed.len() == 1 {
                            "It is"
                        } else {
                            "They are"
                        },
                    ));
                }
                let ids: Vec<_> = this.tabs.iter().map(|t| t.id()).collect();
                for id in ids {
                    this.attach_case(id, window, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }
    /// After a member document loads, give it its case's mapping.
    pub(super) fn attach_case(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if !self.case_coordinator.loaded() {
            self.load_cases(window, cx);
            return;
        }
        let Some(tab) = self.tabs.get(id) else {
            return;
        };
        let Some(view) = tab.view().cloned() else {
            return;
        };
        let wanted = tab.record().case.clone();
        let identity = tab.identity().cloned();
        let (loaded, has_case) = {
            let v = view.read(cx);
            (
                !v.lifecycle.loading() && !v.lifecycle.unavailable() && !v.lifecycle.source_only(),
                v.case.link().is_some(),
            )
        };
        if !loaded || has_case {
            return;
        }
        let case = self.case_coordinator.cases().iter().find(|case| {
            wanted.as_deref() == Some(case.id.as_str())
                || identity.as_ref().is_some_and(|p| case.members.contains(p))
        });
        match case {
            Some(case) => {
                let link = CaseLink::of(case);
                let identities = case.exported();
                let reserved = case.reserved_aliases();
                let skip = case.skipped();
                let lists = case.lists.clone();
                let dismissed = case.dismissed_hints.clone();
                let case_id = link.id.clone();
                view.update(cx, |view, cx| {
                    view.case.set_skip(skip);
                    view.case.set_lists(lists);
                    view.dismiss_case_hints(&dismissed, cx);
                    view.open_case_mapping(link, &identities, &reserved, cx)
                });
                // A saved document is a member from the moment it joins.
                let path = view
                    .read(cx)
                    .session
                    .document
                    .path
                    .as_deref()
                    .map(session_store::identity);
                // Its AI reply comes back with it (ADR 0078).
                let reply = path.as_deref().and_then(|path| {
                    self.case_coordinator
                        .case(&case_id)
                        .and_then(|case| case.reply(path))
                        .map(str::to_owned)
                });
                if let Some(reply) = reply {
                    view.update(cx, |view, cx| view.load_reply(reply, cx));
                }
                if let Some(path) = path
                    && let Some(((), effect)) = self.case_coordinator.edit(&case_id, |case| {
                        (!case.members.contains(&path)).then(|| case.members.push(path))
                    })
                {
                    self.carry_out_cases(effect.into_iter().collect(), cx);
                }
            }
            None if wanted.is_some() => {
                let name = view.read(cx).display_name();
                self.notices.post(format!(
                    "The saved case for {name} could not be found, so it opened without its \
                     replacement mapping."
                ));
                self.tabs.set_case(id, None);
            }
            None => {}
        }
        cx.notify();
    }
    fn tab_view(&self, id: u64) -> Option<Entity<DocumentView>> {
        self.tabs.get(id)?.view().cloned()
    }
    /// Whether this tab can start a case: it has a mapping and none yet.
    pub(super) fn can_create_case(&self, id: u64, cx: &App) -> bool {
        self.tab_view(id).is_some_and(|view| {
            let view = view.read(cx);
            view.case.link().is_none()
                && view.can_copy_markdown()
                && (!view.pii.review().applied().is_empty()
                    || !view.pii.review().variants().is_empty())
        })
    }
    /// A new case whose Categories to detect start from Settings › Detection
    /// (ADR 0076). Existing cases keep their own lists.
    pub(super) fn new_case(
        &self,
        name: String,
        identities: &[mdoc_pii::restore::ExportedIdentity],
        member: Option<PathBuf>,
    ) -> Case {
        let mut case = Case::new(name, identities, member);
        case.skip = self
            .preferences
            .borrow()
            .current
            .as_ref()
            .map(|p| p.categories.skip.clone())
            .unwrap_or_default();
        case
    }
    /// Save this tab's mapping as a new case named after the document.
    pub(super) fn create_case(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let view = self.tab_view(id)?;
        // Already saved, e.g. by an earlier request waiting on the Keychain.
        if view.read(cx).case.link().is_some() {
            return None;
        }
        let (name, identities, dismissed, path) = {
            let v = view.read(cx);
            let name = v.display_name();
            let name = name.strip_suffix(".md").unwrap_or(&name).to_string();
            (
                name,
                v.pii.review().export_identities(),
                v.dismissed_merge_hints(),
                v.session.document.path.clone(),
            )
        };
        if identities.is_empty() {
            self.notices.post(
                "Apply replacements first; a case saves which original each alias stands for.",
            );
            cx.notify();
            return None;
        }
        if self.save_before_case(id, false, window, cx) {
            return None;
        }
        if !self.case_key_ready(
            move |this, window, cx| {
                this.create_case(id, window, cx);
            },
            cx,
        ) {
            return None;
        }
        if let Err(error) = self.case_store() {
            self.key_failed(error);
            cx.notify();
            return None;
        }
        self.resume_locked(window, cx);
        let mut case = self.new_case(
            name,
            &identities,
            path.as_deref().map(session_store::identity),
        );
        // Hints dismissed before the case existed stay dismissed (ADR 0080).
        case.dismissed_hints = dismissed;
        let link = CaseLink::of(&case);
        let case_id = case.id.clone();
        let skip = case.skipped();
        let effect = self.case_coordinator.create(case);
        self.carry_out_cases(effect.into_iter().collect(), cx);
        view.update(cx, |view, cx| {
            // The case's list, seeded from Settings, now decides its scans.
            view.case.set_skip(skip);
            view.case.attach(Some(link.clone()));
            cx.notify();
        });
        self.tabs.set_case(id, Some(link.id));
        self.case_reply_changed(id, cx);
        self.record_review(id, cx);
        self.checkpoint(window, cx);
        cx.notify();
        Some(case_id)
    }
    /// A member tab's mapping changed: fold it into the case and save.
    pub(super) fn case_changed(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(view) = self.tab_view(id) else {
            return;
        };
        // Undo of a join can restore an alias the case has irreversibly
        // retired. Reconcile it before saving, rather than silently dropping
        // the member's new mapping at the tombstone boundary.
        if let Some(case) = view
            .read(cx)
            .case
            .link()
            .and_then(|link| self.case_coordinator.case(&link.id))
        {
            let reserved = case.reserved_aliases();
            if view
                .read(cx)
                .pii
                .review()
                .export_identities()
                .iter()
                .any(|identity| {
                    reserved.contains(&identity.alias)
                        || identity
                            .previous
                            .iter()
                            .any(|alias| reserved.contains(alias))
                })
            {
                let identities = case.exported();
                if let Err(error) = view.update(cx, |view, cx| {
                    view.join_case_identities(&identities, &reserved, cx)
                }) {
                    self.notices
                        .post(format!("Could not preserve the case mapping: {error}"));
                    return;
                }
            }
        }
        let (link, identities, path) = {
            let v = view.read(cx);
            let Some(link) = v.case.link().cloned() else {
                return;
            };
            (
                link,
                v.pii.review().export_identities(),
                v.session.document.path.clone(),
            )
        };
        // Its Keeps and manual marks since the last save, for the case's
        // lists (ADR 0092).
        let listed = view.update(cx, |view, cx| view.take_list_changes(cx));
        let Some(((renames, added, exported, lists), effect)) =
            self.case_coordinator.edit(&link.id, |case| {
                let before = case.clone();
                case.merge(&identities);
                let lists = listed
                    .as_ref()
                    .is_some_and(|changes| case.lists.apply(changes))
                    .then(|| case.lists.clone());
                if let Some(path) = path.as_deref().map(session_store::identity)
                    && !case.members.contains(&path)
                {
                    case.members.push(path);
                }
                if *case == before {
                    return None;
                }
                // Renamed or recategorized identities, by the alias they had.
                let renames = case.renames_since(&before);
                let added = case.identities.len() > before.identities.len();
                Some((renames, added, case.exported(), lists))
            })
        else {
            return;
        };
        view.update(cx, |view, _| view.case.set_names(exported.clone()));
        self.carry_out_cases(effect.into_iter().collect(), cx);
        if let Some(lists) = lists {
            self.case_lists_changed(&link.id, &lists, cx);
        }
        if renames.is_empty() && !added {
            return;
        }
        // Other open members follow, each as an edit in its own history.
        for tab in &self.tabs {
            if tab.id() == id {
                continue;
            }
            if let Some(view) = tab.view()
                && view.read(cx).case.link().is_some_and(|l| l.id == link.id)
            {
                view.update(cx, |view, cx| {
                    view.sync_case_identities(&renames, &exported, cx)
                });
            }
        }
    }
    /// A name the case should replace before any document mentions it.
    pub(super) fn add_case_term(&mut self, id: u64, term: &str, cx: &mut Context<Self>) {
        let Some(view) = self.tab_view(id) else {
            return;
        };
        let Some(link) = view.read(cx).case.link().cloned() else {
            return;
        };
        self.add_case_name(&link.id, term, cx);
    }

    /// A member dismissed a merge hint: the case keeps it and its other open
    /// members hide it too (ADR 0080).
    pub(super) fn dismiss_case_hint(
        &mut self,
        id: u64,
        from: &str,
        into: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(link) = self
            .tab_view(id)
            .and_then(|view| view.read(cx).case.link().cloned())
        else {
            return;
        };
        let pair = (from.to_string(), into.to_string());
        let Some(((), effect)) = self.case_coordinator.edit(&link.id, |case| {
            (!case.dismissed_hints.contains(&pair)).then(|| case.dismissed_hints.push(pair.clone()))
        }) else {
            return;
        };
        self.carry_out_cases(effect.into_iter().collect(), cx);
        for tab in &self.tabs {
            if let Some(view) = tab.view()
                && view.read(cx).case.is(&link.id)
            {
                let pair = std::slice::from_ref(&pair);
                view.update(cx, |view, cx| view.dismiss_case_hints(pair, cx));
            }
        }
    }

    pub(super) fn add_case_name(&mut self, case_id: &str, term: &str, cx: &mut Context<Self>) {
        let term = term.trim();
        let Some((exported, effect)) = self.case_coordinator.edit(case_id, |case| {
            if term.is_empty()
                || case
                    .identities
                    .iter()
                    .any(|i| i.originals.iter().any(|o| o == term))
            {
                return None;
            }
            let category = mdoc_detect::guess_category(term, 0..term.len());
            let alias = case.next_alias(category);
            case.identities.push(mdoc_store::cases::StoredIdentity {
                alias,
                previous: Vec::new(),
                category: category.token().into(),
                custom: false,
                originals: vec![term.to_string()],
            });
            Some(case.exported())
        }) else {
            return;
        };
        self.carry_out_cases(effect.into_iter().collect(), cx);
        for tab in &self.tabs {
            if let Some(view) = tab.view()
                && view.read(cx).case.is(case_id)
            {
                view.update(cx, |view, cx| view.sync_case_identities(&[], &exported, cx));
            }
        }
        cx.notify();
    }
    /// Add a tab to a case (ADR 0041 #10): its names take the case's aliases or
    /// fresh numbers in one undo step, and the case's other names are proposed.
    pub(super) fn join_case(
        &mut self,
        id: u64,
        case_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(view) = self.tab_view(id) else {
            return;
        };
        if view.read(cx).case.link().is_some_and(|l| l.id == case_id) {
            return;
        }
        if !view.read(cx).can_copy_markdown() {
            return;
        }
        if view.read(cx).case.link().is_some() {
            self.remove_from_case(id, window, cx);
        }
        let Some(case) = self.case_coordinator.case(case_id) else {
            return;
        };
        let link = CaseLink::of(case);
        let identities = case.exported();
        let reserved = case.reserved_aliases();
        let skip = case.skipped();
        let lists = case.lists.clone();
        let dismissed = case.dismissed_hints.clone();
        let result = view.update(cx, |view, cx| {
            view.case.set_skip(skip);
            view.case.set_lists(lists);
            view.dismiss_case_hints(&dismissed, cx);
            let result = view.join_case(&link.name, &identities, &reserved, cx);
            if result.is_ok() {
                view.case.attach(Some(link.clone()));
            }
            result
        });
        let summary = match result {
            Ok(summary) => summary,
            Err(error) => {
                self.notices
                    .post(format!("Could not add the document to the case: {error}"));
                cx.notify();
                return;
            }
        };
        self.tabs.set_case(id, Some(link.id.clone()));
        self.case_reply_changed(id, cx);
        // Keep members together in the sidebar.
        let last = self.tabs.iter().rposition(|t| {
            t.id() != id
                && t.view()
                    .and_then(|v| v.read(cx).case.link().map(|l| l.id == link.id))
                    .unwrap_or(false)
        });
        if let Some(last) = last
            && let Some(next) = self.tabs.at(last + 1).map(|t| t.id())
            && next != id
        {
            self.reorder(id, next, cx);
        } else if let Some(last) = last
            && last + 1 == self.tabs.len()
        {
            self.tabs.place(&[id], None);
        }
        self.case_changed(id, cx);
        let mut parts = Vec::new();
        if summary.renumbered > 0 {
            parts.push(format!(
                "{} {} renumbered to fit the case",
                summary.renumbered,
                if summary.renumbered == 1 {
                    "alias"
                } else {
                    "aliases"
                }
            ));
        }
        if summary.proposed > 0 {
            parts.push(format!(
                "{} {} from the case proposed",
                summary.proposed,
                if summary.proposed == 1 {
                    "mention"
                } else {
                    "mentions"
                }
            ));
        }
        if !parts.is_empty() {
            self.notices.post(format!(
                "Added to “{}”: {}. Undo in the document reverts the renumbering.",
                link.name,
                parts.join("; ")
            ));
        }
        self.checkpoint(window, cx);
        cx.notify();
    }
    /// A member saved under a new path no longer lives at `previous`.
    pub(super) fn case_member_moved(
        &mut self,
        id: u64,
        previous: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        let Some(previous) = previous.as_deref().map(session_store::identity) else {
            // An untitled member's reply lived only in its tab until now.
            self.case_reply_changed(id, cx);
            return;
        };
        let Some(view) = self.tab_view(id) else {
            return;
        };
        let (link, current) = {
            let v = view.read(cx);
            (
                v.case.link().cloned(),
                v.session
                    .document
                    .path
                    .as_deref()
                    .map(session_store::identity),
            )
        };
        let Some(link) = link else {
            return;
        };
        if current.as_ref() == Some(&previous) {
            return;
        }
        if let Some(((), effect)) = self.case_coordinator.edit(&link.id, |case| {
            let kept = case.set_reply(&previous, None);
            (case.remove_member(&previous) || kept).then_some(())
        }) {
            self.carry_out_cases(effect.into_iter().collect(), cx);
        }
        // The reply follows the member to its new path.
        self.case_reply_changed(id, cx);
    }
    /// Keep the tab's AI reply in its case under the member's path, or drop
    /// it when cleared (ADR 0078). A member without a path keeps its reply
    /// only in the tab.
    pub(super) fn case_reply_changed(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(view) = self.tab_view(id) else {
            return;
        };
        let (link, path, reply) = {
            let v = view.read(cx);
            (
                v.case.link().cloned(),
                v.session
                    .document
                    .path
                    .as_deref()
                    .map(session_store::identity),
                v.reply.stored(),
            )
        };
        let (Some(link), Some(path)) = (link, path) else {
            return;
        };
        if let Some(((), effect)) = self
            .case_coordinator
            .edit(&link.id, |case| case.set_reply(&path, reply).then_some(()))
        {
            self.carry_out_cases(effect.into_iter().collect(), cx);
        }
    }
    fn pump_cases(&mut self, cx: &mut Context<Self>) {
        let effect = self.case_coordinator.pump();
        self.carry_out_cases(effect.into_iter().collect(), cx);
    }
    pub(super) fn carry_out_cases(
        &mut self,
        effects: Vec<CaseEffect<gpui::AnyWindowHandle>>,
        cx: &mut Context<Self>,
    ) {
        for effect in effects {
            match effect {
                CaseEffect::Write(case) => {
                    let store = match self.case_store() {
                        Ok(store) => store,
                        Err(error) => {
                            let effects = self
                                .case_coordinator
                                .written(Err(error.to_string()), self.quit_flow.finishing());
                            self.carry_out_cases(effects.unwrap_or_default(), cx);
                            continue;
                        }
                    };
                    let task = cx
                        .background_executor()
                        .spawn(async move { store.save(&case) });
                    cx.spawn(async move |this, cx| {
                        let result = task.await;
                        let _ = this.update(cx, |this, cx| {
                            let finishing = this.quit_flow.finishing();
                            let Some(effects) = this
                                .case_coordinator
                                .written(result.map_err(|error| error.to_string()), finishing)
                            else {
                                return;
                            };
                            this.carry_out_cases(effects, cx);
                            cx.notify();
                        });
                    })
                    .detach();
                }
                CaseEffect::CloseTab(tab) => self.close_after_save(tab, cx),
                CaseEffect::WriteFailed(error) => self
                    .notices
                    .post(format!("Could not save the case: {error}")),
                CaseEffect::ResumeQuit(window) => self.resume_case_quit(window, cx),
            }
        }
    }
    fn resume_case_quit(&self, window: gpui::AnyWindowHandle, cx: &mut Context<Self>) {
        let entity = cx.entity().downgrade();
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, window, cx| {
                let _ = entity.update(cx, |this, cx| {
                    if this.quit_flow.finishing() && this.case_coordinator.quit_window().is_some() {
                        this.finish_quit(window, cx);
                    }
                });
            });
        });
    }
    fn close_after_save(&mut self, tab: u64, cx: &mut Context<Self>) {
        let Some(window) = cx.windows().into_iter().next() else {
            return;
        };
        let entity = cx.entity().downgrade();
        // Runs after this update, once the workspace is free again.
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, window, cx| {
                let _ = entity.update(cx, |this, cx| this.close_tab(tab, window, cx));
            });
        });
    }
    #[cfg(test)]
    pub(super) fn set_case_quit_window(&mut self, window: Option<gpui::AnyWindowHandle>) {
        self.case_coordinator.set_quit_window(window);
    }
    #[cfg(test)]
    pub(super) fn case_quit_pending(&self) -> bool {
        self.case_coordinator.quit_pending()
    }
    pub(super) fn cancel_case_quit(&mut self) {
        self.case_coordinator.cancel_quit();
    }
    /// Final saves use the normal async queue. Re-read members on completion
    /// too: edits made while a write ran must reach their latest revision.
    pub(super) fn flush_cases(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        match self.case_coordinator.flush(window.window_handle()) {
            Flush::Prompting => return false,
            Flush::Collect => {}
            Flush::Ask(error) => {
                // A hidden window (ADR 0084) shows before it asks.
                crate::menu_bar::reveal_if_hidden(window, cx);
                let answer = window.prompt(
                PromptLevel::Warning,
                "Could not save replacement mappings",
                Some(&format!("{error}\nYour mappings are still available in this window. Retry saving, cancel quitting, or quit without the latest mappings.")),
                &["Retry", "Cancel", "Quit without saving mappings"], cx,
            );
                cx.spawn_in(window, async move |this, cx| {
                    let answer = answer.await.ok();
                    let _ = this.update_in(cx, |this, window, cx| {
                        this.case_coordinator.answered();
                        match answer {
                            Some(0) => {
                                this.pump_cases(cx);
                                this.finish_quit(window, cx);
                            }
                            Some(2) => {
                                this.case_coordinator.give_up();
                                this.close_window(window, cx);
                            }
                            _ => {
                                this.cancel_case_quit();
                                this.quit_flow.abandon();
                                Self::quit_cancelled(cx);
                                cx.notify();
                            }
                        }
                    });
                })
                .detach();
                return false;
            }
        }
        let ids: Vec<_> = self.tabs.iter().map(|t| t.id()).collect();
        for id in ids {
            self.case_changed(id, cx);
        }
        self.pump_cases(cx);
        self.case_coordinator.flushed()
    }
    /// Leave the case: the tab keeps its live mapping, the case keeps its
    /// identities (ADR 0041 #25).
    pub(super) fn remove_from_case(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(view) = self.tab_view(id) else {
            return;
        };
        let Some(link) = view.update(cx, |view, cx| view.leave_case(cx)) else {
            return;
        };
        let path = view
            .read(cx)
            .session
            .document
            .path
            .as_deref()
            .map(session_store::identity);
        self.tabs.set_case(id, None);
        if let Some(path) = path
            && let Some(((), effect)) = self.case_coordinator.edit(&link.id, |case| {
                // The reply stays with the tab, not the case it left.
                let kept = case.set_reply(&path, None);
                (case.remove_member(&path) || kept).then_some(())
            })
        {
            self.carry_out_cases(effect.into_iter().collect(), cx);
        }
        self.checkpoint(window, cx);
        cx.notify();
    }
    /// Confirm, then wipe the case. Member tabs keep their live mappings.
    pub(super) fn delete_case(
        &mut self,
        case_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(name) = self.case_coordinator.case(&case_id).map(|c| c.name.clone()) else {
            return;
        };
        let answer = window.prompt(
            PromptLevel::Warning,
            &format!("Delete the case “{name}” and forget its originals?"),
            Some(
                "Its saved replacement mapping is wiped from this computer. Documents keep \
                 their text. Open documents keep their live mappings until closed; without \
                 this saved mapping, closed documents and AI replies cannot be restored. \
                 This cannot be undone.",
            ),
            &["Delete Case", "Cancel"],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            if answer.await.ok() != Some(0) {
                return;
            }
            let _ = this.update_in(cx, |this, window, cx| {
                this.forget_case(&case_id, window, cx)
            });
        })
        .detach();
    }
    pub(super) fn forget_case(
        &mut self,
        case_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .cases
            .page
            .as_ref()
            .is_some_and(|page| page.id() == case_id)
        {
            // Home takes the deleted case page's stop (ADR 0081).
            self.show_home_as(super::navigation::Arrival::Replace, window, cx);
        }
        self.navigation
            .forget(&super::navigation::Stop::Case(case_id.into()));
        self.case_coordinator.delete(case_id);
        self.tabs.forget_case(case_id);
        for tab in &self.tabs {
            if let Some(view) = tab.view() {
                view.update(cx, |view, cx| view.case_deleted(case_id, cx));
            }
        }
        if let Ok(store) = self.case_store() {
            let id = case_id.to_string();
            let task = cx
                .background_executor()
                .spawn(async move { store.delete(&id) });
            cx.spawn(async move |this, cx| {
                if let Err(error) = task.await {
                    let _ = this.update(cx, |this, cx| {
                        this.notices
                            .post(format!("Could not delete the case file: {error}"));
                        cx.notify();
                    });
                }
            })
            .detach();
        }
        self.checkpoint(window, cx);
        cx.notify();
    }
    pub(super) fn start_rename_case(
        &mut self,
        case_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(name) = self.case_coordinator.case(&case_id).map(|c| c.name.clone()) else {
            return;
        };
        let input = cx.new(|cx| {
            let mut input = markdown_search::SearchInput::new(cx)
                .with_theme(self.theme.clone())
                .with_key_context("CaseName")
                .with_placeholder("Case name");
            input.set_value(name, cx);
            input
        });
        let focus = input.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        // Leaving the field saves, like Enter (Escape still cancels).
        self.cases.rename_blur = Some(cx.on_focus_out(&focus, window, |this, _, window, cx| {
            this.finish_rename(true, window, cx)
        }));
        self.cases.renaming = Some((case_id, input));
        cx.notify();
    }
    pub(super) fn finish_rename(
        &mut self,
        save: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(input) = self.end_rename(save, cx) else {
            return;
        };
        // A click on another control has already moved focus there; keep it.
        if input.read(cx).focus_handle(cx).contains_focused(window, cx) {
            self.focus_active(window, cx);
        }
        cx.notify();
    }
    /// End a rename in progress, saving a non-empty name when `save`. Leaving
    /// the case page for a tab, Home or another case saves, as a click away does.
    pub(super) fn end_rename(
        &mut self,
        save: bool,
        cx: &mut Context<Self>,
    ) -> Option<Entity<markdown_search::SearchInput>> {
        let (case_id, input) = self.cases.renaming.take()?;
        self.cases.rename_blur = None;
        let name = input.read(cx).value().trim().to_string();
        if save && !name.is_empty() {
            if let Some(((), effect)) = self.case_coordinator.edit(&case_id, |case| {
                case.name = name.clone();
                Some(())
            }) {
                self.carry_out_cases(effect.into_iter().collect(), cx);
            }
            for view in self.tabs.iter().filter_map(|t| t.view().cloned()) {
                view.update(cx, |view, cx| {
                    let effect = view.case.case_renamed(&case_id, &name);
                    view.carry_out_case_link(effect, cx);
                });
            }
        }
        Some(input)
    }
    /// Show the cached case page. This path never opens a key or a member file.
    pub(super) fn open_case(&mut self, case_id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.show_case_page(case_id, super::case_page::Place::Header, window, cx);
    }
    pub(super) fn show_case_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_case_list_at(None, window, cx);
    }
    /// Open the list below `anchor`, the link it was opened from (#307).
    pub(super) fn show_case_list_at(
        &mut self,
        anchor: Option<gpui::Bounds<gpui::Pixels>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // An existing modal keeps focus. Browsing itself needs no key, but
        // waits for that modal to finish before it is shown.
        if self.cases.explainer {
            let earlier = self.cases.then.take();
            self.cases.then = Some(Box::new(move |this, window, cx| {
                if let Some(earlier) = earlier {
                    earlier(this, window, cx);
                }
                this.show_case_list_at(anchor, window, cx);
            }));
            return;
        }
        self.cases.list_open = true;
        self.cases.list_anchor = anchor;
        self.case_list_return = window.focused(cx).filter(|f| *f != self.case_list_focus);
        // After the palette, if it ran this, has restored its own focus.
        let focus = self.case_list_focus.clone();
        window.defer(cx, move |window, cx| window.focus(&focus, cx));
        cx.notify();
    }

    /// Close the Open Case list, focus back where it was opened from.
    pub(super) fn close_case_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cases.list_open = false;
        match self.case_list_return.take() {
            Some(focus) => window.focus(&focus, cx),
            None => self.focus_active(window, cx),
        }
        cx.notify();
    }

    /// A case's sidebar header above its first member tab.
    pub(super) fn case_header(
        &self,
        link: &CaseLink,
        _first: u64,
        count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let id = link.id.clone();
        div()
            .id(gpui::SharedString::from(format!("case-{}", link.id)))
            .when(cfg!(test), |v| v.debug_selector(|| "case-header".into()))
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .pt_2()
            .pb_1()
            .text_size(px(11.))
            .text_color(palette.header_muted)
            .when(
                self.cases
                    .page
                    .as_ref()
                    .is_some_and(|page| page.id() == link.id),
                |v| v.bg(theme.sidebar_selected()),
            )
            .drag_over::<TabDrag>(move |v, _, _, _| v.bg(theme.sidebar_selected()))
            .on_drop(cx.listener({
                let id = link.id.clone();
                move |this, drag: &TabDrag, window, cx| this.join_case(drag.id, &id, window, cx)
            }))
            // A folder dropped here offers Add to this case (ADR 0075).
            .drag_over::<gpui::ExternalPaths>(move |v, _, _, _| v.bg(theme.sidebar_selected()))
            .can_drop(super::can_drop_files)
            .on_drop(cx.listener({
                let id = link.id.clone();
                move |this, paths: &gpui::ExternalPaths, window, cx| {
                    this.drop_on_case(paths, &id, window, cx)
                }
            }))
            .child(
                ui::control(
                    gpui::SharedString::from(format!("collapse-case-{}", link.id)),
                    if self.cases.collapsed.contains(&link.id) {
                        "▸"
                    } else {
                        "▾"
                    },
                    theme,
                    true,
                )
                .when(cfg!(test), |v| v.debug_selector(|| "collapse-case".into()))
                .aria_label(if self.cases.collapsed.contains(&link.id) {
                    "Expand case"
                } else {
                    "Collapse case"
                })
                .px_1()
                .py_0()
                .on_click(cx.listener({
                    let id = link.id.clone();
                    move |this, _, window, cx| {
                        cx.stop_propagation();
                        if !this.cases.collapsed.remove(&id) {
                            this.cases.collapsed.insert(id.clone());
                        }
                        this.checkpoint(window, cx);
                        cx.notify();
                    }
                })),
            )
            .child(
                div()
                    .size(px(8.))
                    .flex_shrink_0()
                    .rounded_full()
                    .bg(style::case_color(link.hue(), theme)),
            )
            .child(
                ui::control(
                    gpui::SharedString::from(format!("case-page-link-{}", link.id)),
                    link.name.clone(),
                    theme,
                    true,
                )
                .when(cfg!(test), |v| v.debug_selector(|| "case-page-link".into()))
                .flex_1()
                .min_w_0()
                .truncate()
                .px_1()
                .py_1()
                .aria_label(format!("Open case {}", link.name))
                .aria_selected(
                    self.cases
                        .page
                        .as_ref()
                        .is_some_and(|page| page.id() == link.id),
                )
                .on_click(cx.listener({
                    let id = id.clone();
                    move |this, _, window, cx| this.open_case(&id, window, cx)
                })),
            )
            .child(
                ui::control(
                    gpui::SharedString::from(format!("case-menu-{}", link.id)),
                    "⋯",
                    theme,
                    true,
                )
                .when(cfg!(test), |v| {
                    v.debug_selector(|| "case-header-menu".into())
                })
                .aria_label(format!("Actions for case {}", link.name))
                .px_1()
                .py_0()
                .on_click(cx.listener({
                    let id = id.clone();
                    move |this, event: &gpui::ClickEvent, window, cx| {
                        this.menu_return = window.focused(cx);
                        this.cases.header_menu = Some((id.clone(), event.position()));
                        window.focus(&this.menu_focus, cx);
                        cx.notify();
                    }
                })),
            )
            .when(count > 1, |v| v.child(div().child(count.to_string())))
            .when(!self.context_menu_open(), |v| {
                v.tooltip(style::tooltip(
                    format!("Case “{}” · its replacement mapping is saved", link.name),
                    theme,
                ))
            })
            .on_mouse_down(
                gpui::MouseButton::Right,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    this.menu_return = window.focused(cx);
                    window.focus(&this.menu_focus, cx);
                    this.cases.header_menu = Some((id.clone(), event.position));
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    /// Switch a category on or off for a case and its open members.
    pub(super) fn toggle_case_category(
        &mut self,
        case_id: &str,
        category: mdoc_pii::Category,
        cx: &mut Context<Self>,
    ) {
        let Some((skip, effect)) = self.case_coordinator.edit(case_id, |case| {
            let token = category.token().to_string();
            if case.skip.contains(&token) {
                case.skip.retain(|s| *s != token);
            } else {
                case.skip.push(token);
            }
            Some(case.skipped())
        }) else {
            return;
        };
        self.carry_out_cases(effect.into_iter().collect(), cx);
        for view in self.tabs.iter().filter_map(|t| t.view().cloned()) {
            view.update(cx, |view, cx| {
                let effect = view.case.case_skip_changed(case_id, &skip);
                view.carry_out_case_link(effect, cx);
            });
        }
        cx.notify();
    }
    /// Case `case_id`'s lists changed: its open members scan with them.
    pub(super) fn case_lists_changed(
        &mut self,
        case_id: &str,
        lists: &mdoc_store::lists::Lists,
        cx: &mut Context<Self>,
    ) {
        for view in self.tabs.iter().filter_map(|t| t.view().cloned()) {
            view.update(cx, |view, _| view.case.case_lists_changed(case_id, lists));
        }
        cx.notify();
    }
    /// The case page's Remove on an Always (`always`) or Never entry.
    pub(super) fn remove_case_list_entry(
        &mut self,
        case_id: &str,
        always: bool,
        wording: &str,
        cx: &mut Context<Self>,
    ) {
        let Some((lists, effect)) = self.case_coordinator.edit(case_id, |case| {
            let removed = if always {
                case.lists.remove_always(wording)
            } else {
                case.lists.remove_never(wording)
            };
            removed.then(|| case.lists.clone())
        }) else {
            return;
        };
        self.carry_out_cases(effect.into_iter().collect(), cx);
        self.case_lists_changed(case_id, &lists, cx);
    }
    /// Case entries for the tab context menu.
    pub(super) fn case_menu_items(&self, tab: u64, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let row = |id: &'static str, label: &'static str| {
            crate::ui::focus_ring(id, theme)
                .role(gpui::Role::Button)
                .aria_label(label)
                .key_context("UiControl")
                .tab_index(0)
                .when(cfg!(test), move |v| v.debug_selector(move || id.into()))
                .px_3()
                .py_1()
                .rounded_sm()
                .cursor_pointer()
                .hover(move |v| v.bg(palette.placeholder_bg))
                .child(label)
        };
        let link = self
            .tab_view(tab)
            .and_then(|v| v.read(cx).case.link().cloned());
        let movable = self
            .tab_view(tab)
            .is_some_and(|v| v.read(cx).can_copy_markdown());
        let moves: Vec<AnyElement> = if movable {
            self.case_coordinator
                .cases()
                .iter()
                .filter(|c| link.as_ref().is_none_or(|l| l.id != c.id))
                .take(5)
                .map(|c| {
                    let case_id = c.id.clone();
                    let label = format!("Move to case “{}”", c.name);
                    crate::ui::focus_ring(
                        gpui::SharedString::from(format!("context-move-{}", c.id)),
                        theme,
                    )
                    .role(gpui::Role::Button)
                    .aria_label(label.clone())
                    .key_context("UiControl")
                    .tab_index(0)
                    .when(cfg!(test), |v| {
                        v.debug_selector(|| "context-move-case".into())
                    })
                    .px_3()
                    .py_1()
                    .rounded_sm()
                    .cursor_pointer()
                    .hover(move |v| v.bg(palette.placeholder_bg))
                    .max_w(px(320.))
                    .truncate()
                    .child(label)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.tab_menu = None;
                        this.join_case(tab, &case_id, window, cx);
                    }))
                    .into_any_element()
                })
                .collect()
        } else {
            Vec::new()
        };
        let mut items = match link {
            Some(link) => {
                let rename = link.id.clone();
                let delete = link.id.clone();
                let categories = link.id.clone();
                vec![
                    row("context-case-categories", "Categories to detect…")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.tab_menu = None;
                            this.show_case_page(
                                &categories,
                                super::case_page::Place::Categories,
                                window,
                                cx,
                            );
                        }))
                        .into_any_element(),
                    row("context-rename-case", "Rename case…")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.tab_menu = None;
                            this.show_case_page(
                                &rename,
                                super::case_page::Place::Rename,
                                window,
                                cx,
                            );
                        }))
                        .into_any_element(),
                    row("context-leave-case", "Remove from case")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.tab_menu = None;
                            this.remove_from_case(tab, window, cx);
                        }))
                        .into_any_element(),
                    row("context-delete-case", "Delete case and forget originals…")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.tab_menu = None;
                            this.show_case_page(
                                &delete,
                                super::case_page::Place::Delete,
                                window,
                                cx,
                            );
                        }))
                        .into_any_element(),
                ]
            }
            None if self.can_create_case(tab, cx) => vec![
                row("context-create-case", "Save as case")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.tab_menu = None;
                        this.create_case(tab, window, cx);
                    }))
                    .into_any_element(),
            ],
            None => Vec::new(),
        };
        items.extend(moves);
        items
    }

    /// File → Open Case…: every saved case, newest first.
    pub(super) fn case_list(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.cases.list_open || self.cases.explainer {
            return None;
        }
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let open: HashSet<String> = self
            .tabs
            .iter()
            .filter_map(|t| t.view()?.read(cx).case.link().map(|l| l.id.clone()))
            .collect();
        let rows: Vec<AnyElement> = self
            .case_coordinator
            .summaries()
            .iter()
            .map(|case| {
                let link = case.link();
                let open_id = case.id.clone();
                let delete_id = case.id.clone();
                let documents = case.documents;
                div()
                    .id(gpui::SharedString::from(format!("case-row-{}", case.id)))
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .hover(move |v| v.bg(palette.placeholder_bg))
                    .child(
                        div()
                            .size(px(8.))
                            .flex_shrink_0()
                            .rounded_full()
                            .bg(style::case_color(link.hue(), theme)),
                    )
                    .child(
                        crate::ui::focus_ring(
                            gpui::SharedString::from(format!("open-case-{}", case.id)),
                            theme,
                        )
                        .when(cfg!(test), |v| v.debug_selector(|| "open-case-row".into()))
                        // Enter or Space opens it, as a click does (ADR 0062).
                        .key_context("UiControl")
                        .tab_index(0)
                        .tab_stop(true)
                        .role(gpui::Role::Button)
                        .aria_label(format!("Open {}", case.name))
                        .rounded_sm()
                        .flex_1()
                        .min_w_0()
                        .cursor_pointer()
                        .flex()
                        .flex_col()
                        .child(div().truncate().child(case.name.clone()))
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(palette.header_muted)
                                .child(format!(
                                    "{}{documents} {}{}",
                                    self.case_coordinator.case(&case.id).map_or_else(
                                        String::new,
                                        |case| {
                                            let count = case.exported().len();
                                            format!(
                                                "{count} {} · ",
                                                if count == 1 { "name" } else { "names" }
                                            )
                                        }
                                    ),
                                    if documents == 1 {
                                        "document"
                                    } else {
                                        "documents"
                                    },
                                    if open.contains(&case.id) {
                                        " · open"
                                    } else {
                                        ""
                                    }
                                )),
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| this.open_case(&open_id, window, cx),
                        )),
                    )
                    .child(
                        ui::control(
                            gpui::SharedString::from(format!("delete-case-{}", case.id)),
                            "Delete…",
                            theme,
                            true,
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.close_case_list(window, cx);
                                this.show_case_page(
                                    &delete_id,
                                    super::case_page::Place::Delete,
                                    window,
                                    cx,
                                )
                            },
                        )),
                    )
                    .into_any_element()
            })
            .collect();
        let locked = self.case_list_locked_line();
        let panel = ui::panel("case-list", theme)
            .when(cfg!(test), |v| v.debug_selector(|| "case-list".into()))
            .track_focus(&self.case_list_focus)
            .key_context("UiPanel UiMenu")
            .on_action(cx.listener(|this, _: &ui::NextControl, window, cx| {
                ui::cycle(window, cx, Some(&this.case_list_focus), false);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &ui::PreviousControl, window, cx| {
                ui::cycle(window, cx, Some(&this.case_list_focus), true);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &ui::CloseMenu, window, cx| {
                this.close_case_list(window, cx);
                cx.stop_propagation();
            }))
            .w(px(420.))
            .max_h(px(480.))
            .overflow_y_scroll()
            .p_2()
            .flex()
            .flex_col()
            .gap_1()
            .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                this.cases.list_open = false;
                // Unless the click focused something, focus
                // goes back where the list was opened from.
                let list = this.case_list_focus.clone();
                let back = this.case_list_return.take();
                let weak = cx.entity().downgrade();
                window.defer(cx, move |window, cx| {
                    if list.contains_focused(window, cx) {
                        match back {
                            Some(focus) => window.focus(&focus, cx),
                            None => {
                                let _ = weak.update(cx, |this, cx| this.focus_active(window, cx));
                            }
                        }
                    }
                });
                cx.notify();
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .px_2()
                    .pb_1()
                    .child(div().flex_1().child("Open case"))
                    .child(
                        ui::control("close-case-list", "×", theme, true)
                            .aria_label("Close case list")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.close_case_list(window, cx);
                            })),
                    ),
            )
            // Locked cases are never "no saved cases" (#307).
            .when(rows.is_empty() && locked.is_none(), |v| {
                v.child(
                    div()
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "case-list-empty".into())
                        })
                        .px_2()
                        .py_2()
                        .text_size(px(12.))
                        .text_color(palette.header_muted)
                        .child(
                            "No saved cases yet. Right-click a tab with applied \
                             replacements, or open Share, and choose Save as \
                             case.",
                        ),
                )
            })
            .children(rows)
            .children(locked.map(|line| {
                div()
                    .when(cfg!(test), |v| {
                        v.debug_selector(|| "case-list-locked".into())
                    })
                    .px_2()
                    .py_2()
                    .text_size(px(12.))
                    .text_color(palette.header_muted)
                    .child(line)
            }))
            .when(!self.case_coordinator.loaded(), |v| {
                v.child(
                    ui::control("case-list-unlock", "Unlock saved cases…", theme, true)
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "case-list-unlock".into())
                        })
                        .on_click(cx.listener(|this, _, window, cx| this.unlock_cases(window, cx))),
                )
            });
        // Below Home's link when opened from it; centered otherwise.
        let placed = match self.cases.list_anchor {
            Some(link) => gpui::anchored()
                .position(gpui::point(link.right(), link.bottom() + px(4.)))
                .anchor(gpui::Anchor::TopRight)
                .snap_to_window_with_margin(px(8.))
                .child(panel)
                .into_any_element(),
            None => div()
                .absolute()
                .inset_0()
                .flex()
                .items_start()
                .justify_center()
                .pt(px(80.))
                .child(panel)
                .into_any_element(),
        };
        Some(gpui::deferred(placed).with_priority(2).into_any_element())
    }

    /// Save the tab as a case and close it once the case is safely written;
    /// after the Keychain explainer when that comes first (ADR 0043).
    pub(super) fn save_case_and_close(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.case_coordinator.forget_asked(id);
        if self.save_before_case(id, true, window, cx) {
            return;
        }
        if !self.case_key_ready(
            move |this, window, cx| this.save_case_and_close(id, window, cx),
            cx,
        ) {
            return;
        }
        // The case already exists, e.g. its save failed: retry it (#189).
        let case = self
            .tab_view(id)
            .and_then(|view| view.read(cx).case.link().map(|link| link.id.clone()));
        if let Some(case) = case {
            let effects = self.case_coordinator.save_and_close(case, id);
            self.carry_out_cases(effects, cx);
            return;
        }
        if let Some(case) = self.create_case(id, window, cx) {
            self.case_coordinator.note_asked(id);
            self.case_coordinator.close_after(case, id);
        }
    }

    /// A case lists its documents by path, so a never-saved document (a fresh
    /// conversion) is saved first; the case follows once it is (#75).
    /// Returns true when a save started instead.
    fn save_before_case(
        &mut self,
        id: u64,
        close: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(view) = self.tab_view(id) else {
            return false;
        };
        if view.read(cx).session.document.path.is_some() {
            return false;
        }
        // Answering a close or quit prompt is not a stop (ADR 0081).
        let arrival = if close {
            super::navigation::Arrival::Unrecorded
        } else {
            super::navigation::Arrival::Visit
        };
        self.activate_as(id, arrival, window, cx);
        view.update(cx, |view, cx| {
            view.save(true, Some(Next::CreateCase { close }), window, cx)
        });
        true
    }

    /// Whether closing tab `id` would lose an unsaved mapping, and Settings
    /// asks first: applied aliases and no case, or, closing one tab, a case
    /// whose save failed; a quit retries those in its final case saves
    /// (ADR 0071's at-risk tab, without the unsaved changes).
    pub(super) fn mapping_at_risk(&self, id: u64, quitting: bool, cx: &App) -> bool {
        let ask = self
            .preferences
            .borrow()
            .snapshot()
            .map(|p| p.cases.ask_before_closing)
            .unwrap_or(true);
        let Some(view) = self.tab_view(id) else {
            return false;
        };
        let v = view.read(cx);
        ask && !self.case_coordinator.asked(id)
            && cfg!(any(target_os = "macos", windows, target_os = "linux"))
            && !v.pii.review().applied().is_empty()
            && v.case
                .link()
                .is_none_or(|l| !quitting && self.case_coordinator.failed(&l.id))
    }

    /// Before closing a tab whose mapping would be lost, offer to keep it:
    /// with its unsaved changes in one prompt (ADR 0071). Returns true when
    /// the close continues now.
    pub(super) fn confirm_mapping_close(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        !self.ask_about_mapping(id, false, window, cx)
    }

    /// While quitting, ask about tab `id`'s mapping, with its unsaved changes
    /// when it has them. Returns false when its mapping isn't at risk.
    pub(super) fn ask_quit_mapping(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.ask_about_mapping(id, true, window, cx)
    }

    /// Returns true when a prompt took over the close or quit step.
    fn ask_about_mapping(
        &mut self,
        id: u64,
        quitting: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.mapping_at_risk(id, quitting, cx) {
            return false;
        }
        let Some(view) = self.tab_view(id) else {
            return false;
        };
        let (dirty, prompting) = {
            let v = view.read(cx);
            (v.dirty(cx), v.lifecycle.prompting())
        };
        if prompting {
            return true;
        }
        self.case_coordinator.note_asked(id);
        if quitting {
            self.cases.quit_asked.push(id);
        }
        self.cases.mapping_prompt = true;
        // The prompt's tab comes to the front; it isn't a stop (ADR 0081).
        self.activate_as(id, super::navigation::Arrival::Unrecorded, window, cx);
        if dirty {
            self.ask_changes_and_mapping(id, quitting, window, cx);
        } else {
            self.ask_mapping(id, quitting, window, cx);
        }
        true
    }

    pub(super) fn ask_mapping(
        &mut self,
        id: u64,
        quitting: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let answer = window.prompt(
            PromptLevel::Info,
            "Keep the replacement mapping?",
            Some(
                "Closing forgets which original each alias stands for, so AI replies to this \
                 document can't be restored. Save it as a case to keep it, encrypted on this \
                 computer. You can turn this question off in Settings.",
            ),
            &["Save as Case", "Close", "Cancel"],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let answer = answer.await.ok();
            let _ = this.update_in(cx, |this, window, cx| {
                this.cases.mapping_prompt = false;
                match answer {
                    Some(0) if quitting => this.quit_save_case(id, window, cx),
                    Some(0) => this.save_case_and_close(id, window, cx),
                    Some(1) if quitting => this.event(id, &CLOSE_RESOLVED, window, cx),
                    Some(1) => this.close_tab(id, window, cx),
                    _ => this.cancel_mapping_close(id, quitting, window, cx),
                }
            });
        })
        .detach();
    }

    /// Unsaved changes and an unsaved mapping: one question for both.
    pub(super) fn ask_changes_and_mapping(
        &mut self,
        id: u64,
        quitting: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(view) = self.tab_view(id) else {
            return;
        };
        let question = format!(
            "Save “{}” and keep its mapping?",
            view.read(cx).display_name()
        );
        let answer = window.prompt(
            PromptLevel::Warning,
            &question,
            Some(
                "Closing forgets which original each alias stands for, so AI replies to this \
                 document can't be restored. Save as Case keeps it, encrypted on this \
                 computer; Save keeps only the document. You can turn this question off in \
                 Settings.",
            ),
            &[
                gpui::PromptButton::new("Save as Case"),
                gpui::PromptButton::new("Save"),
                gpui::PromptButton::new("Don't Save"),
                gpui::PromptButton::cancel("Cancel"),
            ],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let answer = answer.await.ok();
            let _ = this.update_in(cx, |this, window, cx| {
                this.cases.mapping_prompt = false;
                let Some(view) = this.tab_view(id) else {
                    return;
                };
                let unsaved = view.read(cx).session.document.path.is_none();
                match answer {
                    Some(0) if quitting => this.quit_save_case(id, window, cx),
                    Some(0) => {
                        // The document is saved first; its case follows and
                        // closes the tab once written.
                        this.case_coordinator.forget_asked(id);
                        view.update(cx, |view, cx| {
                            view.save(unsaved, Some(Next::CreateCase { close: true }), window, cx)
                        });
                    }
                    Some(answer @ (1 | 2)) => {
                        if !quitting {
                            this.quit_flow.close(id);
                        }
                        view.update(cx, |view, cx| {
                            if answer == 1 {
                                view.save(unsaved, Some(Next::Close), window, cx)
                            } else {
                                view.proceed(Next::Close, window, cx)
                            }
                        });
                    }
                    _ => this.cancel_mapping_close(id, quitting, window, cx),
                }
            });
        })
        .detach();
    }

    fn cancel_mapping_close(
        &mut self,
        id: u64,
        quitting: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cases.closing_members.cancel();
        self.case_coordinator.forget_asked(id);
        if quitting {
            self.event(id, &CLOSE_CANCELLED, window, cx);
        }
    }

    /// Save as Case while quitting: the document first when it needs it,
    /// then the case; the quit goes on once the case is queued.
    fn quit_save_case(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(view) = self.tab_view(id) else {
            return;
        };
        let (dirty, unsaved) = {
            let v = view.read(cx);
            (v.dirty(cx), v.session.document.path.is_none())
        };
        if dirty || unsaved {
            self.cases.quit_case = Some(id);
            view.update(cx, |view, cx| {
                view.save(unsaved, Some(Next::CreateCase { close: false }), window, cx)
            });
            return;
        }
        self.quit_create_case(id, window, cx);
    }

    /// Whether a Create event from tab `id` finishes a Save as Case answered
    /// while quitting; if so, it is carried out.
    pub(super) fn quit_case_saved(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.cases.quit_case != Some(id) {
            return false;
        }
        self.cases.quit_case = None;
        self.quit_create_case(id, window, cx);
        true
    }

    fn quit_create_case(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        // The Keychain explainer comes first; the quit waits for it and goes
        // on once Continue opens the key. Not now cancels the quit.
        // A quit cancelled meanwhile (a declined dialog keeps this waiting
        // for Try again) only gets its case: it must not resolve another quit.
        if !self.case_key_ready(
            move |this, window, cx| {
                if this.cases.quit_key_wait == Some(id) {
                    this.quit_create_case(id, window, cx);
                } else {
                    this.create_case(id, window, cx);
                }
            },
            cx,
        ) {
            self.cases.quit_key_wait = Some(id);
            return;
        }
        self.cases.quit_key_wait = None;
        let event = if self.create_case(id, window, cx).is_some() {
            CLOSE_RESOLVED
        } else {
            CLOSE_CANCELLED
        };
        self.event(id, &event, window, cx);
    }

    /// A close or quit was cancelled: the mappings it asked about are asked
    /// about again next time.
    pub(super) fn mapping_close_cancelled(&mut self, id: u64) {
        self.cases.quit_case = None;
        self.cases.quit_key_wait = None;
        self.case_coordinator.forget_asked(id);
        for asked in std::mem::take(&mut self.cases.quit_asked) {
            self.case_coordinator.forget_asked(asked);
        }
    }

    /// A question about a mapping is up: a quit waits until it's answered.
    pub(super) fn mapping_prompt_open(&self) -> bool {
        self.cases.mapping_prompt
    }

    /// The Keychain explainer was declined while a quit waited on it.
    fn stop_quit_key_wait(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.cases.quit_key_wait.take() {
            self.event(id, &CLOSE_CANCELLED, window, cx);
        }
    }

    /// A quit starts: none of its mappings is asked about yet.
    pub(super) fn mapping_quit_started(&mut self) {
        for asked in std::mem::take(&mut self.cases.quit_asked) {
            self.case_coordinator.forget_asked(asked);
        }
    }
}

const CLOSE_RESOLVED: TabEvent = TabEvent::Close(CloseEvent::Resolved);
const CLOSE_CANCELLED: TabEvent = TabEvent::Close(CloseEvent::Cancelled);
