//! The Project page: a header (colour, title, counts, Add document, ⋯) over
//! three segments, `Documents`, `Replies` and `Names`, from the window's
//! Projects and the Vault's summaries; it opens no document.
use super::ProjectLink;
use super::{
    Workspace,
    projects_ui::{AddProjectName, CancelProjectName, ClearProjectName, ConfirmProjectName},
};
use crate::{
    markdown_search::{SearchInput, SearchInputEvent},
    style, ui,
};
use gpui::{
    AnyElement, Context, Entity, FocusHandle, Focusable, PathPromptOptions, SharedString,
    Subscription, Window, div, prelude::*, px,
};
use mdoc_store::document_id::{DocumentId, VaultId};
use mdoc_vault::{Colour, DocumentSummary, Project, ReplySummary, SourceKind};
use std::{cell::RefCell, collections::HashMap};

/// A swatch's name, as the page reads it out.
pub(crate) fn colour_name(colour: Colour) -> &'static str {
    match colour {
        Colour::Red => "Red",
        Colour::Orange => "Orange",
        Colour::Amber => "Amber",
        Colour::Green => "Green",
        Colour::Teal => "Teal",
        Colour::Blue => "Blue",
        Colour::Violet => "Violet",
        Colour::Pink => "Pink",
    }
}

#[derive(Default)]
pub(super) struct ProjectClose {
    waiting: Option<u64>,
    queue: std::collections::VecDeque<u64>,
}
impl ProjectClose {
    pub(super) fn begin(&mut self, ids: impl IntoIterator<Item = u64>) -> Option<u64> {
        self.queue = ids.into_iter().collect();
        self.waiting = self.queue.pop_front();
        self.waiting
    }
    pub(super) fn removed(&mut self, id: u64) -> Option<u64> {
        if self.waiting != Some(id) {
            return None;
        }
        self.waiting = self.queue.pop_front();
        self.waiting
    }
}

/// One of the page's three views of its Project.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Segment {
    Documents,
    Replies,
    Names,
}

impl Segment {
    pub(crate) const ALL: [Segment; 3] = [Segment::Documents, Segment::Replies, Segment::Names];

    fn label(self) -> &'static str {
        match self {
            Segment::Documents => "Documents",
            Segment::Replies => "Replies",
            Segment::Names => "Names",
        }
    }

    fn control(self) -> &'static str {
        match self {
            Segment::Documents => "project-segment-documents",
            Segment::Replies => "project-segment-replies",
            Segment::Names => "project-segment-names",
        }
    }
}

/// Where a request to show the page lands.
#[derive(Clone, Copy)]
pub(super) enum Place {
    /// Documents, focus on the colour.
    Header,
    Rename,
    /// The swatches open.
    Colour,
    /// Names, at Categories to detect.
    Categories,
    /// The page, then the delete question.
    Delete,
}

/// View state only: never serialized and cleared on any tab activation.
pub(super) struct ProjectPage {
    id: String,
    segment: Segment,
    swatches: bool,
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    place: Place,
    filter: Entity<SearchInput>,
    add: Entity<SearchInput>,
    scroll: gpui::ScrollHandle,
    color_focus: FocusHandle,
    category_focus: FocusHandle,
    controls: RefCell<HashMap<String, FocusHandle>>,
    _filter_changes: Subscription,
}

/// Controls of the header and the segment row, kept across segments.
fn header_control(id: &str) -> bool {
    id.starts_with("project-segment-")
        || id.starts_with("project-swatch-")
        || matches!(
            id,
            "project-page-rename" | "project-page-add-document" | "project-page-menu"
        )
}

impl ProjectPage {
    pub(super) fn id(&self) -> &str {
        &self.id
    }

    #[cfg(test)]
    pub(crate) fn segment(&self) -> Segment {
        self.segment
    }

    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    pub(super) fn categories(&self) -> bool {
        matches!(self.place, Place::Categories)
    }

    #[cfg(test)]
    pub(super) fn test_inputs(&self) -> (Entity<SearchInput>, Entity<SearchInput>) {
        (self.filter.clone(), self.add.clone())
    }

    /// The controls the page draws now: the header's and its segment's.
    #[cfg(test)]
    pub(super) fn test_controls(&self) -> Vec<(String, FocusHandle)> {
        let mut controls: Vec<_> = self
            .controls
            .borrow()
            .iter()
            .map(|(id, focus)| (id.clone(), focus.clone()))
            .collect();
        controls.push(("project-page-color".into(), self.color_focus.clone()));
        if self.segment == Segment::Names {
            controls.push(("page-category-PERSON".into(), self.category_focus.clone()));
        }
        controls
    }
    #[cfg(test)]
    pub(super) fn test_scroll_bounds(&self) -> gpui::Bounds<gpui::Pixels> {
        self.scroll.bounds()
    }
    #[cfg(test)]
    pub(super) fn test_scroll_offset(&self) -> gpui::Pixels {
        self.scroll.offset().y
    }
}

impl Workspace {
    /// The Project tab `tab` shows in the sidebar under: its view's, or,
    /// before it loads, the one the Vault files its document in.
    pub(super) fn sidebar_project_link(
        &self,
        tab: &super::Tab,
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
    pub(super) fn open_project_menu(
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

    /// The Project ⋯ menu, from the page's header or the sidebar's.
    pub(super) fn project_header_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (id, position) = self.project_ui.header_menu.as_ref()?;
        let theme = self.theme.get();
        let mut menu = ui::panel("project-header-actions", theme)
            .track_focus(&self.menu_focus)
            .key_context("UiPanel UiMenu")
            .flex()
            .flex_col()
            .p_2()
            .gap_1()
            .on_action(cx.listener(|this, _: &ui::NextControl, window, cx| {
                ui::cycle(window, cx, Some(&this.menu_focus), false);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &ui::PreviousControl, window, cx| {
                ui::cycle(window, cx, Some(&this.menu_focus), true);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &ui::CloseMenu, window, cx| {
                this.project_ui.header_menu = None;
                if let Some(focus) = this.menu_return.take() {
                    window.focus(&focus, cx);
                }
                cx.notify();
            }))
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.project_ui.header_menu = None;
                cx.notify();
            }));
        for (key, label, place) in [
            ("project-action-rename", "Rename", Place::Rename),
            ("project-action-colour", "Colour", Place::Colour),
            (
                "project-action-categories",
                "Categories to detect",
                Place::Categories,
            ),
        ] {
            let id = id.clone();
            menu = menu.child(
                ui::control(key, label, theme, true)
                    .when(cfg!(test), move |v| v.debug_selector(move || key.into()))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.project_ui.header_menu = None;
                        this.show_project_page(&id, place, window, cx);
                    })),
            );
        }
        // Its Markdown and originals as plain files (#1091 Decision 22).
        let export = id.clone();
        menu = menu.child(
            ui::control("project-action-export", "Export Project…", theme, true)
                .when(cfg!(test), |v| {
                    v.debug_selector(|| "project-action-export".into())
                })
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.project_ui.header_menu = None;
                    this.export_project(export.clone(), window, cx);
                })),
        );
        let delete = id.clone();
        menu = menu.child(ui::menu_divider(theme)).child(
            ui::control("project-action-delete", "Delete Project…", theme, true)
                .text_color(style::markdown_style(theme).popover_danger)
                .when(cfg!(test), |v| {
                    v.debug_selector(|| "project-action-delete".into())
                })
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.project_ui.header_menu = None;
                    this.delete_project(delete.clone(), window, cx);
                })),
        );
        Some(
            gpui::deferred(
                gpui::anchored()
                    .position(*position)
                    .snap_to_window()
                    .child(menu),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }
    pub(super) fn confirm_forget_project_identity(
        &mut self,
        id: &str,
        alias: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (id, alias) = (id.to_string(), alias.to_string());
        let answer = window.prompt(gpui::PromptLevel::Warning, &format!("Forget identity “{alias}”?"),
            Some("Its originals are forgotten from this Project and its documents. Document text is untouched. This cannot be undone."), &["Forget Identity", "Cancel"], cx);
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

    pub(super) fn forget_project_identity(
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
    pub(super) fn show_project_page(
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
        let filter = cx.new(|cx| input(cx, "Search names", "ProjectPageFilter"));
        let add = cx.new(|cx| input(cx, "Add a name", "ProjectTerm"));
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
        self.note_destination(super::navigation::Arrival::Visit, cx);
        match place {
            Place::Rename => self.start_rename_project(id.into(), window, cx),
            Place::Delete => self.delete_project(id.into(), window, cx),
            // Controls are mounted on the next frame; GPUI reveals a focused descendant.
            _ => window.defer(cx, move |window, cx| window.focus(&focus, cx)),
        }
        cx.notify();
    }

    /// Show `segment` of the page; focus stays on its toggle.
    pub(super) fn show_project_segment(&mut self, segment: Segment, cx: &mut Context<Self>) {
        let Some(page) = self.project_ui.page.as_mut() else {
            return;
        };
        if page.segment == segment {
            return;
        }
        page.segment = segment;
        page.controls
            .borrow_mut()
            .retain(|id, _| header_control(id));
        page.scroll.set_offset(gpui::point(px(0.), px(0.)));
        cx.notify();
    }

    pub(super) fn set_project_color(
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

    pub(super) fn open_project_member(
        &mut self,
        document: VaultId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_vault_document(document, window, cx);
    }

    /// Add document: the system file picker; what is chosen is imported
    /// into Project `id`.
    pub(super) fn add_document_to_project(
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
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some(format!("Add to \u{201c}{name}\u{201d}").into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let paths = match paths.await {
                Ok(Ok(Some(paths))) => paths,
                Ok(Err(error)) => {
                    let _ = this.update(cx, |this, cx| {
                        this.notices
                            .post(format!("Could not show the file chooser: {error}"));
                        cx.notify();
                    });
                    return;
                }
                // Cancelled.
                _ => return,
            };
            let _ = this.update_in(cx, |this, window, cx| {
                this.open_paths_into(paths, None, Some(id), window, cx);
            });
        })
        .detach();
    }

    /// The Project page's Move to Unfiled; its document may be closed.
    pub(super) fn remove_project_member(
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
    pub(super) fn members(&self, id: &str) -> Vec<(VaultId, String)> {
        (self.member_summaries(id).into_iter())
            .filter_map(|summary| Some((VaultId::parse(&summary.id)?, summary.title)))
            .collect()
    }

    /// Project `id`'s documents' summaries, by title.
    fn member_summaries(&self, id: &str) -> Vec<DocumentSummary> {
        let Some(vault) = self.vault.vault() else {
            return Vec::new();
        };
        let mut members = vault.documents_in(Some(id));
        members.sort_by_key(|summary| summary.title.to_lowercase());
        members
    }

    /// Project `id`'s AI Replies, newest first: its own and its documents'.
    pub(super) fn project_replies(&self, id: &str) -> Vec<ReplySummary> {
        (self.vault.vault()).map_or_else(Vec::new, |vault| vault.project_replies(id))
    }

    pub(super) fn project_page(&mut self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let page = self.project_ui.page.as_ref().unwrap();
        let Some(project) = self.projects.project(&page.id).cloned() else {
            let gone = super::navigation::Stop::Project(page.id.clone());
            self.project_ui.page = None;
            self.tabs.show_home();
            self.note_destination(super::navigation::Arrival::Replace, cx);
            self.navigation.forget(&gone);
            return self.home_view(window, cx);
        };
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let detached = self.title.detached.get();
        let bar = ui::title_drag(
            div()
                .flex()
                .items_center()
                .flex_shrink_0()
                .h(px(style::HEADER_HEIGHT))
                .pl(px(16. + self.title.inset.get()))
                .pr_2()
                .gap_2()
                .when(detached, |v| {
                    v.h(px(style::CARD_MARGIN + style::HEADER_HEIGHT))
                        .pt(px(style::CARD_MARGIN))
                })
                .text_size(px(13.)),
            self.title.armed.clone(),
        )
        .child(
            div()
                .size(px(8.))
                .rounded_full()
                .bg(style::project_color(ProjectLink::of(&project).hue(), theme)),
        )
        .child("Project")
        .child(div().flex_1())
        .children(ui::notice_bell(&self.title, theme).map(|bell| ui::no_drag(div().child(bell))));
        let header = ui::title_bar(bar, (!detached).then_some(p.border), window, theme);
        let content = self.unlocked_project_page(&project, cx);
        let body = div()
            .id("project-page-scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&page.scroll)
            .child(div().w_full().flex().justify_center().p_6().child(content));
        let drop_into = project.id.clone();
        div()
            .id("project-page")
            .when(cfg!(test), |v| v.debug_selector(|| "project-page".into()))
            .track_focus(self.focus_regions.home())
            .size_full()
            .flex()
            .flex_col()
            // Files dropped anywhere on the page are imported into it.
            .on_drop(
                cx.listener(move |this, paths: &gpui::ExternalPaths, window, cx| {
                    cx.stop_propagation();
                    this.shell.drop = None;
                    this.drop_on_project(paths, &drop_into, window, cx);
                }),
            )
            .child(header)
            .child(if detached {
                // The card clips its corners; it wraps the scroll area so its
                // overflow_hidden does not replace the area's scrolling.
                ui::card_surface(
                    div()
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .flex_col()
                        .ml(px(style::CARD_MARGIN))
                        .child(body),
                    theme,
                )
                .into_any_element()
            } else {
                body.into_any_element()
            })
            .into_any_element()
    }

    fn page_control(
        &self,
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) -> ui::Control {
        let page = self.project_ui.page.as_ref().unwrap();
        let id = id.into();
        let focus = match id.as_ref() {
            "project-page-color" => page.color_focus.clone(),
            "page-category-PERSON" => page.category_focus.clone(),
            _ => page
                .controls
                .borrow_mut()
                .entry(id.to_string())
                .or_insert_with(|| cx.focus_handle().tab_index(0).tab_stop(true))
                .clone(),
        };
        ui::reveal_focus(
            ui::control(id.clone(), label, self.theme.get(), true)
                .track_focus(&focus)
                .when(cfg!(test), |v| v.debug_selector(move || id.to_string())),
            focus,
            page.scroll.clone(),
        )
    }

    fn unlocked_project_page(&self, project: &Project, cx: &mut Context<Self>) -> AnyElement {
        let page = self.project_ui.page.as_ref().unwrap();
        let members = self.member_summaries(&project.id);
        let replies = self.project_replies(&project.id);
        let heading = self.project_heading(project, members.len(), replies.len(), cx);
        let segments = div()
            .id("project-segments")
            .role(gpui::Role::TabList)
            .aria_label("Project view")
            .flex()
            .items_center()
            .gap_1()
            .p_1()
            .rounded_lg()
            .border_1()
            .border_color(self.theme.get().pdf_style().border)
            .children(Segment::ALL.into_iter().map(|segment| {
                let p = self.theme.get().pdf_style();
                let selected = page.segment == segment;
                self.page_control(segment.control(), segment.label(), cx)
                    .role(gpui::Role::Tab)
                    .aria_selected(selected)
                    .px_3()
                    .when(selected, |v| v.bg(p.placeholder_bg).text_color(p.header_fg))
                    .when(!selected, |v| v.text_color(p.header_muted))
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.show_project_segment(segment, cx)),
                    )
            }));
        let body = match page.segment {
            Segment::Documents => self.documents_segment(project, &members, cx),
            Segment::Replies => self.replies_segment(project, &replies, cx),
            Segment::Names => self.names_segment(project, &members, cx),
        };
        div()
            .w_full()
            .max_w(px(680.))
            .flex()
            .flex_col()
            .gap_5()
            .text_size(px(13.))
            .child(heading)
            .child(div().flex().child(segments))
            .child(body)
            .into_any_element()
    }

    /// Colour, title, counts, Add document and ⋯.
    fn project_heading(
        &self,
        project: &Project,
        documents: usize,
        replies: usize,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let page = self.project_ui.page.as_ref().unwrap();
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let id = project.id.clone();
        let names = project.mapping.exported().len();
        let name = match self
            .project_ui
            .renaming
            .as_ref()
            .filter(|(id, _)| *id == project.id)
        {
            Some((_, input)) => div()
                .flex_1()
                .min_w_0()
                .border_1()
                .border_color(p.border)
                .rounded_sm()
                .p_1()
                .on_action(cx.listener(|this, _: &ConfirmProjectName, window, cx| {
                    this.finish_rename(true, window, cx);
                    cx.stop_propagation();
                }))
                .on_action(cx.listener(|this, _: &CancelProjectName, window, cx| {
                    this.finish_rename(false, window, cx);
                    cx.stop_propagation();
                }))
                // A click anywhere else saves the name, like Enter.
                .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                    this.finish_rename(true, window, cx);
                }))
                .child(input.clone())
                .into_any_element(),
            None => self
                .page_control("project-page-rename", project.name.clone(), cx)
                .aria_label(format!("Rename {}", project.name))
                .flex_1()
                .min_w_0()
                .truncate()
                .text_size(px(26.))
                .on_click(cx.listener({
                    let id = id.clone();
                    move |this, _, window, cx| this.start_rename_project(id.clone(), window, cx)
                }))
                .into_any_element(),
        };
        let color = style::project_color(ProjectLink::of(project).hue(), theme);
        let (add, menu) = (id.clone(), id.clone());
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        self.page_control("project-page-color", "", cx)
                            .aria_label("Project colour")
                            .p_2()
                            .child(div().size(px(16.)).rounded_full().bg(color))
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(page) = this.project_ui.page.as_mut() {
                                    page.swatches = !page.swatches;
                                }
                                cx.notify();
                            })),
                    )
                    .child(name)
                    .child(
                        self.page_control("project-page-add-document", "Add document", cx)
                            .flex_shrink_0()
                            .border_1()
                            .border_color(p.border)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.add_document_to_project(add.clone(), window, cx)
                            })),
                    )
                    .child(
                        self.page_control("project-page-menu", "⋯", cx)
                            .aria_label(format!("Actions for Project {}", project.name))
                            .flex_shrink_0()
                            .on_click(cx.listener(
                                move |this, event: &gpui::ClickEvent, window, cx| {
                                    this.open_project_menu(&menu, event.position(), window, cx)
                                },
                            )),
                    ),
            )
            .child(
                div()
                    .id("project-page-counts")
                    .when(cfg!(test), |v| {
                        v.debug_selector(|| "project-page-counts".into())
                    })
                    .text_size(px(12.))
                    .text_color(p.header_muted)
                    .child(project_counts(documents, replies, names, project.created)),
            )
            .when(page.swatches, |v| {
                v.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .children(Colour::ALL.into_iter().map(|swatch| {
                            let id = id.clone();
                            let name = colour_name(swatch);
                            self.page_control(
                                SharedString::from(format!("project-swatch-{name}")),
                                name,
                                cx,
                            )
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .size(px(12.))
                                    .rounded_full()
                                    .bg(style::project_color(swatch.hue(), theme)),
                            )
                            .aria_selected(
                                project
                                    .colour
                                    .unwrap_or_else(|| Colour::for_id(&project.id))
                                    == swatch,
                            )
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.set_project_color(&id, swatch, window, cx)
                                },
                            ))
                        })),
                )
            })
    }

    /// The Project's documents by title, each opening on a click, with
    /// Move to Unfiled.
    fn documents_segment(
        &self,
        project: &Project,
        members: &[DocumentSummary],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.theme.get().pdf_style();
        let mut list = div()
            .id("project-documents")
            .when(cfg!(test), |v| {
                v.debug_selector(|| "project-documents".into())
            })
            .flex()
            .flex_col()
            .gap_1();
        if members.is_empty() {
            list = list.child(empty_line(
                "No documents yet. Add a document, or drop files on this page.",
                p,
            ));
        }
        for (index, summary) in members.iter().enumerate() {
            let Some(document) = VaultId::parse(&summary.id) else {
                continue;
            };
            let (open_document, remove_document) = (document.clone(), document);
            let id = project.id.clone();
            let detail = format!(
                "{}{}",
                if summary.scanned {
                    ""
                } else {
                    "Needs review · "
                },
                format_args!("updated {}", created_date(summary.updated))
            );
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .py_1()
                    .child(kind_badge(summary.source_kind, p))
                    .child(
                        self.page_control(
                            SharedString::from(format!("project-document-{index}")),
                            summary.title.clone(),
                            cx,
                        )
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.open_project_member(open_document.clone(), window, cx)
                            },
                        )),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(11.))
                            .text_color(p.header_muted)
                            .child(detail),
                    )
                    .child(
                        self.page_control(
                            SharedString::from(format!("project-remove-{index}")),
                            "Move to Unfiled",
                            cx,
                        )
                        .flex_shrink_0()
                        .aria_label(format!("Move “{}” to Unfiled", summary.title))
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.remove_project_member(&id, remove_document.clone(), window, cx)
                            },
                        )),
                    ),
            );
        }
        list.into_any_element()
    }

    /// The Project's AI Replies, newest first, each opening restored, with
    /// Delete….
    fn replies_segment(
        &self,
        _project: &Project,
        replies: &[ReplySummary],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.theme.get().pdf_style();
        let titles: HashMap<String, String> = (self.vault.vault())
            .map(|vault| {
                (vault.documents().into_iter())
                    .map(|summary| (summary.id, summary.title))
                    .collect()
            })
            .unwrap_or_default();
        let mut list = div()
            .id("project-replies")
            .when(cfg!(test), |v| {
                v.debug_selector(|| "project-replies".into())
            })
            .flex()
            .flex_col()
            .gap_1();
        if replies.is_empty() {
            list = list.child(empty_line(
                "No saved replies. An AI reply pasted into one of this Project's documents is \
                 kept here.",
                p,
            ));
        }
        for (index, reply) in replies.iter().enumerate() {
            let detail = super::saved_replies::reply_detail(
                &reply.owner,
                reply.source_app.as_deref(),
                reply.created,
                &titles,
            );
            let (open, delete) = (reply.id.clone(), reply.id.clone());
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .py_1()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                self.page_control(
                                    SharedString::from(format!("project-reply-{index}")),
                                    reply.title.clone(),
                                    cx,
                                )
                                .truncate()
                                .on_click(cx.listener(
                                    move |this, _, window, cx| this.open_reply(&open, window, cx),
                                )),
                            )
                            .child(
                                div()
                                    .px_2()
                                    .text_size(px(11.))
                                    .text_color(p.header_muted)
                                    .truncate()
                                    .child(detail),
                            ),
                    )
                    .child(
                        self.page_control(
                            SharedString::from(format!("project-reply-delete-{index}")),
                            "Delete…",
                            cx,
                        )
                        .flex_shrink_0()
                        .aria_label(format!("Delete “{}”", reply.title))
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.confirm_delete_reply(&delete, window, cx)
                            },
                        )),
                    ),
            );
        }
        list.into_any_element()
    }

    /// The Project's names, searchable and grouped by category, with Add a
    /// name, the Always and Never lists and Categories to detect.
    fn names_segment(
        &self,
        project: &Project,
        members: &[DocumentSummary],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let page = self.project_ui.page.as_ref().unwrap();
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let id = project.id.clone();
        let filter = page.filter.read(cx).value().to_lowercase();
        let mut names = div().flex().flex_col().gap_2().child(ui::reveal_focus(
            div()
                .id("project-page-filter")
                .when(cfg!(test), |v| {
                    v.debug_selector(|| "project-page-filter".into())
                })
                .border_1()
                .border_color(p.border)
                .rounded_sm()
                .p_1()
                .child(page.filter.clone()),
            page.filter.read(cx).focus_handle(cx),
            page.scroll.clone(),
        ));
        // Mentions in each live document of the Project, by alias.
        let open_members: Vec<_> = (members.iter())
            .filter_map(|summary| {
                let document = DocumentId::Vault(VaultId::parse(&summary.id)?);
                let tab = self.find_document(&document, None, cx)?;
                Some((self.tabs.get(tab)?.view()?.clone(), summary.title.clone()))
            })
            .collect();
        let mut shown = 0;
        for category in mdoc_pii::Category::ALL {
            let identities: Vec<_> = (project.mapping.identities.iter())
                .filter(|identity| {
                    !identity.originals.is_empty()
                        && identity.category == category.token()
                        && (filter.is_empty()
                            || format!(
                                "{} {} {}",
                                identity.alias,
                                identity.originals.join(" "),
                                category.label()
                            )
                            .to_lowercase()
                            .contains(&filter))
                })
                .collect();
            if identities.is_empty() {
                continue;
            }
            shown += identities.len();
            names = names.child(
                div()
                    .pt_2()
                    .text_size(px(12.))
                    .text_color(p.header_muted)
                    .child(category.label()),
            );
            for identity in identities {
                let alias = identity.alias.clone();
                let id = id.clone();
                let counts = (open_members.iter())
                    .map(|(view, title)| {
                        let review = view.read(cx).pii.review();
                        let count = review
                            .identity_by_alias(&identity.alias)
                            .map_or(0, |id| review.identity_count(id));
                        format!("{title}: {count}")
                    })
                    .collect::<Vec<_>>()
                    .join(" · ");
                names = names.child(
                    div()
                        .id(SharedString::from(format!("project-identity-{alias}")))
                        .when(cfg!(test), {
                            let alias = alias.clone();
                            move |v| v.debug_selector(move || format!("project-identity-{alias}"))
                        })
                        .flex()
                        .items_start()
                        .gap_2()
                        .py_1()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(div().text_size(px(13.)).child(format!(
                                    "{} · {}",
                                    identity.alias,
                                    identity.originals.first().map_or("", String::as_str)
                                )))
                                .child(div().text_size(px(11.)).text_color(p.header_muted).child(
                                    format!(
                                        "{} {}{}",
                                        identity.originals.len(),
                                        if identity.originals.len() == 1 {
                                            "variant"
                                        } else {
                                            "variants"
                                        },
                                        if counts.is_empty() {
                                            String::new()
                                        } else {
                                            format!(" · {counts}")
                                        }
                                    ),
                                )),
                        )
                        .child(
                            self.page_control(
                                SharedString::from(format!("project-forget-{alias}")),
                                "Forget identity…",
                                cx,
                            )
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.confirm_forget_project_identity(&id, &alias, window, cx)
                                },
                            )),
                        ),
                );
            }
        }
        if shown == 0 {
            names = names.child(empty_line(
                if filter.is_empty() {
                    "No names yet. Names replaced in this Project's documents appear here."
                } else {
                    "No names match."
                },
                p,
            ));
        }
        names = names.child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .pt_2()
                .on_action(cx.listener(|this, _: &AddProjectName, _, cx| {
                    this.add_page_name(cx);
                    cx.stop_propagation();
                }))
                .on_action(cx.listener(|this, _: &ClearProjectName, _, cx| {
                    if let Some(page) = &this.project_ui.page {
                        page.add.update(cx, |input, cx| input.reset(cx));
                    }
                    cx.stop_propagation();
                }))
                .child(ui::reveal_focus(
                    div()
                        .flex_1()
                        .border_1()
                        .border_color(p.border)
                        .rounded_sm()
                        .p_1()
                        .child(page.add.clone()),
                    page.add.read(cx).focus_handle(cx),
                    page.scroll.clone(),
                ))
                .child(
                    self.page_control("project-page-add", "Add a name", cx)
                        .on_click(cx.listener(|this, _, _, cx| this.add_page_name(cx))),
                ),
        );
        let skipped = project.skipped();
        let categories = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(section("Categories to detect"))
            .child(div().text_size(px(12.)).text_color(p.header_muted).child(
                "Scans skip categories that are off. Names you mark yourself are still replaced.",
            ))
            .child(
                div().flex().flex_wrap().gap_2().children(
                    mdoc_pii::Category::ALL
                        .into_iter()
                        .filter(|category| *category != mdoc_pii::Category::Other)
                        .map(|category| {
                            let id = id.clone();
                            let on = !skipped.contains(&category);
                            self.page_control(
                                SharedString::from(format!("page-category-{}", category.token())),
                                format!(
                                    "{} {}",
                                    if on { "✓" } else { "○" },
                                    category.detection_label()
                                ),
                                cx,
                            )
                            .role(gpui::Role::CheckBox)
                            .aria_toggled(if on {
                                gpui::Toggled::True
                            } else {
                                gpui::Toggled::False
                            })
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.toggle_project_category(&id, category, cx)
                                },
                            ))
                        }),
                ),
            );
        let lists = self.project_lists(project, cx);
        div()
            .flex()
            .flex_col()
            .gap_6()
            .child(names)
            .child(lists)
            .child(categories)
            .into_any_element()
    }

    /// The Project's Always and Never lists: what its members' manual marks
    /// and Keeps put there, each with Remove. Entries are added only by
    /// marking and keeping in a member.
    fn project_lists(&self, project: &Project, cx: &mut Context<Self>) -> gpui::Div {
        let p = self.theme.get().pdf_style();
        let mut lists = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(section("Always and never replace"))
            .child(div().text_size(px(12.)).text_color(p.header_muted).child(
                "Scans in this Project's documents propose what you marked yourself and skip \
                 what you kept.",
            ));
        // Each list's wordings, with the category of an Always one.
        type Entries = Vec<(String, Option<&'static str>)>;
        let entries: [(bool, Entries); 2] = [
            (
                true,
                project
                    .lists
                    .always
                    .iter()
                    .map(|e| (e.wording.clone(), Some(e.category().label())))
                    .collect(),
            ),
            (
                false,
                project
                    .lists
                    .never
                    .iter()
                    .map(|w| (w.clone(), None))
                    .collect(),
            ),
        ];
        for (always, entries) in entries {
            let list = if always { "always" } else { "never" };
            lists = lists.child(
                div()
                    .pt_2()
                    .text_size(px(12.))
                    .text_color(p.header_muted)
                    .child(if always {
                        "Always replace"
                    } else {
                        "Never replace"
                    }),
            );
            if entries.is_empty() {
                lists = lists.child(div().text_size(px(12.)).text_color(p.header_muted).child(
                    if always {
                        "Nothing yet. Words you mark yourself appear here."
                    } else {
                        "Nothing yet. Names you keep everywhere in a document appear here."
                    },
                ));
            }
            for (index, (wording, category)) in entries.into_iter().enumerate() {
                let control = format!("project-{list}-remove-{index}");
                let (id, remove) = (project.id.clone(), wording.clone());
                lists = lists.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .py_1()
                        .child(div().flex_1().min_w_0().truncate().child(wording.clone()))
                        .children(category.map(|label| {
                            div()
                                .text_size(px(11.))
                                .text_color(p.header_muted)
                                .child(label)
                        }))
                        .child(
                            self.page_control(SharedString::from(control), "Remove", cx)
                                .aria_label(format!("Remove “{wording}”"))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.remove_project_list_entry(&id, always, &remove, cx)
                                })),
                        ),
                );
            }
        }
        lists
    }

    pub(super) fn add_page_name(&mut self, cx: &mut Context<Self>) {
        let Some(page) = &self.project_ui.page else {
            return;
        };
        let (id, input) = (page.id.clone(), page.add.clone());
        let term = input.read(cx).value().to_string();
        self.add_project_name(&id, &term, cx);
        input.update(cx, |input, cx| input.reset(cx));
    }
}

fn section(label: &'static str) -> gpui::Div {
    div()
        .text_size(px(16.))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .child(label)
}

fn empty_line(text: &'static str, p: gpui_pdf::PdfStyle) -> gpui::Div {
    div()
        .py_2()
        .text_size(px(12.))
        .text_color(p.header_muted)
        .child(text)
}

/// A document's file family, as its row shows it.
pub(super) fn kind_label(kind: SourceKind) -> &'static str {
    match kind {
        SourceKind::Pdf => "PDF",
        SourceKind::Word => "DOC",
        SourceKind::Spreadsheet => "XLS",
        SourceKind::Presentation => "PPT",
        SourceKind::Ebook => "BOOK",
        SourceKind::Image => "IMG",
        SourceKind::Scan => "SCAN",
        SourceKind::Markdown => "MD",
        SourceKind::Text => "TXT",
    }
}

fn kind_badge(kind: SourceKind, p: gpui_pdf::PdfStyle) -> gpui::Div {
    div()
        .flex_shrink_0()
        .w(px(40.))
        .flex()
        .justify_center()
        .px_1()
        .rounded_sm()
        .border_1()
        .border_color(p.border)
        .text_size(px(10.))
        .text_color(p.header_muted)
        .child(kind_label(kind))
}

/// "4 documents · 2 replies · 11 names · created 3 Nov".
fn project_counts(documents: usize, replies: usize, names: usize, created: u64) -> String {
    let plural = |count: usize, one: &str, many: &str| {
        format!("{count} {}", if count == 1 { one } else { many })
    };
    format!(
        "{} · {} · {} · created {}",
        plural(documents, "document", "documents"),
        plural(replies, "reply", "replies"),
        plural(names, "name", "names"),
        created_date(created)
    )
}

/// Gregorian civil date from Unix days; independent of locale and OS access.
pub(super) fn created_date(seconds: u64) -> String {
    let days = (seconds / 86400).min(i32::MAX as u64) as i64 + 719468;
    let era = days / 146097;
    let doe = days - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    format!(
        "{day} {}",
        [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"
        ][(month - 1) as usize]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_counts_line_says_each_count_in_words() {
        assert_eq!(
            project_counts(1, 0, 2, 1791763200),
            "1 document · 0 replies · 2 names · created 12 Oct"
        );
        assert_eq!(
            project_counts(3, 1, 1, 0),
            "3 documents · 1 reply · 1 name · created 1 Jan"
        );
    }
}
