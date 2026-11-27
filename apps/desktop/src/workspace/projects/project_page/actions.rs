//! The Project page's actions: showing it and its segments, its members
//! and replies, colour, Add document, moving a member out, and the names'
//! forget and add.

use super::state::{Place, Segment};
use super::{ProjectPage, header_control};
use crate::document::markdown_search::{SearchInput, SearchInputEvent};
use crate::workspace::ProjectLink;
use crate::workspace::Workspace;
use crate::workspace::projects::text;
use gpui::{Context, Focusable, PathPromptOptions, Window, prelude::*, px};
use mdoc_store::document_id::{DocumentId, VaultId};
use mdoc_vault::{Colour, DocumentSummary, ReplySummary};
use std::{cell::RefCell, collections::HashMap};

impl Workspace {
    /// Whether a member's row says Needs review: a live document by what
    /// its review holds now, another by what its last save counted.
    pub(in crate::workspace) fn member_needs_review(
        &self,
        summary: &DocumentSummary,
        cx: &gpui::App,
    ) -> bool {
        let live = VaultId::parse(&summary.id)
            .and_then(|document| self.find_document(&DocumentId::Vault(document), None, cx))
            .and_then(|tab| self.tabs.get(tab)?.view().cloned())
            .map(|view| view.read(cx).pii.review().remaining());
        crate::workspace::home::needs_review(
            summary,
            crate::workspace::home::proposals(summary, live),
        )
    }

    /// The Project tab `tab` shows in the sidebar under: its view's, or,
    /// before it loads, the one the Vault files its document in.
    pub(in crate::workspace) fn sidebar_project_link(
        &self,
        tab: &crate::workspace::Tab,
        cx: &gpui::App,
    ) -> Option<ProjectLink> {
        if let Some(view) = tab.view()
            && !view.read(cx).lifecycle.loading()
        {
            return view.read(cx).project.link().cloned();
        }
        let document = tab.record().document.as_ref()?;
        self.projects.link(&self.project_of(document.as_str())?)
    }

    /// Open the Project ⋯ menu for Project `id` at `position`.
    pub(in crate::workspace) fn open_project_menu(
        &mut self,
        id: &str,
        position: gpui::Point<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_return = window.focused(cx);
        self.project_ui.header_menu = Some((id.to_string(), position));
        window.focus(&self.menu_focus, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn confirm_forget_project_identity(
        &mut self,
        id: &str,
        alias: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (id, alias) = (id.to_string(), alias.to_string());
        let answer = window.prompt(
            gpui::PromptLevel::Warning,
            &text::forget_identity_question(&alias),
            Some(text::FORGET_IDENTITY_DETAIL),
            &[text::FORGET_IDENTITY, crate::ui::text::CANCEL],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            if answer.await.ok() == Some(0) {
                let _ = this.update_in(cx, |this, window, cx| {
                    this.forget_project_identity(&id, &alias, cx);
                    if let Some(page) = this.project_ui.page.as_ref().filter(|page| page.id == id) {
                        window.focus(&page.add.read(cx).focus_handle(cx), cx);
                    }
                });
            }
        })
        .detach();
    }

    pub(in crate::workspace) fn forget_project_identity(
        &mut self,
        id: &str,
        alias: &str,
        cx: &mut Context<Self>,
    ) {
        let Some((forgotten, exported)) = self.projects.edit(id, |project| {
            let identity = (project.mapping.identities.iter())
                .find(|identity| identity.names(alias))?
                .clone();
            project
                .forget_identity(alias)
                .then(|| (identity, project.mapping.exported()))
        }) else {
            return;
        };
        self.write_project(id, cx);
        for view in self.tabs.iter().filter_map(|tab| tab.view()) {
            view.update(cx, |view, cx| {
                if !view.project.is(id) {
                    return;
                }
                let aliases: Vec<_> = std::iter::once(forgotten.alias.as_str())
                    .chain(forgotten.previous.iter().map(String::as_str))
                    .collect();
                view.forget_project_identity(&aliases, &forgotten.originals, &exported, cx);
            });
        }
        cx.notify();
    }

    pub(in crate::workspace) fn show_project_page(
        &mut self,
        id: &str,
        place: Place,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.projects.project(id).is_none() {
            return;
        }
        if let Some(view) = self.active_view() {
            view.update(cx, |view, _| {
                view.active = false;
                view.pii.dismiss_popup();
            });
        }
        self.reply_page = None;
        self.project_ui.list_open = false;
        self.project_list_return = None;
        self.end_rename(true, cx);
        let input = |cx: &mut gpui::Context<SearchInput>, placeholder, context| {
            SearchInput::new(cx)
                .with_theme(self.theme.clone())
                .with_placeholder(placeholder)
                .with_key_context(context)
        };
        let filter = cx.new(|cx| input(cx, text::SEARCH_NAMES, "ProjectPageFilter"));
        let add = cx.new(|cx| input(cx, text::ADD_A_NAME, "ProjectTerm"));
        let changes = cx.subscribe(&filter, |_, _, _: &SearchInputEvent, cx| cx.notify());
        let segment = match place {
            Place::Categories => Segment::Names,
            Place::Header | Place::Rename | Place::Colour | Place::Delete => Segment::Documents,
        };
        let page = ProjectPage {
            id: id.into(),
            segment,
            swatches: matches!(place, Place::Colour),
            #[cfg(all(
                debug_assertions,
                feature = "headless-renders",
                any(target_os = "macos", target_os = "windows")
            ))]
            place,
            filter,
            add,
            scroll: gpui::ScrollHandle::new(),
            color_focus: cx.focus_handle().tab_index(0).tab_stop(true),
            category_focus: cx.focus_handle().tab_index(0).tab_stop(true),
            controls: RefCell::new(HashMap::new()),
            _filter_changes: changes,
        };
        let focus = match place {
            Place::Categories => page.category_focus.clone(),
            _ => page.color_focus.clone(),
        };
        self.project_ui.page = Some(page);
        self.note_destination(crate::workspace::sidebar::navigation::Arrival::Visit, cx);
        self.drop_if_empty_new(self.tabs.active(), window, cx);
        match place {
            Place::Rename => self.start_rename_project(id.into(), window, cx),
            Place::Delete => self.delete_project(id.into(), window, cx),
            // Controls are mounted on the next frame; GPUI reveals a focused descendant.
            _ => window.defer(cx, move |window, cx| window.focus(&focus, cx)),
        }
        cx.notify();
    }

    /// Show `segment` of the page; focus stays on its toggle.
    pub(in crate::workspace) fn show_project_segment(
        &mut self,
        segment: Segment,
        cx: &mut Context<Self>,
    ) {
        let Some(page) = self.project_ui.page.as_mut() else {
            return;
        };
        if page.segment == segment {
            return;
        }
        page.segment = segment;
        if segment == Segment::Activity {
            let id = page.id.clone();
            self.load_ai_activity(&id);
        }
        let Some(page) = self.project_ui.page.as_mut() else {
            return;
        };
        page.controls
            .borrow_mut()
            .retain(|id, _| header_control(id));
        page.scroll.set_offset(gpui::point(px(0.), px(0.)));
        cx.notify();
    }

    pub(in crate::workspace) fn set_project_color(
        &mut self,
        id: &str,
        colour: Colour,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(link) = self.projects.edit(id, |project| {
            project.colour = Some(colour);
            Some(ProjectLink::of(project))
        }) {
            self.write_project(id, cx);
            for view in self.tabs.iter().filter_map(|tab| tab.view()) {
                view.update(cx, |view, cx| {
                    if view.project.is(id) {
                        view.project.attach(Some(link.clone()));
                        cx.notify();
                    }
                });
            }
            self.checkpoint(window, cx);
        }
        if let Some(page) = self.project_ui.page.as_mut() {
            page.swatches = false;
            window.focus(&page.color_focus, cx);
        }
        cx.notify();
    }

    pub(in crate::workspace) fn open_project_member(
        &mut self,
        document: VaultId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_vault_document(document, window, cx);
    }

    /// Add document: the system file picker; what is chosen is imported
    /// into Project `id`.
    pub(in crate::workspace) fn add_document_to_project(
        &mut self,
        id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.intake_refused(window, cx) {
            return;
        }
        let name = self.projects.project(&id).map(|p| p.name.clone());
        let Some(name) = name else {
            return;
        };
        let paths = crate::app::native_picker::paths(
            cx,
            PathPromptOptions {
                files: true,
                directories: false,
                multiple: true,
                prompt: Some(text::add_to_curly(&name).into()),
            },
        );
        cx.spawn_in(window, async move |this, cx| {
            let paths = match paths.await {
                Ok(Some(paths)) => paths,
                Err(error) => {
                    let _ = this.update(cx, |this, cx| {
                        this.notices.post(text::chooser_did_not_show(&error));
                        cx.notify();
                    });
                    return;
                }
                // Cancelled.
                _ => return,
            };
            let _ = this.update_in(cx, |this, window, cx| {
                this.open_paths_into(paths, Some(id), window, cx);
            });
        })
        .detach();
    }

    /// The Project page's Move to Unfiled; its document may be closed.
    pub(in crate::workspace) fn remove_project_member(
        &mut self,
        id: &str,
        document: VaultId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_document(document, None, window, cx);
        if let Some(page) = self.project_ui.page.as_ref().filter(|page| page.id == id) {
            let focus = page
                .controls
                .borrow()
                .get(Segment::Documents.control())
                .cloned();
            if let Some(focus) = focus {
                window.focus(&focus, cx);
            }
        }
        cx.notify();
    }

    /// Project `id`'s documents, by title, from the Vault's summaries.
    #[cfg(any(
        test,
        all(
            debug_assertions,
            feature = "headless-renders",
            any(target_os = "macos", target_os = "windows")
        )
    ))]
    pub(in crate::workspace) fn members(&self, id: &str) -> Vec<(VaultId, String)> {
        (self.member_summaries(id).into_iter())
            .filter_map(|summary| Some((VaultId::parse(&summary.id)?, summary.title)))
            .collect()
    }

    /// Project `id`'s documents' summaries, by title.
    pub(super) fn member_summaries(&self, id: &str) -> Vec<DocumentSummary> {
        let Some(vault) = self.vault.vault() else {
            return Vec::new();
        };
        let mut members = vault.documents_in(Some(id));
        members.sort_by_key(|summary| summary.title.to_lowercase());
        members
    }

    /// Project `id`'s AI Replies, newest first: its own and its documents'.
    pub(in crate::workspace) fn project_replies(&self, id: &str) -> Vec<ReplySummary> {
        (self.vault.vault()).map_or_else(Vec::new, |vault| vault.project_replies(id))
    }

    pub(in crate::workspace) fn add_page_name(&mut self, cx: &mut Context<Self>) {
        let Some(page) = &self.project_ui.page else {
            return;
        };
        let (id, input) = (page.id.clone(), page.add.clone());
        let term = input.read(cx).value().to_string();
        self.add_project_name(&id, &term, cx);
        input.update(cx, |input, cx| input.reset(cx));
    }

    /// Every document's title by its id, for the replies their owners' names.
    pub(super) fn document_titles(&self) -> std::collections::HashMap<String, String> {
        (self.vault.vault())
            .map(|vault| {
                (vault.documents().into_iter())
                    .map(|summary| (summary.id, summary.title))
                    .collect()
            })
            .unwrap_or_default()
    }
}
