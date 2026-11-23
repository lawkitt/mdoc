//! The Project page: its documents, names, categories and lists, from the
//! window's Projects and the Vault's summaries; it opens no document.
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
    AnyElement, Context, Entity, FocusHandle, Focusable, SharedString, Subscription, Window, div,
    prelude::*, px,
};
use mdoc_store::document_id::{DocumentId, VaultId};
use mdoc_vault::{Colour, Project};
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

#[derive(Clone, Copy)]
pub(super) enum Place {
    Header,
    Rename,
    Categories,
    Delete,
}

/// View state only: never serialized and cleared on any tab activation.
pub(super) struct ProjectPage {
    id: String,
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
    delete_focus: FocusHandle,
    controls: RefCell<HashMap<String, FocusHandle>>,
    _filter_changes: Subscription,
}

impl ProjectPage {
    pub(super) fn id(&self) -> &str {
        &self.id
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

    #[cfg(test)]
    pub(super) fn test_controls(&self) -> Vec<(String, FocusHandle)> {
        let mut controls: Vec<_> = self
            .controls
            .borrow()
            .iter()
            .map(|(id, focus)| (id.clone(), focus.clone()))
            .collect();
        controls.extend([
            ("project-page-color".into(), self.color_focus.clone()),
            ("page-category-PERSON".into(), self.category_focus.clone()),
            ("project-page-delete".into(), self.delete_focus.clone()),
        ]);
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
            (
                "project-action-categories",
                "Categories to detect",
                Place::Categories,
            ),
            ("project-action-delete", "Delete Project…", Place::Delete),
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
        let id = id.clone();
        menu = menu.child(
            ui::control("project-action-close-all", "Close all", theme, true)
                .when(cfg!(test), |v| {
                    v.debug_selector(|| "project-action-close-all".into())
                })
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.project_ui.header_menu = None;
                    let tabs: Vec<_> = this
                        .tabs
                        .iter()
                        .filter(|tab| {
                            this.sidebar_project_link(tab, cx)
                                .is_some_and(|link| link.id == id)
                        })
                        .map(|tab| tab.id())
                        .collect();
                    if let Some(tab) = this.project_ui.closing_members.begin(tabs) {
                        this.close_tab(tab, window, cx);
                    }
                    cx.notify();
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
        self.project_ui.list_open = false;
        self.project_list_return = None;
        self.end_rename(true, cx);
        let input = |cx: &mut gpui::Context<SearchInput>, placeholder, context| {
            SearchInput::new(cx)
                .with_theme(self.theme.clone())
                .with_placeholder(placeholder)
                .with_key_context(context)
        };
        let filter = cx.new(|cx| input(cx, "Filter names", "ProjectPageFilter"));
        let add = cx.new(|cx| input(cx, "Add a name", "ProjectTerm"));
        let changes = cx.subscribe(&filter, |_, _, _: &SearchInputEvent, cx| cx.notify());
        let page = ProjectPage {
            id: id.into(),
            swatches: false,
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
            delete_focus: cx.focus_handle().tab_index(0).tab_stop(true),
            controls: RefCell::new(HashMap::new()),
            _filter_changes: changes,
        };
        let focus = match place {
            Place::Categories => page.category_focus.clone(),
            Place::Delete => page.delete_focus.clone(),
            _ => page.color_focus.clone(),
        };
        self.project_ui.page = Some(page);
        self.note_destination(super::navigation::Arrival::Visit, cx);
        if matches!(place, Place::Rename) {
            self.start_rename_project(id.into(), window, cx);
        } else {
            // Controls are mounted on the next frame; GPUI reveals a focused descendant.
            window.defer(cx, move |window, cx| window.focus(&focus, cx));
        }
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

    pub(super) fn open_all_project_members(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for document in self.members(id) {
            self.open_project_member(document.0, window, cx);
        }
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
        if let Some(page) = self.project_ui.page.as_ref().filter(|page| page.id == id)
            && let Some(focus) = page.controls.borrow().get("project-page-open-all")
        {
            window.focus(focus, cx);
        }
        cx.notify();
    }

    /// Project `id`'s documents, by title, from the Vault's summaries.
    pub(super) fn members(&self, id: &str) -> Vec<(VaultId, String)> {
        let Some(vault) = self.vault.vault() else {
            return Vec::new();
        };
        let mut members: Vec<_> = (vault.documents_in(Some(id)).into_iter())
            .filter_map(|summary| Some((VaultId::parse(&summary.id)?, summary.title)))
            .collect();
        members.sort_by_key(|(_, title)| title.to_lowercase());
        members
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
        div()
            .id("project-page")
            .when(cfg!(test), |v| v.debug_selector(|| "project-page".into()))
            .track_focus(self.focus_regions.home())
            .size_full()
            .flex()
            .flex_col()
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
            "project-page-delete" => page.delete_focus.clone(),
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
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let id = project.id.clone();
        let exported = project.mapping.exported();
        let members = self.members(&project.id);
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
                .when(cfg!(test), |v| {
                    v.debug_selector(|| "project-page-rename".into())
                })
                .aria_label(format!("Rename {}", project.name))
                .flex_1()
                .min_w_0()
                .text_size(px(26.))
                .on_click(cx.listener({
                    let id = id.clone();
                    move |this, _, window, cx| this.start_rename_project(id.clone(), window, cx)
                }))
                .into_any_element(),
        };
        let color = style::project_color(ProjectLink::of(project).hue(), theme);
        let heading = div()
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
                            .when(cfg!(test), |v| {
                                v.debug_selector(|| "project-page-color".into())
                            })
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
                    .child(name),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(p.header_muted)
                    .child(format!(
                        "{} · {} {} · created {}",
                        document_count(members.len()),
                        exported.len(),
                        if exported.len() == 1 { "name" } else { "names" },
                        created_date(project.created)
                    )),
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
                            .when(cfg!(test), move |v| {
                                v.debug_selector(move || format!("project-swatch-{name}"))
                            })
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
            });
        let documents = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(section("Documents"))
                    .child(div().flex_1())
                    .child(
                        self.page_control("project-page-open-all", "Open all", cx)
                            .when(cfg!(test), |v| {
                                v.debug_selector(|| "project-page-open-all".into())
                            })
                            .on_click(cx.listener({
                                let id = id.clone();
                                move |this, _, window, cx| {
                                    this.open_all_project_members(&id, window, cx)
                                }
                            })),
                    ),
            )
            .children(
                members
                    .iter()
                    .enumerate()
                    .map(|(index, (document, title))| {
                        let open = self
                            .find_document(&DocumentId::Vault(document.clone()), None, cx)
                            .is_some();
                        let (open_document, remove_document) = (document.clone(), document.clone());
                        let id = id.clone();
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .py_1()
                            .child(
                                self.page_control(
                                    SharedString::from(format!("project-document-{index}")),
                                    title.clone(),
                                    cx,
                                )
                                .when(cfg!(test), move |v| {
                                    v.debug_selector(move || format!("project-document-{index}"))
                                })
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
                                    .text_size(px(11.))
                                    .text_color(p.header_muted)
                                    .child(if open { "Open" } else { "Closed" }),
                            )
                            .child(
                                self.page_control(
                                    SharedString::from(format!("project-remove-{index}")),
                                    "Move to Unfiled",
                                    cx,
                                )
                                .when(cfg!(test), move |v| {
                                    v.debug_selector(move || format!("project-remove-{index}"))
                                })
                                .on_click(cx.listener(
                                    move |this, _, window, cx| {
                                        this.remove_project_member(
                                            &id,
                                            remove_document.clone(),
                                            window,
                                            cx,
                                        )
                                    },
                                )),
                            )
                    }),
            );
        let filter = if exported.len() > 10 {
            page.filter.read(cx).value().to_lowercase()
        } else {
            String::new()
        };
        let mut names = div().flex().flex_col().gap_2().child(section("Names"));
        if exported.len() > 10 {
            names = names.child(ui::reveal_focus(
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
        }
        // Mentions in each open member, by alias.
        let open_members: Vec<_> = (members.iter())
            .filter_map(|(document, title)| {
                let tab = self.find_document(&DocumentId::Vault(document.clone()), None, cx)?;
                Some((self.tabs.get(tab)?.view()?.clone(), title.clone()))
            })
            .collect();
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
                            .when(cfg!(test), {
                                let alias = alias.clone();
                                move |v| v.debug_selector(move || format!("project-forget-{alias}"))
                            })
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.confirm_forget_project_identity(&id, &alias, window, cx)
                                },
                            )),
                        ),
                );
            }
        }
        names = names.child(
            div()
                .flex()
                .items_center()
                .gap_2()
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
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "project-page-add".into())
                        })
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
                            .when(cfg!(test), move |v| {
                                v.debug_selector(move || {
                                    format!("page-category-{}", category.token())
                                })
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
            .w_full()
            .max_w(px(680.))
            .flex()
            .flex_col()
            .gap_6()
            .text_size(px(13.))
            .child(heading)
            .child(documents)
            .child(names)
            .child(categories)
            .child(lists)
            .child(
                self.page_control("project-page-delete", "Delete Project…", cx)
                    .text_color(style::markdown_style(theme).popover_danger)
                    .when(cfg!(test), |v| {
                        v.debug_selector(|| "project-page-delete".into())
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.delete_project(id.clone(), window, cx)
                    })),
            )
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
                            self.page_control(SharedString::from(control.clone()), "Remove", cx)
                                .aria_label(format!("Remove “{wording}”"))
                                .when(cfg!(test), move |v| {
                                    v.debug_selector(move || control.clone())
                                })
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

/// Gregorian civil date from Unix days; independent of locale and OS access.
fn created_date(seconds: u64) -> String {
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

fn document_count(count: usize) -> String {
    format!(
        "{count} {}",
        if count == 1 { "document" } else { "documents" }
    )
}
