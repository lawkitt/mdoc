//! The Vault tree's view: its rows drawn from the Vault's summaries, a
//! document row's menu and the move menu's items; what a click, a drag or a key
//! on them carries out is in `actions.rs`.
use crate::workspace::{
    Workspace,
    sidebar::tree::{Row, RowKey},
};
use crate::{ui, ui::style};
use gpui::{AnyElement, App, Context, Window, div, prelude::*, px};
use mdoc_store::document_id::{DocumentId, VaultId};
use mdoc_vault::{DocumentOrigin, DocumentSummary, ProjectSummary, SourceKind};

/// A document row being dragged onto a Project or Unfiled.
#[derive(Clone)]
pub(in crate::workspace) struct DocumentDrag {
    pub(in crate::workspace) id: VaultId,
    title: String,
}

impl Render for DocumentDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().p_2().child(self.title.clone())
    }
}

/// The tree's rows and what they are drawn from.
pub(in crate::workspace) struct TreeNow {
    pub(in crate::workspace) projects: Vec<ProjectSummary>,
    pub(in crate::workspace) documents: Vec<DocumentSummary>,
    pub(in crate::workspace) rows: Vec<Row>,
}

/// The short kind label a document row shows.
pub(in crate::workspace) fn kind_label(document: &DocumentSummary) -> &'static str {
    match document.source_kind {
        SourceKind::Pdf => "PDF",
        SourceKind::Word => "DOC",
        SourceKind::Spreadsheet => "XLS",
        SourceKind::Presentation => "PPT",
        SourceKind::Ebook => "EPUB",
        SourceKind::Image => "IMG",
        SourceKind::Scan => "SCAN",
        SourceKind::Markdown => "MD",
        SourceKind::Text if document.origin == DocumentOrigin::New => "MD",
        SourceKind::Text => "TXT",
    }
}

impl Workspace {
    // ---- The keyboard --------------------------------------------------------

    /// The focus handle of row `key`, made on first use.
    pub(in crate::workspace) fn tree_focus(&self, key: &RowKey, cx: &mut App) -> gpui::FocusHandle {
        self.tree_row_focus
            .borrow_mut()
            .entry(key.clone())
            .or_insert_with(|| cx.focus_handle().tab_stop(true))
            .clone()
    }
    // ---- Drawing ----------------------------------------------------------------

    /// The tree's rows for the expanded sidebar or the rail's reveal.
    pub(in crate::workspace) fn tree_view(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        if !self.tree_has_vault() {
            return vec![
                self.section_label("Projects"),
                div()
                    .px_2()
                    .py_1()
                    .text_color(palette.header_muted)
                    .child(super::text::DOCUMENTS_ONCE_VAULT_OPENS)
                    .into_any_element(),
            ];
        }
        let now = self.tree_now();
        let shown = self.tree_shown(cx);
        let selected = self.tree.selected(&now.rows, &shown, &now.documents);
        let stop = self.tree.stop(&now.rows, selected.as_ref());
        let mut elements = Vec::with_capacity(now.rows.len());
        for row in &now.rows {
            let element = match row {
                Row::ProjectsLabel => {
                    let label = self.tree_projects_label();
                    if now.projects.is_empty() {
                        elements.push(label);
                        div()
                            .px_2()
                            .pb_1()
                            .text_size(px(12.))
                            .text_color(palette.header_muted)
                            .child(super::text::NO_PROJECTS_YET)
                            .into_any_element()
                    } else {
                        label
                    }
                }
                Row::Project {
                    id,
                    expanded,
                    documents,
                } => {
                    let Some(summary) = now.projects.iter().find(|p| &p.id == id) else {
                        continue;
                    };
                    self.tree_project_row(summary, *expanded, *documents, &selected, &stop, cx)
                }
                Row::Document { id, project } => {
                    let Some(summary) = now.documents.iter().find(|d| &d.id == id) else {
                        continue;
                    };
                    self.tree_document_row(summary, project.is_some(), &selected, &stop, cx)
                }
                Row::UnfiledLabel => {
                    let chosen = (self.tree.selection().iter())
                        .filter(|id| {
                            now.documents
                                .iter()
                                .any(|d| &d.id == *id && d.project.is_none())
                        })
                        .count();
                    self.tree_unfiled_label(chosen, cx)
                }
                Row::ShowAll { hidden } => self.tree_show_all(*hidden, &selected, &stop, cx),
            };
            elements.push(element);
        }
        elements
    }
    /// The common shape of a focusable tree row.
    fn tree_row(
        &self,
        key: &RowKey,
        element_id: String,
        label: String,
        selected: &Option<RowKey>,
        stop: &Option<RowKey>,
        cx: &mut Context<Self>,
    ) -> ui::Control {
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let focus = self
            .tree_focus(key, cx)
            .tab_stop(stop.as_ref() == Some(key));
        let is_selected = selected.as_ref() == Some(key);
        let menu_key = key.clone();
        let bounds = std::rc::Rc::new(std::cell::Cell::new(gpui::Bounds::default()));
        let drawn = bounds.clone();
        let selector = element_id.clone();
        ui::focus_ring(element_id, theme)
            .when(cfg!(test), move |v| {
                v.debug_selector(move || selector.clone())
            })
            .map(|v| ui::reveal_focus(v.track_focus(&focus), focus, self.sidebar_scroll.clone()))
            .key_context("UiControl SidebarRow")
            .tab_index(0)
            .tab_stop(stop.as_ref() == Some(key))
            .role(gpui::Role::TreeItem)
            .aria_selected(is_selected)
            .aria_label(label)
            .relative()
            .child(
                gpui::canvas(move |b, _, _| drawn.set(b), |_, _, _, _| {})
                    .absolute()
                    .inset_0(),
            )
            .on_action(cx.listener(move |this, _: &ui::SidebarMenu, window, cx| {
                this.menu_return = window.focused(cx);
                let at = bounds.get().bottom_left();
                match &menu_key {
                    RowKey::Document(id) => {
                        if let Some(id) = VaultId::parse(id) {
                            this.document_menu = Some((id, at));
                            this.move_menu = None;
                        }
                    }
                    RowKey::Project(id) => this.project_ui.header_menu = Some((id.clone(), at)),
                    RowKey::ShowAll => return,
                }
                window.focus(&this.menu_focus, cx);
                cx.notify();
            }))
            .flex()
            .items_center()
            .min_w_0()
            .gap_1()
            .px_2()
            .min_h(px(30.))
            .mb(px(1.))
            .rounded_md()
            .when(is_selected, |v| v.bg(theme.sidebar_selected()))
            .hover(move |v| v.bg(palette.placeholder_bg))
            .cursor_pointer()
    }
    fn tree_project_row(
        &self,
        project: &ProjectSummary,
        expanded: bool,
        documents: usize,
        selected: &Option<RowKey>,
        stop: &Option<RowKey>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let id = project.id.clone();
        let key = RowKey::Project(id.clone());
        let hue = project
            .colour
            .unwrap_or_else(|| mdoc_vault::Colour::for_id(&project.id))
            .hue();
        let menu_open = self.context_menu_open();
        self.tree_row(
            &key,
            format!("tree-project-{id}"),
            super::text::project_row(&project.name, documents),
            selected,
            stop,
            cx,
        )
        .aria_expanded(expanded)
        .child(
            ui::control(
                gpui::SharedString::from(format!("tree-chevron-{id}")),
                if expanded { "▾" } else { "▸" },
                theme,
                true,
            )
            .when(cfg!(test), {
                let id = id.clone();
                move |v| v.debug_selector(move || format!("tree-chevron-{id}"))
            })
            .tab_stop(false)
            .aria_label(if expanded {
                "Collapse Project"
            } else {
                "Expand Project"
            })
            .px_1()
            .py_0()
            .on_click(cx.listener({
                let id = id.clone();
                move |this, _, window, cx| {
                    cx.stop_propagation();
                    this.toggle_project(&id, window, cx);
                }
            })),
        )
        .child(
            div()
                .size(px(8.))
                .flex_shrink_0()
                .rounded_full()
                .bg(style::project_color(hue, theme)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .child(project.name.clone()),
        )
        .child(
            div()
                .text_size(px(11.))
                .text_color(palette.header_muted)
                .child(documents.to_string()),
        )
        .when(!menu_open, |v| {
            v.tooltip(style::tooltip(
                super::text::project_row_tooltip(&project.name),
                theme,
            ))
        })
        .on_click(cx.listener({
            let id = id.clone();
            move |this, _, window, cx| {
                if this.quit_flow.quitting() {
                    return;
                }
                this.tree.set_cursor(Some(RowKey::Project(id.clone())));
                this.open_project(&id, window, cx);
            }
        }))
        .on_mouse_down(
            gpui::MouseButton::Right,
            cx.listener({
                let id = id.clone();
                move |this, event: &gpui::MouseDownEvent, window, cx| {
                    this.menu_return = window.focused(cx);
                    // The row keeps no focus of its own from this press.
                    window.prevent_default();
                    window.focus(&this.menu_focus, cx);
                    this.project_ui.header_menu = Some((id.clone(), event.position));
                    cx.notify();
                }
            }),
        )
        .drag_over::<DocumentDrag>(move |v, _, _, _| v.bg(theme.sidebar_selected()))
        .on_drop(cx.listener({
            let id = id.clone();
            move |this, drag: &DocumentDrag, window, cx| {
                this.drop_document(drag.id.clone(), Some(id.clone()), window, cx)
            }
        }))
        // OS files dropped here are imported into this Project.
        .drag_over::<gpui::ExternalPaths>(move |v, _, _, _| v.bg(theme.sidebar_selected()))
        .can_drop(crate::workspace::sidebar::can_drop_files)
        .on_drop(cx.listener({
            let id = id.clone();
            move |this, paths: &gpui::ExternalPaths, window, cx| {
                cx.stop_propagation();
                this.drop_on_project(paths, &id, window, cx)
            }
        }))
        .into_any_element()
    }
    fn tree_document_row(
        &self,
        document: &DocumentSummary,
        member: bool,
        selected: &Option<RowKey>,
        stop: &Option<RowKey>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let Some(id) = VaultId::parse(&document.id) else {
            return div().into_any_element();
        };
        let key = RowKey::Document(document.id.clone());
        let live = self
            .find_document(&DocumentId::Vault(id.clone()), None, cx)
            .and_then(|tab| self.tabs.get(tab))
            .and_then(|tab| tab.view())
            .map(|view| view.read(cx));
        let running =
            live.is_some_and(|v| v.lifecycle.loading() || v.import.busy() || v.preview.loading());
        let busy = running || live.is_some_and(|v| v.preview.queued());
        let error = live.is_some_and(|v| {
            (v.error.is_some() && !v.conversion_cancelled())
                || v.preview.failed()
                || v.import.ocr_required().is_some()
                || v.import.password_prompt().is_some()
        });
        let pseudonymized = live.is_some_and(|v| v.pseudonymized());
        let recognized = live.is_some_and(|v| v.recognized());
        let chosen = !member && self.tree.is_chosen(&document.id);
        let title = document.title.clone();
        let tooltip = super::text::document_tooltip(&title, recognized, pseudonymized, busy, error);
        let menu_open = self.context_menu_open();
        let unfiled = !member;
        self.tree_row(
            &key,
            format!("tree-document-{}", document.id),
            title.clone(),
            selected,
            stop,
            cx,
        )
        .when(member, |v| v.pl(px(26.)))
        .when(chosen, |v| {
            v.bg(gpui::Hsla {
                a: 0.18,
                ..theme.search_accent()
            })
        })
        .child(
            div()
                .min_w(px(32.))
                .text_size(px(10.))
                .text_color(palette.header_muted)
                .flex_shrink_0()
                .child(kind_label(document)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(crate::workspace::sidebar::view::middle_truncated(&title)),
        )
        .when(recognized, |v| {
            v.child(crate::workspace::sidebar::view::tab_badge(
                ui::Icon::Ocr,
                palette.header_muted,
            ))
        })
        .when(pseudonymized, |v| {
            v.child(crate::workspace::sidebar::view::tab_badge(
                ui::Icon::Anonymous,
                palette.header_muted,
            ))
        })
        .when(busy || error, |v| {
            v.child(
                div()
                    .text_color(if error {
                        style::markdown_style(theme).alert_warning
                    } else {
                        palette.header_muted
                    })
                    .child(if running && !error {
                        ui::spinner(
                            gpui::SharedString::from(format!("tree-activity-{}", document.id)),
                            theme,
                        )
                    } else {
                        div()
                            .child(if error { "!" } else { "…" })
                            .into_any_element()
                    }),
            )
        })
        .when(!menu_open, |v| v.tooltip(style::tooltip(tooltip, theme)))
        .on_click(cx.listener({
            let id = id.clone();
            move |this, event: &gpui::ClickEvent, window, cx| {
                let modifiers = event.modifiers();
                if unfiled && modifiers.secondary(cx) {
                    this.tree.toggle_chosen(id.as_str());
                    cx.notify();
                    return;
                }
                if unfiled && modifiers.shift {
                    let rows = this.tree_now().rows;
                    this.tree.extend_to(&rows, id.as_str());
                    cx.notify();
                    return;
                }
                if unfiled {
                    this.tree.choose(id.as_str());
                } else {
                    this.tree.clear_selection();
                }
                this.open_from_tree(id.clone(), window, cx);
            }
        }))
        .on_mouse_down(
            gpui::MouseButton::Right,
            cx.listener({
                let id = id.clone();
                move |this, event: &gpui::MouseDownEvent, window, cx| {
                    this.menu_return = window.focused(cx);
                    // The row keeps no focus of its own from this press.
                    window.prevent_default();
                    window.focus(&this.menu_focus, cx);
                    this.document_menu = Some((id.clone(), event.position));
                    this.move_menu = None;
                    cx.notify();
                }
            }),
        )
        .on_drag(
            DocumentDrag {
                id: id.clone(),
                title,
            },
            |drag, _, _, cx| {
                cx.stop_propagation();
                cx.new(|_| drag.clone())
            },
        )
        .when(unfiled, |v| {
            v.drag_over::<DocumentDrag>(move |v, _, _, _| v.bg(theme.sidebar_selected()))
                .on_drop(cx.listener(move |this, drag: &DocumentDrag, window, cx| {
                    this.drop_document(drag.id.clone(), None, window, cx)
                }))
        })
        .into_any_element()
    }
    /// The Projects label, with New Project.
    fn tree_projects_label(&self) -> AnyElement {
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        div()
            .id("tree-projects")
            .when(cfg!(test), |v| v.debug_selector(|| "tree-projects".into()))
            .flex()
            .items_center()
            .gap_1()
            .px_2()
            .pt_3()
            .pb_1()
            .text_size(px(11.))
            .text_color(palette.header_muted)
            .child(div().flex_1().child("Projects"))
            .child(
                ui::header_icon_untipped(
                    "tree-new-project",
                    "New Project",
                    ui::Icon::Plus,
                    theme,
                    false,
                )
                .when(!self.context_menu_open(), |v| {
                    v.tooltip(style::tooltip(
                        crate::commands::shortcuts::tip(
                            "New Project",
                            &crate::workspace::NewProject,
                        ),
                        theme,
                    ))
                })
                .when(cfg!(test), |v| {
                    v.debug_selector(|| "tree-new-project".into())
                })
                .size(px(22.))
                .on_click(|_, window, cx| {
                    window.dispatch_action(Box::new(crate::workspace::NewProject), cx)
                }),
            )
            .into_any_element()
    }
    /// Unfiled's label: a drop target for documents, New, and Delete N…
    /// while several rows are chosen.
    fn tree_unfiled_label(&self, chosen: usize, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        div()
            .id("tree-unfiled")
            .when(cfg!(test), |v| v.debug_selector(|| "tree-unfiled".into()))
            .flex()
            .items_center()
            .gap_1()
            .px_2()
            .pt_3()
            .pb_1()
            .text_size(px(11.))
            .text_color(palette.header_muted)
            .child(div().flex_1().child("Unfiled"))
            .when(chosen > 1, |v| {
                v.child(
                    ui::control(
                        "tree-delete-chosen",
                        super::text::delete_chosen(chosen),
                        theme,
                        true,
                    )
                    .when(cfg!(test), |v| {
                        v.debug_selector(|| "tree-delete-chosen".into())
                    })
                    .aria_label(super::text::delete_chosen_label(chosen))
                    .px_1()
                    .py_0()
                    .text_color(style::markdown_style(theme).alert_caution)
                    .on_click(cx.listener(|this, _, window, cx| {
                        let documents = this.tree.selection().to_vec();
                        this.delete_documents(documents, window, cx);
                    })),
                )
            })
            .child(
                ui::header_icon_untipped(
                    "tree-new",
                    super::text::NEW_DOCUMENT,
                    ui::Icon::Plus,
                    theme,
                    false,
                )
                .track_focus(&self.panel_new_focus)
                .when(!self.context_menu_open(), |v| {
                    v.tooltip(style::tooltip(
                        crate::commands::shortcuts::tip(
                            super::text::NEW_DOCUMENT,
                            &crate::workspace::New,
                        ),
                        theme,
                    ))
                })
                .when(cfg!(test), |v| v.debug_selector(|| "tree-new".into()))
                .size(px(22.))
                .on_click(|_, window, cx| {
                    window.dispatch_action(Box::new(crate::workspace::New), cx)
                }),
            )
            .drag_over::<DocumentDrag>(move |v, _, _, _| v.bg(theme.sidebar_selected()))
            .on_drop(cx.listener(|this, drag: &DocumentDrag, window, cx| {
                this.drop_document(drag.id.clone(), None, window, cx)
            }))
            .into_any_element()
    }
    fn tree_show_all(
        &self,
        hidden: usize,
        selected: &Option<RowKey>,
        stop: &Option<RowKey>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let palette = self.theme.get().pdf_style();
        let label = if hidden > 0 {
            super::text::show_all(hidden)
        } else {
            super::text::SHOW_FEWER.to_string()
        };
        self.tree_row(
            &RowKey::ShowAll,
            "tree-show-all".into(),
            label.clone(),
            selected,
            stop,
            cx,
        )
        .text_color(palette.header_muted)
        .child(div().pl(px(36.)).child(label))
        .on_click(cx.listener(|this, _, _, cx| {
            this.tree.set_cursor(Some(RowKey::ShowAll));
            this.tree.toggle_show_all();
            cx.notify();
        }))
        .into_any_element()
    }
    /// Move to each other Project (scrolling past eight), Move to New
    /// Project, Move to Unfiled for a member, and Delete… on an Unfiled row
    /// of the sidebar.
    pub(in crate::workspace) fn document_menu_items(
        &self,
        document: &VaultId,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let row = |id: String, label: String| {
            let selector = id.clone();
            ui::focus_ring(gpui::SharedString::from(id), theme)
                .role(gpui::Role::MenuItem)
                .aria_label(label.clone())
                .key_context("UiControl")
                .tab_index(0)
                .when(cfg!(test), move |v| {
                    let selector = selector.clone();
                    v.debug_selector(move || selector.clone())
                })
                .px_3()
                .py_1()
                .rounded_sm()
                .cursor_pointer()
                .hover(move |v| v.bg(palette.placeholder_bg))
                .max_w(px(320.))
                .truncate()
                .child(label)
        };
        let project = self.project_of(document.as_str());
        let movable = self.document_movable(document);
        let mut items = Vec::new();
        if movable {
            let mut others = Vec::new();
            for p in
                (self.projects.summaries().into_iter()).filter(|p| project.as_ref() != Some(&p.id))
            {
                let (to, document) = (p.id.clone(), document.clone());
                // Tab reveals a row scrolled out of the list.
                let focus = (self.move_row_focus.borrow_mut())
                    .entry(p.id.clone())
                    .or_insert_with(|| cx.focus_handle().tab_stop(true))
                    .clone();
                others.push(
                    row(
                        format!("context-move-{}", p.id),
                        super::text::move_to(&p.name),
                    )
                    .map(|v| {
                        ui::reveal_focus(
                            v.track_focus(&focus),
                            focus,
                            self.move_menu_scroll.clone(),
                        )
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.close_document_menu(window, cx);
                        this.drop_document(document.clone(), Some(to.clone()), window, cx);
                    }))
                    .into_any_element(),
                );
            }
            if !others.is_empty() {
                // Eight rows show; the rest scroll.
                items.push(
                    div()
                        .id("context-move-projects")
                        .track_scroll(&self.move_menu_scroll)
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "context-move-projects".into())
                        })
                        .flex()
                        .flex_col()
                        .max_h(px(8. * 26.))
                        .overflow_y_scroll()
                        .children(others)
                        .into_any_element(),
                );
            }
            let new = document.clone();
            items.push(
                row(
                    "context-new-project".into(),
                    super::text::MOVE_TO_NEW_PROJECT.into(),
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.close_document_menu(window, cx);
                    this.move_document_to_new_project(new.clone(), window, cx);
                }))
                .into_any_element(),
            );
            if project.is_some() {
                let leave = document.clone();
                items.push(
                    row(
                        "context-leave-project".into(),
                        super::text::MOVE_TO_UNFILED.into(),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.close_document_menu(window, cx);
                        this.drop_document(leave.clone(), None, window, cx);
                    }))
                    .into_any_element(),
                );
            }
        }
        if project.is_none() && !self.moving.contains(document) && self.move_menu.is_none() {
            let chosen = self.tree.to_delete(document.as_str());
            let label = super::text::delete_documents(chosen.len());
            if !items.is_empty() {
                items.push(ui::menu_divider(theme).into_any_element());
            }
            items.push(
                row("context-delete-document".into(), label)
                    .text_color(style::markdown_style(theme).alert_caution)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.close_document_menu(window, cx);
                        this.delete_documents(chosen.clone(), window, cx);
                    }))
                    .into_any_element(),
            );
        }
        items
    }
}
