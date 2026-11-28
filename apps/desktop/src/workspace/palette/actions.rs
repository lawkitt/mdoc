//! The palette's actions: opening and closing it, its rows (the actions,
//! the page's, the headings, and what a query finds in the Vault), moving
//! the selection and running the chosen row.

use super::text;
use super::{Command, PALETTE, PageAction, Palette, Row};
use crate::commands::table;
use crate::document::markdown_search::SearchInput;
use crate::document::markdown_search::SearchInputEvent;
use crate::ui::style;
use crate::workspace::Workspace;
use crate::workspace::home;
use gpui::App;
use gpui::Context;
use gpui::Focusable;
use gpui::Window;
use gpui::prelude::*;
#[cfg(test)]
use gpui::px;
use mdoc_store::document_id::VaultId;
use std::collections::HashMap;

impl Workspace {
    pub(in crate::workspace) fn open_palette(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.shell.palette.is_some() {
            self.close_palette(window, cx);
            return;
        }
        let input = cx.new(|cx| {
            SearchInput::new(cx)
                .with_theme(self.theme.clone())
                .with_key_context(PALETTE)
                .with_placeholder(text::SEARCH_OR_RUN)
        });
        let changes = cx.subscribe(&input, |this, _, _: &SearchInputEvent, cx| {
            if let Some(palette) = &mut this.shell.palette {
                palette.selected = 0;
                palette.scroll.set_offset(gpui::Point::default());
            }
            cx.notify();
        });
        let return_focus = window.focused(cx);
        window.focus(&input.read(cx).focus_handle(cx), cx);
        self.shell.palette = Some(Palette {
            input,
            selected: 0,
            scroll: gpui::ScrollHandle::new(),
            return_focus,
            _changes: changes,
        });
        cx.notify();
    }

    pub(in crate::workspace) fn close_palette(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(palette) = self.shell.palette.take() {
            match palette.return_focus {
                Some(focus) => window.focus(&focus, cx),
                None => self.focus_active(window, cx),
            }
        }
        cx.notify();
    }

    pub(super) fn palette_rows(&self, cx: &App) -> Vec<Row> {
        let query = self
            .shell
            .palette
            .as_ref()
            .map(|p| p.input.read(cx).value().trim().to_lowercase())
            .unwrap_or_default();
        let has_document = !self.showing_overview(cx);
        let mut rows = self.page_rows();
        rows.extend(vec![
            Row {
                section: text::ACTIONS,
                label: table::NEW.label.into(),
                detail: None,
                command: Command::NewTab,
            },
            Row {
                section: text::ACTIONS,
                label: table::OPEN.label.into(),
                detail: None,
                command: Command::OpenFiles,
            },
            Row {
                section: text::ACTIONS,
                label: text::PASTE_TEXT.into(),
                detail: None,
                command: Command::PasteText,
            },
        ]);
        if has_document {
            rows.extend([
                Row {
                    section: text::ACTIONS,
                    label: table::PSEUDONYMIZE.label.into(),
                    detail: None,
                    command: Command::Pseudonymize,
                },
                Row {
                    section: text::ACTIONS,
                    label: table::COPY_FOR_AI.label.into(),
                    detail: None,
                    command: Command::CopyForAi,
                },
                Row {
                    section: text::ACTIONS,
                    label: table::COPY_MARKDOWN.label.into(),
                    detail: None,
                    command: Command::CopyMarkdown,
                },
                Row {
                    section: text::ACTIONS,
                    label: table::SAVE.label.into(),
                    detail: None,
                    command: Command::Save,
                },
                Row {
                    section: text::ACTIONS,
                    label: table::EXPORT_MARKDOWN.label.into(),
                    detail: None,
                    command: Command::ExportMarkdown,
                },
                Row {
                    section: text::ACTIONS,
                    label: table::SIDE_BY_SIDE.label.into(),
                    detail: None,
                    command: Command::BeforeAfter,
                },
                Row {
                    section: text::ACTIONS,
                    label: text::WHAT_YOU_DID.into(),
                    detail: None,
                    command: Command::DecisionHistory,
                },
            ]);
            // Notes, and the reply a promoted document came from.
            let (notes, source) = self.active_view().map_or((false, false), |view| {
                let view = view.read(cx);
                (
                    view.can_copy_markdown() && view.notes_available(),
                    view.from_reply.as_ref().is_some_and(|from| !from.gone),
                )
            });
            if notes {
                rows.push(Row {
                    section: text::ACTIONS,
                    label: table::NOTES.label.into(),
                    detail: None,
                    command: Command::Notes,
                });
            }
            if source {
                rows.push(Row {
                    section: text::ACTIONS,
                    label: table::OPEN_SOURCE_REPLY.label.into(),
                    detail: None,
                    command: Command::SourceReply,
                });
            }
            let panel = self.active_view().is_some_and(|view| {
                let view = view.read(cx);
                view.replacements_open() && view.can_copy_markdown()
            });
            if panel {
                rows.extend([
                    Row {
                        section: text::ACTIONS,
                        label: text::EXPAND_IDENTITIES.into(),
                        detail: None,
                        command: Command::ExpandIdentities,
                    },
                    Row {
                        section: text::ACTIONS,
                        label: text::COLLAPSE_IDENTITIES.into(),
                        detail: None,
                        command: Command::CollapseIdentities,
                    },
                ]);
            }
            let converting = self.active_view().is_some_and(|view| {
                let import = &view.read(cx).import;
                import.busy() && !import.stopping()
            });
            if converting {
                rows.push(Row {
                    section: text::ACTIONS,
                    label: text::CANCEL_CONVERSION.into(),
                    detail: None,
                    command: Command::CancelConversion,
                });
            }
            rows.extend([Row {
                section: text::ACTIONS,
                label: table::CLOSE.label.into(),
                detail: None,
                command: Command::Close,
            }]);
        }
        rows.extend([
            Row {
                section: text::ACTIONS,
                label: table::PASTE_AI_REPLY.label.into(),
                detail: None,
                command: Command::RestoreReply,
            },
            Row {
                section: text::ACTIONS,
                label: table::PSEUDONYMIZE_CLIPBOARD.label.into(),
                detail: None,
                command: Command::PseudonymizeClipboard,
            },
            Row {
                section: text::ACTIONS,
                label: table::OPEN_PROJECT.label.into(),
                detail: None,
                command: Command::OpenProject,
            },
            Row {
                section: text::ACTIONS,
                label: table::NEW_PROJECT.label.into(),
                detail: None,
                command: Command::NewProject,
            },
            Row {
                section: text::ACTIONS,
                label: style::theme_switch_label(self.theme_choice.next()).into(),
                detail: None,
                command: Command::Theme,
            },
            Row {
                section: text::ACTIONS,
                label: table::TOGGLE_SIDEBAR.label.into(),
                detail: None,
                command: Command::ToggleSidebar,
            },
            Row {
                section: text::ACTIONS,
                label: table::KEYBOARD_SHORTCUTS.label.into(),
                detail: None,
                command: Command::Shortcuts,
            },
            Row {
                section: text::GO_TO,
                label: table::HOME.label.into(),
                detail: None,
                command: Command::Home,
            },
            Row {
                section: text::GO_TO,
                label: table::SETTINGS.label.into(),
                detail: None,
                command: Command::Settings,
            },
        ]);
        rows.extend(
            crate::settings::Category::ALL
                .into_iter()
                .map(|category| Row {
                    section: text::GO_TO,
                    label: text::settings_category(category.label()),
                    detail: None,
                    command: Command::SettingsCategory(category),
                }),
        );
        // The active document's headings (from MiaoYan's table of contents).
        if let Some(view) = self.active_view().filter(|_| has_document) {
            let view = view.read(cx);
            if !view.lifecycle.source_only()
                && !view.lifecycle.loading()
                && !view.lifecycle.unavailable()
            {
                for heading in mdoc_editor::headings(view.editor.read(cx).text()) {
                    rows.push(Row {
                        section: text::GO_TO_HEADING,
                        label: heading.title,
                        detail: Some(text::heading_level(heading.level)),
                        command: Command::Heading(heading.offset),
                    });
                }
            }
        }
        if query.is_empty() {
            return rows;
        }
        // What the query finds in the Vault comes first, then the actions.
        let mut found = self.vault_rows(&query);
        found.extend(rows.into_iter().filter(|row| {
            let label = row.label.to_lowercase();
            query.split_whitespace().all(|word| label.contains(word))
        }));
        found
    }

    /// The palette's Documents and Projects for `query`: titles and Project
    /// names from the manifest.
    pub(super) fn vault_rows(&self, query: &str) -> Vec<Row> {
        let Some(vault) = self.vault.vault() else {
            return Vec::new();
        };
        let projects = vault.projects();
        let documents = vault.documents();
        let found = home::search(query, &projects, &documents);
        let names: HashMap<&str, &str> = (projects.iter())
            .map(|project| (project.id.as_str(), project.name.as_str()))
            .collect();
        let mut rows: Vec<Row> = (found.documents.iter())
            .filter_map(|summary| {
                let place = (summary.project.as_deref())
                    .and_then(|id| names.get(id).copied())
                    .unwrap_or(text::UNFILED);
                Some(Row {
                    section: text::DOCUMENTS,
                    label: summary.title.clone(),
                    detail: Some(text::document_detail(
                        place,
                        crate::workspace::sidebar::tree_ui::kind_label(summary),
                    )),
                    command: Command::Document(VaultId::parse(&summary.id)?),
                })
            })
            .collect();
        rows.extend(found.projects.into_iter().map(|project| {
            let count = (documents.iter())
                .filter(|summary| summary.project.as_deref() == Some(project.id.as_str()))
                .count();
            Row {
                section: text::PROJECTS,
                label: project.name,
                detail: Some(text::documents(count)),
                command: Command::Project(project.id),
            }
        }));
        rows
    }

    /// Each palette row with a key: its label and the key, for tests.
    #[cfg(test)]
    pub(in crate::workspace) fn palette_shortcuts(&self, cx: &App) -> Vec<(String, String)> {
        self.palette_rows(cx)
            .into_iter()
            .filter_map(|row| Some((row.label, row.command.shortcut()?)))
            .collect()
    }

    /// The palette list's vertical scroll offset, for tests.
    #[cfg(test)]
    pub(in crate::workspace) fn palette_offset(&self) -> gpui::Pixels {
        self.shell
            .palette
            .as_ref()
            .map_or(px(0.), |palette| palette.scroll.offset().y)
    }

    /// The palette's labels in order, for tests.
    #[cfg(test)]
    pub(in crate::workspace) fn palette_labels(&self, cx: &App) -> Vec<String> {
        self.palette_rows(cx)
            .into_iter()
            .map(|row| row.label)
            .collect()
    }

    /// The palette's labels in one section, for tests.
    #[cfg(test)]
    pub(in crate::workspace) fn palette_section(&self, section: &str, cx: &App) -> Vec<String> {
        self.palette_rows(cx)
            .into_iter()
            .filter(|row| row.section == section)
            .map(|row| row.label)
            .collect()
    }

    pub(in crate::workspace) fn palette_step(&mut self, delta: isize, cx: &mut Context<Self>) {
        let rows = self.palette_rows(cx);
        if let Some(palette) = &mut self.shell.palette
            && !rows.is_empty()
        {
            let selected =
                (palette.selected as isize + delta).rem_euclid(rows.len() as isize) as usize;
            palette.selected = selected;
            // The list's children are its rows and a label before each
            // section.
            let labels = 1
                + (rows[..=selected].windows(2))
                    .filter(|pair| pair[0].section != pair[1].section)
                    .count();
            palette.scroll.scroll_to_item(selected + labels);
            cx.notify();
        }
    }

    pub(in crate::workspace) fn palette_confirm(
        &mut self,
        index: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let rows = self.palette_rows(cx);
        let index = index.or(self.shell.palette.as_ref().map(|p| p.selected));
        let Some(row) = index.and_then(|i| rows.into_iter().nth(i)) else {
            return;
        };
        self.palette_run(row.command, window, cx);
    }

    /// Close the palette and run `command`.
    pub(super) fn palette_run(
        &mut self,
        command: Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_palette(window, cx);
        let dispatch =
            |window: &mut Window, cx: &mut Context<Self>, action: Box<dyn gpui::Action>| {
                window.dispatch_action(action, cx)
            };
        match command {
            Command::NewTab => dispatch(window, cx, table::NEW.action()),
            Command::OpenFiles => dispatch(window, cx, table::OPEN.action()),
            Command::PasteText => self.paste_as_new_tab(window, cx),
            Command::Pseudonymize => dispatch(window, cx, table::PSEUDONYMIZE.action()),
            Command::PseudonymizeClipboard => {
                dispatch(window, cx, table::PSEUDONYMIZE_CLIPBOARD.action())
            }
            Command::RestoreReply => dispatch(window, cx, table::PASTE_AI_REPLY.action()),
            Command::CopyForAi => dispatch(window, cx, table::COPY_FOR_AI.action()),
            Command::CopyMarkdown => dispatch(window, cx, table::COPY_MARKDOWN.action()),
            Command::Save => dispatch(window, cx, table::SAVE.action()),
            Command::ExportMarkdown => dispatch(window, cx, table::EXPORT_MARKDOWN.action()),
            Command::Close => dispatch(window, cx, table::CLOSE.action()),
            Command::BeforeAfter => dispatch(window, cx, table::SIDE_BY_SIDE.action()),
            // Straight to the document: focus may have gone back to the
            // sidebar, outside the view's action handlers.
            Command::DecisionHistory => {
                if let Some(view) = self.active_view() {
                    view.update(cx, |view, cx| view.open_decision_history(window, cx));
                }
            }
            Command::Notes => {
                if let Some(view) = self.active_view() {
                    view.update(cx, |view, cx| view.show_notes(window, cx));
                }
            }
            Command::SourceReply => {
                if let Some(view) = self.active_view() {
                    view.update(cx, |view, cx| view.open_source_reply(cx));
                }
            }
            Command::CancelConversion => {
                if let Some(view) = self.active_view() {
                    view.update(cx, |view, cx| {
                        view.import.stop();
                        cx.notify();
                    });
                }
            }
            Command::ExpandIdentities | Command::CollapseIdentities => {
                let expand = matches!(command, Command::ExpandIdentities);
                if let Some(view) = self.active_view() {
                    view.update(cx, |view, cx| {
                        if view.replacements_open() {
                            view.set_all_identities_expanded(expand, cx)
                        }
                    });
                }
            }
            Command::ToggleSidebar => dispatch(window, cx, table::TOGGLE_SIDEBAR.action()),
            Command::OpenProject => dispatch(window, cx, table::OPEN_PROJECT.action()),
            Command::NewProject => dispatch(window, cx, table::NEW_PROJECT.action()),
            Command::Home => self.show_home(window, cx),
            Command::Settings => dispatch(window, cx, table::SETTINGS.action()),
            Command::SettingsCategory(category) => self
                .settings
                .update(cx, |panel, cx| panel.show_category(category, window, cx)),
            Command::Shortcuts => dispatch(window, cx, table::KEYBOARD_SHORTCUTS.action()),
            Command::Theme => self.cycle_theme(window, cx),
            Command::Heading(offset) => {
                // The Markdown editor must be the pane shown: not Before/After,
                // nor Source in a narrow window.
                if let Some(view) = self.active_view() {
                    view.update(cx, |view, cx| {
                        if view.compare.is_some() {
                            view.close_compare(window, cx);
                        }
                        view.original_selected = false;
                        view.editor
                            .update(cx, |editor, cx| editor.reveal_offset(offset, cx));
                        cx.notify();
                    });
                }
                self.focus_active(window, cx);
            }
            Command::Document(id) => self.open_vault_document(id, window, cx),
            Command::Project(id) => self.open_project(&id, window, cx),
            Command::Page(action) => self.run_page_action(action, window, cx),
        }
    }

    /// Headless renders: type `query` into the open palette.
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    pub(in crate::workspace) fn palette_query_for_render(
        &mut self,
        query: &str,
        cx: &mut Context<Self>,
    ) {
        if let Some(palette) = &self.shell.palette {
            (palette.input.clone()).update(cx, |input, cx| {
                input.set_value(query.into(), cx);
                input.move_to_end(cx);
            });
        }
        cx.notify();
    }

    /// The palette's rows for the page shown: its header and ⋯ actions.
    pub(super) fn page_rows(&self) -> Vec<Row> {
        let row = |label: &str, action| Row {
            section: text::PAGE,
            label: label.into(),
            detail: None,
            command: Command::Page(action),
        };
        if self.reply_page.is_some() {
            return vec![
                row(text::COPY_REPLY, PageAction::CopyReply),
                row(text::PROMOTE_TO_DOCUMENT, PageAction::PromoteReply),
                row(text::DELETE_REPLY, PageAction::DeleteReply),
            ];
        }
        if self.project_ui.page.is_none() {
            return Vec::new();
        }
        vec![
            row(
                table::ADD_DOCUMENT_TO_PROJECT.label,
                PageAction::AddDocument,
            ),
            row(table::RENAME_PROJECT.label, PageAction::RenameProject),
            row(table::PROJECT_COLOUR.label, PageAction::ProjectColour),
            row(
                table::PROJECT_CATEGORIES.label,
                PageAction::ProjectCategories,
            ),
            row(table::EXPORT_PROJECT.label, PageAction::ExportProject),
            row(table::DELETE_PROJECT.label, PageAction::DeleteProject),
        ]
    }

    pub(in crate::workspace) fn run_page_action(
        &mut self,
        action: PageAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use crate::workspace::projects::project_page::Place;
        let reply = self.reply_page.as_ref().map(|page| page.id().to_string());
        // The Project shown, else the active document's.
        let project = (self.project_ui.page.as_ref())
            .map(|page| page.id().to_string())
            .or_else(|| {
                let view = self.active_view().filter(|_| !self.showing_overview(cx))?;
                view.read(cx).project.link().map(|link| link.id.clone())
            });
        match (action, reply, project) {
            (PageAction::CopyReply, Some(_), _) => self.copy_saved_reply(cx),
            (PageAction::PromoteReply, Some(_), _) => self.promote_saved_reply(window, cx),
            (PageAction::DeleteReply, Some(id), _) => self.confirm_delete_reply(&id, window, cx),
            (PageAction::AddDocument, _, Some(id)) => self.add_document_to_project(id, window, cx),
            (PageAction::RenameProject, _, Some(id)) => {
                self.show_project_page(&id, Place::Rename, window, cx)
            }
            (PageAction::ProjectColour, _, Some(id)) => {
                self.show_project_page(&id, Place::Colour, window, cx)
            }
            (PageAction::ProjectCategories, _, Some(id)) => {
                self.show_project_page(&id, Place::Categories, window, cx)
            }
            (PageAction::ExportProject, _, Some(id)) => self.export_project(id, window, cx),
            (PageAction::DeleteProject, _, Some(id)) => self.delete_project(id, window, cx),
            _ => {}
        }
    }
}
