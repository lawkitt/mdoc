//! Transient case overview (ADR 0077). Opening uses the coordinator's cache only.
use super::{
    Workspace,
    cases_ui::{AddCaseName, CancelCaseName, ClearCaseName, ConfirmCaseName},
};
use crate::{
    markdown_search::{SearchInput, SearchInputEvent},
    style, ui,
};
use gpui::{
    AnyElement, Context, Entity, FocusHandle, Focusable, SharedString, Subscription, Window, div,
    prelude::*, px,
};
use mdoc_store::cases::{CaseColor, CaseLink, CaseSummary};
use std::{
    cell::RefCell,
    collections::HashMap,
    path::{Path, PathBuf},
};

#[derive(Default)]
pub(super) struct CaseClose {
    waiting: Option<u64>,
    queue: std::collections::VecDeque<u64>,
}
impl CaseClose {
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
    pub(super) fn cancel(&mut self) {
        *self = Self::default();
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
pub(super) struct CasePage {
    id: String,
    swatches: bool,
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

impl CasePage {
    pub(super) fn id(&self) -> &str {
        &self.id
    }

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
            ("case-page-color".into(), self.color_focus.clone()),
            ("page-category-PERSON".into(), self.category_focus.clone()),
            ("case-page-delete".into(), self.delete_focus.clone()),
        ]);
        controls
    }
    #[cfg(test)]
    pub(super) fn test_scroll_bounds(&self) -> gpui::Bounds<gpui::Pixels> {
        self.scroll.bounds()
    }
}

impl Workspace {
    pub(super) fn sidebar_case_link(&self, tab: &super::Tab, cx: &gpui::App) -> Option<CaseLink> {
        tab.view()
            .and_then(|view| view.read(cx).case.link().cloned())
            .or_else(|| {
                let id = tab.record().case.as_ref()?;
                self.case_coordinator
                    .summaries()
                    .iter()
                    .find(|case| &case.id == id)
                    .map(CaseSummary::link)
            })
    }
    pub(super) fn case_header_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (id, position) = self.cases.header_menu.as_ref()?;
        let theme = self.theme.get();
        let mut menu = ui::panel("case-header-actions", theme)
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
                this.cases.header_menu = None;
                if let Some(focus) = this.menu_return.take() {
                    window.focus(&focus, cx);
                }
                cx.notify();
            }))
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.cases.header_menu = None;
                cx.notify();
            }));
        for (key, label, place) in [
            ("case-action-rename", "Rename", Place::Rename),
            (
                "case-action-categories",
                "Categories to detect",
                Place::Categories,
            ),
            (
                "case-action-delete",
                "Delete case and forget originals…",
                Place::Delete,
            ),
        ] {
            let id = id.clone();
            menu = menu.child(
                ui::control(key, label, theme, true)
                    .when(cfg!(test), move |v| v.debug_selector(move || key.into()))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.cases.header_menu = None;
                        this.show_case_page(&id, place, window, cx);
                    })),
            );
        }
        let id = id.clone();
        menu = menu.child(
            ui::control("case-action-close-all", "Close all", theme, true)
                .when(cfg!(test), |v| {
                    v.debug_selector(|| "case-action-close-all".into())
                })
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.cases.header_menu = None;
                    let tabs: Vec<_> = this
                        .tabs
                        .iter()
                        .filter(|tab| tab.record().case.as_deref() == Some(id.as_str()))
                        .map(|tab| tab.id())
                        .collect();
                    if let Some(tab) = this.cases.closing_members.begin(tabs) {
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
    pub(super) fn confirm_forget_case_identity(
        &mut self,
        id: &str,
        alias: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (id, alias) = (id.to_string(), alias.to_string());
        let answer = window.prompt(gpui::PromptLevel::Warning, &format!("Forget identity “{alias}”?"),
            Some("Its originals are forgotten from this case and its member documents. Document text is untouched. This cannot be undone."), &["Forget Identity", "Cancel"], cx);
        cx.spawn_in(window, async move |this, cx| {
            if answer.await.ok() == Some(0) {
                let _ = this.update_in(cx, |this, window, cx| {
                    this.forget_case_identity(&id, &alias, cx);
                    if let Some(page) = this.cases.page.as_ref().filter(|page| page.id == id) {
                        window.focus(&page.add.read(cx).focus_handle(cx), cx);
                    }
                });
            }
        })
        .detach();
    }

    pub(super) fn forget_case_identity(&mut self, id: &str, alias: &str, cx: &mut Context<Self>) {
        let Some(((forgotten, exported), effect)) = self.case_coordinator.edit(id, |case| {
            let identity = case
                .identities
                .iter()
                .find(|identity| identity.names(alias))?
                .clone();
            case.forget_identity(alias)
                .then(|| (identity, case.exported()))
        }) else {
            return;
        };
        self.carry_out_cases(effect.into_iter().collect(), cx);
        for view in self.tabs.iter().filter_map(|tab| tab.view()) {
            view.update(cx, |view, cx| {
                if !view.case.is(id) {
                    return;
                }
                let aliases: Vec<_> = std::iter::once(forgotten.alias.as_str())
                    .chain(forgotten.previous.iter().map(String::as_str))
                    .collect();
                view.pii.review_mut().forget_aliases(&aliases);
                view.pii.review_mut().reserve_aliases(
                    &aliases
                        .iter()
                        .map(|alias| alias.to_string())
                        .collect::<Vec<_>>(),
                );
                view.case.set_names(exported.clone());
                view.review_changed(cx);
                cx.notify();
            });
        }
        cx.notify();
    }
    pub(super) fn show_case_page(
        &mut self,
        id: &str,
        place: Place,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self
            .case_coordinator
            .summaries()
            .iter()
            .any(|case| case.id == id)
        {
            return;
        }
        if let Some(view) = self.active_view() {
            view.update(cx, |view, _| {
                view.active = false;
                view.pii.dismiss_popup();
            });
        }
        self.cases.list_open = false;
        self.case_list_return = None;
        self.cases.renaming = None;
        let input = |cx: &mut gpui::Context<SearchInput>, placeholder, context| {
            SearchInput::new(cx)
                .with_theme(self.theme.clone())
                .with_placeholder(placeholder)
                .with_key_context(context)
        };
        let filter = cx.new(|cx| input(cx, "Filter names", "CasePageFilter"));
        let add = cx.new(|cx| input(cx, "Add a name", "CaseTerm"));
        let changes = cx.subscribe(&filter, |_, _, _: &SearchInputEvent, cx| cx.notify());
        let page = CasePage {
            id: id.into(),
            swatches: false,
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
            Place::Categories if self.case_coordinator.case(id).is_some() => {
                page.category_focus.clone()
            }
            Place::Delete if self.case_coordinator.case(id).is_some() => page.delete_focus.clone(),
            _ => page.color_focus.clone(),
        };
        self.cases.page = Some(page);
        if matches!(place, Place::Rename) && self.case_coordinator.case(id).is_some() {
            self.start_rename_case(id.into(), window, cx);
        } else {
            // Controls are mounted on the next frame; GPUI reveals a focused descendant.
            window.defer(cx, move |window, cx| window.focus(&focus, cx));
        }
        cx.notify();
    }

    pub(super) fn set_case_color(
        &mut self,
        id: &str,
        color: CaseColor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some((link, effect)) = self.case_coordinator.edit(id, |case| {
            case.color = Some(color);
            Some(CaseLink::of(case))
        }) {
            self.carry_out_cases(effect.into_iter().collect(), cx);
            for view in self.tabs.iter().filter_map(|tab| tab.view()) {
                view.update(cx, |view, cx| {
                    if view.case.is(id) {
                        view.case.attach(Some(link.clone()));
                        cx.notify();
                    }
                });
            }
            self.checkpoint(window, cx);
        }
        if let Some(page) = self.cases.page.as_mut() {
            page.swatches = false;
            window.focus(&page.color_focus, cx);
        }
        cx.notify();
    }

    pub(super) fn open_case_member(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab) = self.find_path(&path, None, cx) {
            self.activate(tab, window, cx);
        } else if path.is_file() {
            self.open_paths(vec![path], window, cx);
        }
    }

    pub(super) fn open_all_case_members(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(case) = self.case_coordinator.case(id) else {
            return;
        };
        let paths: Vec<_> = case
            .members
            .iter()
            .filter(|path| path.is_file() || self.find_path(path, None, cx).is_some())
            .cloned()
            .collect();
        for path in paths {
            self.open_case_member(path, window, cx);
        }
    }

    pub(super) fn remove_case_member(
        &mut self,
        id: &str,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab) = self.find_path(path, None, cx)
            && self
                .tabs
                .get(tab)
                .is_some_and(|tab| tab.record().case.as_deref() == Some(id))
        {
            self.remove_from_case(tab, window, cx);
            self.tabs.set_case(tab, None);
        }
        // Closed/missing members have no view; removing never opens one.
        if let Some(((), effect)) = self.case_coordinator.edit(id, |case| {
            case.members
                .contains(&path.to_path_buf())
                .then(|| case.members.retain(|member| member != path))
        }) {
            self.carry_out_cases(effect.into_iter().collect(), cx);
        }
        self.checkpoint(window, cx);
        if let Some(page) = self.cases.page.as_ref().filter(|page| page.id == id)
            && let Some(focus) = page.controls.borrow().get("case-page-open-all")
        {
            window.focus(focus, cx);
        }
        cx.notify();
    }

    pub(super) fn case_page(&mut self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let page = self.cases.page.as_ref().unwrap();
        let Some(summary) = self
            .case_coordinator
            .summaries()
            .into_iter()
            .find(|case| case.id == page.id)
        else {
            self.cases.page = None;
            self.tabs.show_home();
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
                .bg(style::case_color(summary.link().hue(), theme)),
        )
        .child("Case")
        .child(div().flex_1())
        .children(ui::notice_bell(&self.title, theme).map(|bell| ui::no_drag(div().child(bell))));
        let header = ui::title_bar(bar, (!detached).then_some(p.border), window, theme);
        let content = match self.case_coordinator.case(&page.id) {
            Some(case) => self.unlocked_case_page(case, cx),
            None => self.locked_case_page(&summary, cx),
        };
        let body = div()
            .id("case-page-scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&page.scroll)
            .child(div().w_full().flex().justify_center().p_6().child(content));
        div()
            .id("case-page")
            .when(cfg!(test), |v| v.debug_selector(|| "case-page".into()))
            .track_focus(self.focus_regions.home())
            .size_full()
            .flex()
            .flex_col()
            .child(header)
            .child(if detached {
                ui::card_surface(body.ml(px(style::CARD_MARGIN)), theme)
            } else {
                body
            })
            .into_any_element()
    }

    fn locked_case_page(&self, summary: &CaseSummary, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("case-page-locked")
            .when(cfg!(test), |v| {
                v.debug_selector(|| "case-page-locked".into())
            })
            .w_full()
            .max_w(px(680.))
            .flex()
            .flex_col()
            .gap_4()
            .child(div().text_size(px(26.)).child(summary.name.clone()))
            .child(document_count(summary.documents))
            .child(
                ui::primary_button("case-page-unlock", "Unlock…", self.theme.get(), true)
                    .track_focus(&self.cases.page.as_ref().unwrap().color_focus)
                    .when(cfg!(test), |v| {
                        v.debug_selector(|| "case-page-unlock".into())
                    })
                    .on_click(cx.listener(|this, _, window, cx| this.unlock_cases(window, cx))),
            )
            .into_any_element()
    }

    fn page_control(
        &self,
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) -> ui::Control {
        let page = self.cases.page.as_ref().unwrap();
        let id = id.into();
        let focus = match id.as_ref() {
            "case-page-color" => page.color_focus.clone(),
            "page-category-PERSON" => page.category_focus.clone(),
            "case-page-delete" => page.delete_focus.clone(),
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

    fn unlocked_case_page(
        &self,
        case: &mdoc_store::cases::Case,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let page = self.cases.page.as_ref().unwrap();
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let id = case.id.clone();
        let name = match self
            .cases
            .renaming
            .as_ref()
            .filter(|(id, _)| *id == case.id)
        {
            Some((_, input)) => div()
                .flex_1()
                .min_w_0()
                .border_1()
                .border_color(p.border)
                .rounded_sm()
                .p_1()
                .on_action(cx.listener(|this, _: &ConfirmCaseName, window, cx| {
                    this.finish_rename(true, window, cx);
                    cx.stop_propagation();
                }))
                .on_action(cx.listener(|this, _: &CancelCaseName, window, cx| {
                    this.finish_rename(false, window, cx);
                    cx.stop_propagation();
                }))
                .child(input.clone())
                .into_any_element(),
            None => self
                .page_control("case-page-rename", case.name.clone(), cx)
                .when(cfg!(test), |v| {
                    v.debug_selector(|| "case-page-rename".into())
                })
                .aria_label(format!("Rename {}", case.name))
                .flex_1()
                .min_w_0()
                .text_size(px(26.))
                .on_click(cx.listener({
                    let id = id.clone();
                    move |this, _, window, cx| this.start_rename_case(id.clone(), window, cx)
                }))
                .into_any_element(),
        };
        let color = style::case_color(CaseLink::of(case).hue(), theme);
        let heading =
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
                            self.page_control("case-page-color", "", cx)
                                .when(cfg!(test), |v| {
                                    v.debug_selector(|| "case-page-color".into())
                                })
                                .aria_label("Case colour")
                                .p_2()
                                .child(div().size(px(16.)).rounded_full().bg(color))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(page) = this.cases.page.as_mut() {
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
                            document_count(case.members.len()),
                            case.exported().len(),
                            if case.exported().len() == 1 {
                                "name"
                            } else {
                                "names"
                            },
                            created_date(case.created)
                        )),
                )
                .when(page.swatches, |v| {
                    v.child(div().flex().flex_wrap().gap_2().children(
                        CaseColor::ALL.into_iter().map(|swatch| {
                            let id = id.clone();
                            self.page_control(
                                SharedString::from(format!("case-swatch-{}", swatch.name())),
                                swatch.name(),
                                cx,
                            )
                            .when(cfg!(test), move |v| {
                                v.debug_selector(move || format!("case-swatch-{}", swatch.name()))
                            })
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .size(px(12.))
                                    .rounded_full()
                                    .bg(style::case_color(swatch.hue(), theme)),
                            )
                            .aria_selected(
                                case.color.unwrap_or_else(|| CaseColor::for_id(&case.id)) == swatch,
                            )
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.set_case_color(&id, swatch, window, cx)
                                },
                            ))
                        }),
                    ))
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
                        self.page_control("case-page-open-all", "Open all", cx)
                            .when(cfg!(test), |v| {
                                v.debug_selector(|| "case-page-open-all".into())
                            })
                            .on_click(cx.listener({
                                let id = id.clone();
                                move |this, _, window, cx| {
                                    this.open_all_case_members(&id, window, cx)
                                }
                            })),
                    ),
            )
            .children(case.members.iter().enumerate().map(|(index, path)| {
                let open = self.find_path(path, None, cx).is_some();
                let missing = !open && !path.is_file(); // metadata only; no member contents
                let title = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                let path = path.clone();
                let remove_path = path.clone();
                let id = id.clone();
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .py_1()
                    .child(if missing {
                        div().flex_1().min_w_0().child(title).into_any_element()
                    } else {
                        self.page_control(
                            SharedString::from(format!("case-document-{index}")),
                            title,
                            cx,
                        )
                        .when(cfg!(test), move |v| {
                            v.debug_selector(move || format!("case-document-{index}"))
                        })
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_case_member(path.clone(), window, cx)
                        }))
                        .into_any_element()
                    })
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(p.header_muted)
                            .child(if open {
                                "Open"
                            } else if missing {
                                "Missing"
                            } else {
                                "Closed"
                            }),
                    )
                    .child(
                        self.page_control(
                            SharedString::from(format!("case-remove-{index}")),
                            "Remove from case",
                            cx,
                        )
                        .when(cfg!(test), move |v| {
                            v.debug_selector(move || format!("case-remove-{index}"))
                        })
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.remove_case_member(&id, &remove_path, window, cx)
                            },
                        )),
                    )
            }));
        let filter = if case.exported().len() > 10 {
            page.filter.read(cx).value().to_lowercase()
        } else {
            String::new()
        };
        let mut names = div().flex().flex_col().gap_2().child(section("Names"));
        if case.exported().len() > 10 {
            names = names.child(ui::reveal_focus(
                div()
                    .id("case-page-filter")
                    .when(cfg!(test), |v| {
                        v.debug_selector(|| "case-page-filter".into())
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
        for category in mdoc_pii::Category::ALL {
            let identities: Vec<_> = case
                .identities
                .iter()
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
                let counts = case
                    .members
                    .iter()
                    .map(|path| {
                        let count = self
                            .find_path(path, None, cx)
                            .and_then(|tab| self.tabs.get(tab)?.view())
                            .map(|view| {
                                let review = view.read(cx).pii.review();
                                review
                                    .identity_by_alias(&identity.alias)
                                    .map_or(0, |id| review.identity_count(id))
                            });
                        format!(
                            "{}: {}",
                            path.file_name().unwrap_or_default().to_string_lossy(),
                            count.map_or_else(|| "—".into(), |count| count.to_string())
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" · ");
                names = names.child(
                    div()
                        .id(SharedString::from(format!("case-identity-{alias}")))
                        .when(cfg!(test), {
                            let alias = alias.clone();
                            move |v| v.debug_selector(move || format!("case-identity-{alias}"))
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
                                        "{} {} · {}",
                                        identity.originals.len(),
                                        if identity.originals.len() == 1 {
                                            "variant"
                                        } else {
                                            "variants"
                                        },
                                        counts
                                    ),
                                )),
                        )
                        .child(
                            self.page_control(
                                SharedString::from(format!("case-forget-{alias}")),
                                "Forget identity…",
                                cx,
                            )
                            .when(cfg!(test), {
                                let alias = alias.clone();
                                move |v| v.debug_selector(move || format!("case-forget-{alias}"))
                            })
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.confirm_forget_case_identity(&id, &alias, window, cx)
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
                .on_action(cx.listener(|this, _: &AddCaseName, _, cx| {
                    this.add_page_name(cx);
                    cx.stop_propagation();
                }))
                .on_action(cx.listener(|this, _: &ClearCaseName, _, cx| {
                    if let Some(page) = &this.cases.page {
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
                    self.page_control("case-page-add", "Add a name", cx)
                        .when(cfg!(test), |v| v.debug_selector(|| "case-page-add".into()))
                        .on_click(cx.listener(|this, _, _, cx| this.add_page_name(cx))),
                ),
        );
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
                        .filter(|category| {
                            !matches!(
                                category,
                                mdoc_pii::Category::Date | mdoc_pii::Category::Other
                            )
                        })
                        .map(|category| {
                            let id = id.clone();
                            let on = !case.skipped().contains(&category);
                            self.page_control(
                                SharedString::from(format!("page-category-{}", category.token())),
                                format!("{} {}", if on { "✓" } else { "○" }, category.label()),
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
                                move |this, _, _, cx| this.toggle_case_category(&id, category, cx),
                            ))
                        }),
                ),
            );
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
            .child(
                self.page_control("case-page-delete", "Delete case and forget originals…", cx)
                    .text_color(style::markdown_style(theme).popover_danger)
                    .when(cfg!(test), |v| {
                        v.debug_selector(|| "case-page-delete".into())
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.delete_case(id.clone(), window, cx)
                    })),
            )
            .into_any_element()
    }

    pub(super) fn add_page_name(&mut self, cx: &mut Context<Self>) {
        let Some(page) = &self.cases.page else {
            return;
        };
        let (id, input) = (page.id.clone(), page.add.clone());
        let term = input.read(cx).value().to_string();
        self.add_case_name(&id, &term, cx);
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
