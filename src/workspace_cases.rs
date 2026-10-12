//! Saved cases in the window (ADR 0041, phase 2): create a case from a tab's
//! mapping, save changes in the background, give reopened members their
//! mapping back, rename, remove, delete, and the Open Case list.
use super::*;
use crate::cases::{Case, CaseLink, Store};
use std::collections::{HashMap, HashSet};

gpui::actions!(cases, [ConfirmCaseName, CancelCaseName]);

pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("enter", ConfirmCaseName, Some("CaseName")),
        KeyBinding::new("escape", CancelCaseName, Some("CaseName")),
    ]);
}

#[derive(Default)]
pub(super) struct CaseState {
    store: Option<Arc<Store>>,
    /// Every readable case, once loaded.
    pub(super) cases: Vec<Case>,
    loaded: bool,
    loading: bool,
    /// Latest unsaved state per case; one write runs at a time.
    pending: HashMap<String, Case>,
    /// The case being written now.
    writing: Option<Case>,
    /// Cases whose last write failed; kept pending for the next attempt.
    failed: HashSet<String>,
    /// Tabs to close once their new case is safely written.
    close_after: HashMap<String, u64>,
    pub(super) renaming: Option<(String, Entity<markdown_search::SearchInput>)>,
    pub(super) list_open: bool,
    /// Tabs whose close already asked about their unsaved mapping.
    pub(super) asked: HashSet<u64>,
}

impl Workspace {
    /// The store, opening the OS-held key on first use.
    pub(super) fn case_store(&mut self) -> Result<Arc<Store>, String> {
        if let Some(store) = &self.cases.store {
            return Ok(store.clone());
        }
        // Tests inject a store; they never reach the real key or app data.
        if cfg!(test) {
            return Err("Saved cases are not set up in this test.".into());
        }
        let dir = Store::default_dir().ok_or("No folder for app data on this system.")?;
        let store = Arc::new(Store::open(dir)?);
        self.cases.store = Some(store.clone());
        Ok(store)
    }
    #[cfg(test)]
    pub(super) fn use_case_store(&mut self, store: Store) {
        self.cases.store = Some(Arc::new(store));
        self.cases.loaded = false;
    }
    /// Read every case in the background, then attach loaded member tabs.
    /// Nothing touches the key until a cases folder exists.
    pub(super) fn load_cases(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.cases.loaded || self.cases.loading {
            return;
        }
        let store = match &self.cases.store {
            Some(store) => store.clone(),
            None => {
                let exists = Store::default_dir().is_some_and(|d| d.exists());
                if !exists || cfg!(test) {
                    self.cases.loaded = true;
                    return;
                }
                match self.case_store() {
                    Ok(store) => store,
                    Err(error) => {
                        self.notice = Some(error);
                        self.cases.loaded = true;
                        return;
                    }
                }
            }
        };
        self.cases.loading = true;
        let task = cx.background_executor().spawn(async move { store.list() });
        cx.spawn_in(window, async move |this, cx| {
            let (cases, failed) = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.cases.loading = false;
                this.cases.loaded = true;
                // Keep cases created while loading.
                for case in cases {
                    if !this.cases.cases.iter().any(|c| c.id == case.id) {
                        this.cases.cases.push(case);
                    }
                }
                if !failed.is_empty() {
                    this.notice = Some(format!(
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
                let ids: Vec<_> = this.tabs.iter().map(|t| t.id).collect();
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
        if !self.cases.loaded {
            self.load_cases(window, cx);
            return;
        }
        let Some(tab) = self.tabs.iter().find(|t| t.id == id) else {
            return;
        };
        let Some(view) = tab.view.clone() else {
            return;
        };
        let wanted = tab.record.case.clone();
        let identity = tab.identity.clone();
        let (loaded, has_case) = {
            let v = view.read(cx);
            (
                !v.loading && !v.unavailable && !v.source_only,
                v.case.is_some(),
            )
        };
        if !loaded || has_case {
            return;
        }
        let case = self.cases.cases.iter().find(|case| {
            wanted.as_deref() == Some(case.id.as_str())
                || identity.as_ref().is_some_and(|p| case.members.contains(p))
        });
        match case {
            Some(case) => {
                let link = CaseLink::of(case);
                let identities = case.exported();
                view.update(cx, |view, cx| view.open_case_mapping(link, &identities, cx));
            }
            None if wanted.is_some() => {
                let name = view.read(cx).display_name();
                self.notice = Some(format!(
                    "The saved case for {name} could not be found, so it opened without its \
                     replacement mapping."
                ));
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == id) {
                    tab.record.case = None;
                }
            }
            None => {}
        }
        cx.notify();
    }
    fn tab_view(&self, id: u64) -> Option<Entity<DocumentView>> {
        self.tabs.iter().find(|t| t.id == id)?.view.clone()
    }
    /// Whether this tab can start a case: it has a mapping and none yet.
    pub(super) fn can_create_case(&self, id: u64, cx: &App) -> bool {
        self.tab_view(id).is_some_and(|view| {
            let view = view.read(cx);
            view.case.is_none()
                && view.can_copy_markdown()
                && (!view.pii.review.applied().is_empty() || !view.pii.review.variants().is_empty())
        })
    }
    /// Save this tab's mapping as a new case named after the document.
    pub(super) fn create_case(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let view = self.tab_view(id)?;
        let (name, identities, path) = {
            let v = view.read(cx);
            let name = v.display_name();
            let name = name.strip_suffix(".md").unwrap_or(&name).to_string();
            (
                name,
                v.pii.review.export_identities(),
                v.session.document.path.clone(),
            )
        };
        if identities.is_empty() {
            self.notice = Some(
                "Apply replacements first; a case saves which original each alias stands for."
                    .into(),
            );
            cx.notify();
            return None;
        }
        if let Err(error) = self.case_store() {
            self.notice = Some(error);
            cx.notify();
            return None;
        }
        let case = Case::new(
            name,
            &identities,
            path.as_deref().map(session_store::identity),
        );
        let link = CaseLink::of(&case);
        let case_id = case.id.clone();
        self.cases.cases.insert(0, case.clone());
        self.enqueue_case(case, cx);
        view.update(cx, |view, cx| {
            view.case = Some(link.clone());
            cx.notify();
        });
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == id) {
            tab.record.case = Some(link.id);
        }
        self.checkpoint(window, cx);
        cx.notify();
        Some(case_id)
    }
    /// A member tab's mapping changed: fold it into the case and save.
    pub(super) fn case_changed(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(view) = self.tab_view(id) else {
            return;
        };
        let (link, identities, path) = {
            let v = view.read(cx);
            let Some(link) = v.case.clone() else {
                return;
            };
            (
                link,
                v.pii.review.export_identities(),
                v.session.document.path.clone(),
            )
        };
        let Some(case) = self.cases.cases.iter_mut().find(|c| c.id == link.id) else {
            return;
        };
        let before = case.clone();
        case.merge(&identities);
        if let Some(path) = path.as_deref().map(session_store::identity)
            && !case.members.contains(&path)
        {
            case.members.push(path);
        }
        if *case != before {
            let case = case.clone();
            self.enqueue_case(case, cx);
        }
    }
    /// A member saved under a new path no longer lives at `previous`.
    pub(super) fn case_member_moved(
        &mut self,
        id: u64,
        previous: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        let Some(previous) = previous.as_deref().map(session_store::identity) else {
            return;
        };
        let Some(view) = self.tab_view(id) else {
            return;
        };
        let (link, current) = {
            let v = view.read(cx);
            (
                v.case.clone(),
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
        if let Some(case) = self.cases.cases.iter_mut().find(|c| c.id == link.id)
            && case.members.contains(&previous)
        {
            case.members.retain(|m| *m != previous);
            let case = case.clone();
            self.enqueue_case(case, cx);
        }
    }
    fn enqueue_case(&mut self, mut case: Case, cx: &mut Context<Self>) {
        if let Some(stored) = self.cases.cases.iter_mut().find(|c| c.id == case.id) {
            stored.revision += 1;
            case.revision = stored.revision;
        }
        self.cases.pending.insert(case.id.clone(), case);
        self.pump_cases(cx);
    }
    fn pump_cases(&mut self, cx: &mut Context<Self>) {
        if self.cases.writing.is_some() {
            return;
        }
        let Some(id) = self.cases.pending.keys().next().cloned() else {
            return;
        };
        let case = self.cases.pending.remove(&id).unwrap();
        let store = match self.case_store() {
            Ok(store) => store,
            Err(error) => {
                self.notice = Some(error);
                return;
            }
        };
        self.cases.writing = Some(case.clone());
        let task = cx
            .background_executor()
            .spawn(async move { store.save(&case) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                let Some(case) = this.cases.writing.take() else {
                    return;
                };
                match result {
                    Ok(()) => {
                        this.cases.failed.remove(&case.id);
                        if let Some(tab) = this.cases.close_after.remove(&case.id) {
                            this.close_after_save(tab, cx);
                        }
                        this.pump_cases(cx);
                    }
                    Err(error) => {
                        // Keep it for the next change, close or quit; no busy retry.
                        this.notice = Some(format!("Could not save the case: {error}"));
                        this.cases.failed.insert(case.id.clone());
                        if let Some(tab) = this.cases.close_after.remove(&case.id) {
                            this.cases.asked.remove(&tab);
                        }
                        this.cases.pending.entry(case.id.clone()).or_insert(case);
                    }
                }
                cx.notify();
            });
        })
        .detach();
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
    /// At quit: fold every member tab's mapping in and write it now.
    pub(super) fn flush_cases(&mut self, cx: &mut Context<Self>) {
        let ids: Vec<_> = self.tabs.iter().map(|t| t.id).collect();
        for id in ids {
            self.case_changed(id, cx);
        }
        // The store serializes writes, so this waits for one in flight and
        // never lets it overwrite the newer state.
        let mut cases: Vec<_> = self.cases.writing.clone().into_iter().collect();
        for (_, case) in self.cases.pending.drain() {
            cases.retain(|c| c.id != case.id);
            cases.push(case);
        }
        if cases.is_empty() {
            return;
        }
        if let Ok(store) = self.case_store() {
            for case in cases {
                let _ = store.save(&case);
            }
        }
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
        let Some(link) = view.update(cx, |view, cx| {
            view.case_save = None;
            cx.notify();
            view.case.take()
        }) else {
            return;
        };
        let path = view
            .read(cx)
            .session
            .document
            .path
            .as_deref()
            .map(session_store::identity);
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == id) {
            tab.record.case = None;
        }
        if let Some(case) = self.cases.cases.iter_mut().find(|c| c.id == link.id)
            && let Some(path) = path
            && case.members.contains(&path)
        {
            case.members.retain(|m| *m != path);
            let case = case.clone();
            self.enqueue_case(case, cx);
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
        let Some(name) = self
            .cases
            .cases
            .iter()
            .find(|c| c.id == case_id)
            .map(|c| c.name.clone())
        else {
            return;
        };
        let answer = window.prompt(
            PromptLevel::Warning,
            &format!("Delete the case “{name}” and forget its originals?"),
            Some(
                "Its replacement mapping is wiped from this computer. Documents keep their \
                 text, but aliases in them and in AI replies can no longer be restored. \
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
        self.cases.pending.remove(case_id);
        self.cases.cases.retain(|c| c.id != case_id);
        for tab in &mut self.tabs {
            if tab.record.case.as_deref() == Some(case_id) {
                tab.record.case = None;
            }
            if let Some(view) = &tab.view {
                view.update(cx, |view, cx| {
                    if view.case.as_ref().is_some_and(|l| l.id == case_id) {
                        view.case = None;
                        view.case_save = None;
                        cx.notify();
                    }
                });
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
                        this.notice = Some(format!("Could not delete the case file: {error}"));
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
        let Some(name) = self
            .cases
            .cases
            .iter()
            .find(|c| c.id == case_id)
            .map(|c| c.name.clone())
        else {
            return;
        };
        let input = cx.new(|cx| {
            let mut input = markdown_search::SearchInput::new(cx)
                .with_key_context("CaseName")
                .with_placeholder("Case name");
            input.set_value(name, cx);
            input
        });
        window.focus(&input.read(cx).focus_handle(cx), cx);
        self.cases.renaming = Some((case_id, input));
        cx.notify();
    }
    fn finish_rename(&mut self, save: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some((case_id, input)) = self.cases.renaming.take() else {
            return;
        };
        let name = input.read(cx).value().trim().to_string();
        if save && !name.is_empty() {
            if let Some(case) = self.cases.cases.iter_mut().find(|c| c.id == case_id) {
                case.name = name.clone();
                let case = case.clone();
                self.enqueue_case(case, cx);
            }
            for view in self.tabs.iter().filter_map(|t| t.view.clone()) {
                view.update(cx, |view, cx| {
                    if let Some(link) = view.case.as_mut().filter(|l| l.id == case_id) {
                        link.name = name.clone();
                        cx.notify();
                    }
                });
            }
        }
        self.focus_active(window, cx);
        cx.notify();
    }
    /// Open the documents of a case from the Open Case list.
    pub(super) fn open_case(&mut self, case_id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.cases.list_open = false;
        let Some(case) = self.cases.cases.iter().find(|c| c.id == case_id) else {
            return;
        };
        let existing: Vec<_> = case
            .members
            .iter()
            .filter(|p| p.exists())
            .cloned()
            .collect();
        if existing.is_empty() {
            self.notice = Some(format!(
                "None of the documents in “{}” are where they were saved. Open a document \
                 and save the mapping as a new case.",
                case.name
            ));
            cx.notify();
            return;
        }
        if let Some(open) = existing
            .iter()
            .find_map(|path| self.find_path(path, None, cx))
        {
            self.activate(open, window, cx);
        }
        self.open_paths(existing, window, cx);
        cx.notify();
    }
    pub(super) fn show_case_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.load_cases(window, cx);
        if let Err(error) = self.case_store() {
            self.notice = Some(error);
            cx.notify();
            return;
        }
        self.cases.list_open = true;
        cx.notify();
    }

    /// A case's sidebar header above its first member tab.
    pub(super) fn case_header(
        &self,
        link: &CaseLink,
        first: u64,
        count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let renaming = self
            .cases
            .renaming
            .as_ref()
            .filter(|(id, _)| *id == link.id)
            .map(|(_, input)| input.clone());
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
            .child(
                div()
                    .size(px(8.))
                    .flex_shrink_0()
                    .rounded_full()
                    .bg(link.color()),
            )
            .child(match renaming {
                Some(input) => div()
                    .flex_1()
                    .min_w_0()
                    .px_1()
                    .rounded_sm()
                    .border_1()
                    .border_color(theme.search_accent())
                    .text_color(palette.header_fg)
                    .on_action(cx.listener(|this, _: &ConfirmCaseName, window, cx| {
                        this.finish_rename(true, window, cx)
                    }))
                    .on_action(cx.listener(|this, _: &CancelCaseName, window, cx| {
                        this.finish_rename(false, window, cx)
                    }))
                    .child(input)
                    .into_any_element(),
                None => div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child(link.name.clone())
                    .into_any_element(),
            })
            .when(count > 1, |v| v.child(div().child(count.to_string())))
            .tooltip(style::tooltip(
                format!("Case “{}” · its replacement mapping is saved", link.name),
                theme,
            ))
            .on_mouse_down(
                gpui::MouseButton::Right,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    this.menu_return = window.focused(cx);
                    window.focus(&this.menu_focus, cx);
                    this.tab_menu = Some((first, event.position));
                    cx.notify();
                }),
            )
            .on_click(
                cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                    if event.click_count() == 2 {
                        this.start_rename_case(id.clone(), window, cx);
                    }
                }),
            )
            .into_any_element()
    }

    /// Case entries for the tab context menu.
    pub(super) fn case_menu_items(&self, tab: u64, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let row = |id: &'static str, label: &'static str| {
            div()
                .id(id)
                .role(gpui::Role::Button)
                .aria_label(label)
                .key_context("UiControl")
                .tab_index(0)
                .focus_visible(move |s| s.bg(palette.placeholder_bg))
                .when(cfg!(test), move |v| v.debug_selector(move || id.into()))
                .px_3()
                .py_1()
                .rounded_sm()
                .cursor_pointer()
                .hover(move |v| v.bg(palette.placeholder_bg))
                .child(label)
        };
        let link = self.tab_view(tab).and_then(|v| v.read(cx).case.clone());
        match link {
            Some(link) => {
                let rename = link.id.clone();
                let delete = link.id.clone();
                vec![
                    row("context-rename-case", "Rename case…")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.tab_menu = None;
                            this.start_rename_case(rename.clone(), window, cx);
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
                            this.delete_case(delete.clone(), window, cx);
                        }))
                        .into_any_element(),
                ]
            }
            None if self.can_create_case(tab, cx) => vec![
                row("context-create-case", "Save mapping as case")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.tab_menu = None;
                        this.create_case(tab, window, cx);
                    }))
                    .into_any_element(),
            ],
            None => Vec::new(),
        }
    }

    /// File → Open Case…: every saved case, newest first.
    pub(super) fn case_list(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.cases.list_open {
            return None;
        }
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let open: HashSet<String> = self
            .tabs
            .iter()
            .filter_map(|t| {
                t.view
                    .as_ref()?
                    .read(cx)
                    .case
                    .as_ref()
                    .map(|l| l.id.clone())
            })
            .collect();
        let rows: Vec<AnyElement> = self
            .cases
            .cases
            .iter()
            .map(|case| {
                let link = CaseLink::of(case);
                let open_id = case.id.clone();
                let delete_id = case.id.clone();
                let documents = case.members.len();
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
                            .bg(link.color()),
                    )
                    .child(
                        div()
                            .id(gpui::SharedString::from(format!("open-case-{}", case.id)))
                            .when(cfg!(test), |v| v.debug_selector(|| "open-case-row".into()))
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
                                        "{} {} · {} {}{}",
                                        case.identities.len(),
                                        if case.identities.len() == 1 {
                                            "name"
                                        } else {
                                            "names"
                                        },
                                        documents,
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
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_case(&open_id, window, cx)
                            })),
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
                                this.cases.list_open = false;
                                this.delete_case(delete_id.clone(), window, cx)
                            },
                        )),
                    )
                    .into_any_element()
            })
            .collect();
        Some(
            gpui::deferred(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_start()
                    .justify_center()
                    .pt(px(80.))
                    .child(
                        ui::panel("case-list", theme)
                            .when(cfg!(test), |v| v.debug_selector(|| "case-list".into()))
                            .w(px(420.))
                            .max_h(px(480.))
                            .overflow_y_scroll()
                            .p_2()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                                this.cases.list_open = false;
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
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.cases.list_open = false;
                                                cx.notify();
                                            })),
                                    ),
                            )
                            .when(rows.is_empty(), |v| {
                                v.child(
                                    div()
                                        .px_2()
                                        .py_2()
                                        .text_size(px(12.))
                                        .text_color(palette.header_muted)
                                        .child(
                                            "No saved cases yet. Right-click a tab with applied \
                                             replacements and choose Save mapping as case.",
                                        ),
                                )
                            })
                            .children(rows),
                    ),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }

    /// Before closing a tab whose mapping would be lost, offer to keep it.
    /// Returns true when the close continues now.
    pub(super) fn confirm_mapping_close(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let ask = self
            .preferences
            .borrow()
            .snapshot()
            .map(|p| p.cases.ask_before_closing)
            .unwrap_or(true);
        let Some(view) = self.tab_view(id) else {
            return true;
        };
        let at_risk = {
            let v = view.read(cx);
            !v.pii.review.applied().is_empty()
                && v.case
                    .as_ref()
                    .is_none_or(|l| self.cases.failed.contains(&l.id))
        };
        if !ask
            || !at_risk
            || self.cases.asked.contains(&id)
            || cfg!(not(any(target_os = "macos", windows)))
        {
            return true;
        }
        self.cases.asked.insert(id);
        self.activate(id, window, cx);
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
            let _ = this.update_in(cx, |this, window, cx| match answer {
                Some(0) => {
                    // Close only once the case is safely written.
                    if let Some(case) = this.create_case(id, window, cx) {
                        this.cases.close_after.insert(case, id);
                    } else {
                        this.cases.asked.remove(&id);
                    }
                }
                Some(1) => this.close_tab(id, window, cx),
                _ => {
                    this.cases.asked.remove(&id);
                }
            });
        })
        .detach();
        false
    }
}
