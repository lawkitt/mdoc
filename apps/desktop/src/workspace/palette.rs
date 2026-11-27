//! The command palette, with its search over the Vault and the shown page's
//! actions, and the keyboard shortcuts sheet it opens.
use super::Workspace;
use super::home;
use super::home::ShowHome;
use crate::commands::shortcuts;
use crate::document::CopyMarkdown;
use crate::document::ExportMarkdown;
use crate::document::OpenSourceReply;
use crate::document::Save;
use crate::document::ShowNotes;
use crate::document::ToggleCompare;
use crate::document::markdown_search::SearchInput;
use crate::document::markdown_search::SearchInputEvent;
use crate::review::CopyForAi;
use crate::review::Pseudonymize;
use crate::ui;
use crate::ui::style;
use crate::workspace::Close;
use crate::workspace::New;
use crate::workspace::Open;
use crate::workspace::OpenProject;
use crate::workspace::PasteAiResponse;
use crate::workspace::PseudonymizeClipboard;
use crate::workspace::Settings;
use crate::workspace::ToggleSidebar;
use crate::workspace::palette::shortcuts_ui::ShowShortcuts;
use gpui::AnyElement;
use gpui::App;
use gpui::Context;
use gpui::Entity;
use gpui::Focusable;
use gpui::Subscription;
use gpui::Window;
use gpui::actions;
use gpui::div;
use gpui::prelude::*;
use gpui::px;
use mdoc_store::document_id::VaultId;
use std::collections::HashMap;
pub(crate) mod shortcuts_ui;
mod text;

actions!(
    mdoc_shell,
    [
        OpenPalette,
        PaletteUp,
        PalettePrevious,
        PaletteDown,
        PaletteConfirm,
        PaletteClose,
    ]
);

pub(crate) const PALETTE: &str = "CommandPalette";

pub(super) struct Palette {
    input: Entity<SearchInput>,
    selected: usize,
    /// The list's scroll, which keeps the selected row in view.
    scroll: gpui::ScrollHandle,
    return_focus: Option<gpui::FocusHandle>,
    _changes: Subscription,
}

/// What a palette row does when chosen.
#[derive(Clone)]
enum Command {
    NewTab,
    OpenFiles,
    PasteText,
    Pseudonymize,
    PseudonymizeClipboard,
    RestoreReply,
    CopyForAi,
    CopyMarkdown,
    Save,
    ExportMarkdown,
    Close,
    BeforeAfter,
    /// The active document's What you did; no key of its own.
    DecisionHistory,
    /// The active document's Notes tab.
    Notes,
    /// The AI reply the active document was promoted from.
    SourceReply,
    ExpandIdentities,
    CollapseIdentities,
    /// Stop the active tab's running conversion (#950); no key of its own.
    CancelConversion,
    ToggleSidebar,
    OpenProject,
    NewProject,
    Home,
    Settings,
    SettingsCategory(crate::settings::Category),
    Shortcuts,
    Theme,
    /// The active document's heading whose line starts at this offset.
    Heading(usize),
    Document(VaultId),
    Project(String),
    /// An action of the Project page or Reply page shown.
    Page(PageAction),
}

/// The header and ⋯ actions of the page shown, in the palette too.
#[derive(Clone, Copy)]
pub(super) enum PageAction {
    RenameProject,
    ProjectColour,
    ProjectCategories,
    AddDocument,
    ExportProject,
    DeleteProject,
    CopyReply,
    PromoteReply,
    DeleteReply,
}

impl Command {
    /// The action this command runs, when it is one: the palette shows its
    /// key from the shortcuts table.
    fn action(&self) -> Option<Box<dyn gpui::Action>> {
        Some(match self {
            Command::NewTab => Box::new(New),
            Command::OpenFiles => Box::new(Open),
            Command::Pseudonymize => Box::new(Pseudonymize),
            Command::PseudonymizeClipboard => Box::new(PseudonymizeClipboard),
            Command::RestoreReply => Box::new(PasteAiResponse),
            Command::CopyForAi => Box::new(CopyForAi),
            Command::CopyMarkdown => Box::new(CopyMarkdown),
            Command::Save => Box::new(Save),
            Command::ExportMarkdown => Box::new(ExportMarkdown),
            Command::Close => Box::new(Close),
            Command::BeforeAfter => Box::new(ToggleCompare),
            Command::ToggleSidebar => Box::new(ToggleSidebar),
            Command::OpenProject => Box::new(OpenProject),
            Command::NewProject => Box::new(super::NewProject),
            Command::Home => Box::new(ShowHome),
            Command::Settings => Box::new(Settings),
            Command::Shortcuts => Box::new(ShowShortcuts),
            Command::Notes => Box::new(ShowNotes),
            Command::SourceReply => Box::new(OpenSourceReply),
            Command::PasteText
            | Command::DecisionHistory
            | Command::ExpandIdentities
            | Command::CollapseIdentities
            | Command::CancelConversion
            | Command::Theme
            | Command::SettingsCategory(_)
            | Command::Heading(_)
            | Command::Document(_)
            | Command::Project(_)
            | Command::Page(_) => return None,
        })
    }

    /// The key that runs this command on this platform, if any.
    fn shortcut(&self) -> Option<String> {
        self.action().and_then(|action| shortcuts::label(&*action))
    }
}

struct Row {
    section: &'static str,
    label: String,
    detail: Option<String>,
    command: Command,
}

/// The key `action` runs from, as the shortcuts table writes it.
pub(super) fn key(action: &dyn gpui::Action) -> String {
    shortcuts::label(action).unwrap_or_default()
}

impl Workspace {
    // ---- Command palette ------------------------------------------------------

    pub(super) fn open_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
    pub(super) fn close_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(palette) = self.shell.palette.take() {
            match palette.return_focus {
                Some(focus) => window.focus(&focus, cx),
                None => self.focus_active(window, cx),
            }
        }
        cx.notify();
    }
    fn palette_rows(&self, cx: &App) -> Vec<Row> {
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
                label: text::NEW_DOCUMENT.into(),
                detail: None,
                command: Command::NewTab,
            },
            Row {
                section: text::ACTIONS,
                label: text::OPEN_FILES.into(),
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
                    label: text::PSEUDONYMIZE.into(),
                    detail: None,
                    command: Command::Pseudonymize,
                },
                Row {
                    section: text::ACTIONS,
                    label: text::COPY_FOR_AI.into(),
                    detail: None,
                    command: Command::CopyForAi,
                },
                Row {
                    section: text::ACTIONS,
                    label: text::COPY_MARKDOWN.into(),
                    detail: None,
                    command: Command::CopyMarkdown,
                },
                Row {
                    section: text::ACTIONS,
                    label: text::SAVE.into(),
                    detail: None,
                    command: Command::Save,
                },
                Row {
                    section: text::ACTIONS,
                    label: text::EXPORT_MARKDOWN.into(),
                    detail: None,
                    command: Command::ExportMarkdown,
                },
                Row {
                    section: text::ACTIONS,
                    label: text::SIDE_BY_SIDE.into(),
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
                    label: text::NOTES.into(),
                    detail: None,
                    command: Command::Notes,
                });
            }
            if source {
                rows.push(Row {
                    section: text::ACTIONS,
                    label: text::OPEN_SOURCE_REPLY.into(),
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
                label: crate::ui::text::CLOSE.into(),
                detail: None,
                command: Command::Close,
            }]);
        }
        rows.extend([
            Row {
                section: text::ACTIONS,
                label: text::PASTE_AI_REPLY.into(),
                detail: None,
                command: Command::RestoreReply,
            },
            Row {
                section: text::ACTIONS,
                label: text::PSEUDONYMIZE_CLIPBOARD.into(),
                detail: None,
                command: Command::PseudonymizeClipboard,
            },
            Row {
                section: text::ACTIONS,
                label: text::OPEN_A_PROJECT.into(),
                detail: None,
                command: Command::OpenProject,
            },
            Row {
                section: text::ACTIONS,
                label: text::NEW_PROJECT.into(),
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
                label: text::TOGGLE_SIDEBAR.into(),
                detail: None,
                command: Command::ToggleSidebar,
            },
            Row {
                section: text::ACTIONS,
                label: text::KEYBOARD_SHORTCUTS.into(),
                detail: None,
                command: Command::Shortcuts,
            },
            Row {
                section: text::GO_TO,
                label: text::HOME.into(),
                detail: None,
                command: Command::Home,
            },
            Row {
                section: text::GO_TO,
                label: text::SETTINGS.into(),
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
    fn vault_rows(&self, query: &str) -> Vec<Row> {
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
                        super::sidebar::tree_ui::kind_label(summary),
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
    pub(super) fn palette_shortcuts(&self, cx: &App) -> Vec<(String, String)> {
        self.palette_rows(cx)
            .into_iter()
            .filter_map(|row| Some((row.label, row.command.shortcut()?)))
            .collect()
    }
    /// The palette list's vertical scroll offset, for tests.
    #[cfg(test)]
    pub(super) fn palette_offset(&self) -> gpui::Pixels {
        self.shell
            .palette
            .as_ref()
            .map_or(px(0.), |palette| palette.scroll.offset().y)
    }
    /// The palette's labels in order, for tests.
    #[cfg(test)]
    pub(super) fn palette_labels(&self, cx: &App) -> Vec<String> {
        self.palette_rows(cx)
            .into_iter()
            .map(|row| row.label)
            .collect()
    }
    /// The palette's labels in one section, for tests.
    #[cfg(test)]
    pub(super) fn palette_section(&self, section: &str, cx: &App) -> Vec<String> {
        self.palette_rows(cx)
            .into_iter()
            .filter(|row| row.section == section)
            .map(|row| row.label)
            .collect()
    }
    pub(super) fn palette_step(&mut self, delta: isize, cx: &mut Context<Self>) {
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
    pub(super) fn palette_confirm(
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
    fn palette_run(&mut self, command: Command, window: &mut Window, cx: &mut Context<Self>) {
        self.close_palette(window, cx);
        let dispatch =
            |window: &mut Window, cx: &mut Context<Self>, action: Box<dyn gpui::Action>| {
                window.dispatch_action(action, cx)
            };
        match command {
            Command::NewTab => dispatch(window, cx, Box::new(New)),
            Command::OpenFiles => dispatch(window, cx, Box::new(Open)),
            Command::PasteText => self.paste_as_new_tab(window, cx),
            Command::Pseudonymize => dispatch(window, cx, Box::new(Pseudonymize)),
            Command::PseudonymizeClipboard => dispatch(window, cx, Box::new(PseudonymizeClipboard)),
            Command::RestoreReply => dispatch(window, cx, Box::new(PasteAiResponse)),
            Command::CopyForAi => dispatch(window, cx, Box::new(CopyForAi)),
            Command::CopyMarkdown => dispatch(window, cx, Box::new(CopyMarkdown)),
            Command::Save => dispatch(window, cx, Box::new(Save)),
            Command::ExportMarkdown => dispatch(window, cx, Box::new(ExportMarkdown)),
            Command::Close => dispatch(window, cx, Box::new(Close)),
            Command::BeforeAfter => dispatch(window, cx, Box::new(ToggleCompare)),
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
            Command::ToggleSidebar => dispatch(window, cx, Box::new(ToggleSidebar)),
            Command::OpenProject => dispatch(window, cx, Box::new(OpenProject)),
            Command::NewProject => dispatch(window, cx, Box::new(super::NewProject)),
            Command::Home => self.show_home(window, cx),
            Command::Settings => dispatch(window, cx, Box::new(Settings)),
            Command::SettingsCategory(category) => self
                .settings
                .update(cx, |panel, cx| panel.show_category(category, window, cx)),
            Command::Shortcuts => dispatch(window, cx, Box::new(ShowShortcuts)),
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
    pub(super) fn palette_view(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let palette = self.shell.palette.as_ref()?;
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let rows = self.palette_rows(cx);
        let selected = palette.selected.min(rows.len().saturating_sub(1));
        let mut list = div()
            .id("palette-list")
            .track_scroll(&palette.scroll)
            .max_h(px(470.))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .px_1()
            .pb_1();
        let mut section = "";
        for (i, row) in rows.iter().enumerate() {
            if row.section != section {
                section = row.section;
                list = list.child(
                    div()
                        .px_2()
                        .pt_2()
                        .pb_1()
                        .text_size(px(11.))
                        .text_color(p.header_muted)
                        .child(section),
                );
            }
            let icon = match &row.command {
                Command::NewTab | Command::PasteText | Command::NewProject => ui::Icon::Plus,
                Command::OpenFiles => ui::Icon::Open,
                Command::Document(_) => ui::Icon::Source,
                Command::Heading(_) => ui::Icon::ChevronRight,
                Command::Home => ui::Icon::Home,
                Command::Settings | Command::SettingsCategory(_) => ui::Icon::Settings,
                Command::Shortcuts => ui::Icon::Help,
                Command::Theme => ui::Icon::ThemeSystem,
                Command::RestoreReply => ui::Icon::Reply,
                Command::CopyForAi | Command::CopyMarkdown => ui::Icon::Copy,
                Command::Save | Command::ExportMarkdown => ui::Icon::Save,
                Command::Close | Command::CancelConversion => ui::Icon::Close,
                Command::BeforeAfter => ui::Icon::PanelRight,
                Command::DecisionHistory => ui::Icon::Clock,
                Command::Notes => ui::Icon::PanelRight,
                Command::SourceReply => ui::Icon::Reply,
                Command::ExpandIdentities => ui::Icon::ExpandAll,
                Command::CollapseIdentities => ui::Icon::CollapseAll,
                Command::ToggleSidebar => ui::Icon::Sidebar,
                Command::Pseudonymize | Command::PseudonymizeClipboard => ui::Icon::Anonymous,
                Command::Project(_) | Command::OpenProject => ui::Icon::Sidebar,
                Command::Page(PageAction::CopyReply) => ui::Icon::Copy,
                Command::Page(_) => ui::Icon::Sidebar,
            };
            list = list.child(
                div()
                    .id(("palette-row", i))
                    .when(cfg!(test), |v| {
                        v.debug_selector(move || format!("palette-row-{i}"))
                    })
                    .role(gpui::Role::MenuItem)
                    .aria_label(row.label.clone())
                    .flex()
                    .items_center()
                    .gap_2()
                    .h(px(32.))
                    .px_2()
                    .rounded_md()
                    .cursor_pointer()
                    .when(i == selected, |v| v.bg(p.placeholder_bg))
                    .hover(move |s| s.bg(p.placeholder_bg))
                    .child(
                        gpui::svg()
                            .data(icon.data())
                            .size(px(15.))
                            .text_color(p.header_muted),
                    )
                    .child(div().flex_1().min_w_0().truncate().child(row.label.clone()))
                    .when_some(row.detail.clone(), |v, d| {
                        v.child(div().text_size(px(11.)).text_color(p.header_muted).child(d))
                    })
                    .when_some(row.command.shortcut(), |v, s| {
                        v.child(div().text_size(px(11.)).text_color(p.header_muted).child(s))
                    })
                    .on_click({
                        let command = row.command.clone();
                        cx.listener(move |this, _, window, cx| {
                            this.palette_run(command.clone(), window, cx)
                        })
                    }),
            );
        }
        if rows.is_empty() {
            list = list.child(
                div()
                    .px_3()
                    .py_4()
                    .text_color(p.header_muted)
                    .child(text::NOTHING_MATCHES),
            );
        }
        Some(
            gpui::deferred(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .justify_center()
                    // The palette hugs its rows instead of stretching down.
                    .items_start()
                    .pt(px(72.))
                    .bg(gpui::hsla(0., 0., 0., 0.18))
                    .occlude()
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, window, cx| this.close_palette(window, cx)),
                    )
                    .child(
                        ui::panel("command-palette", theme)
                            .when(cfg!(test), |v| {
                                v.debug_selector(|| "command-palette".into())
                            })
                            .key_context(PALETTE)
                            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation()
                            })
                            .on_action(
                                cx.listener(|this, _: &PaletteUp, _, cx| this.palette_step(-1, cx)),
                            )
                            .on_action(
                                cx.listener(|this, _: &PaletteDown, _, cx| {
                                    this.palette_step(1, cx)
                                }),
                            )
                            .on_action(cx.listener(|this, _: &PaletteConfirm, window, cx| {
                                this.palette_confirm(None, window, cx)
                            }))
                            .on_action(cx.listener(|this, _: &PaletteClose, window, cx| {
                                this.close_palette(window, cx)
                            }))
                            .w(px(560.))
                            .flex()
                            .flex_col()
                            .rounded_xl()
                            .shadow_lg()
                            .overflow_hidden()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .px_3()
                                    .h(px(44.))
                                    .flex_shrink_0()
                                    .border_b_1()
                                    .border_color(p.border)
                                    .child(
                                        gpui::svg()
                                            .data(ui::Icon::Search.data())
                                            .size(px(15.))
                                            .text_color(p.header_muted),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .text_size(px(14.))
                                            .child(palette.input.clone()),
                                    ),
                            )
                            .child(list),
                    ),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }
}

impl Workspace {
    /// Headless renders: type `query` into the open palette.
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    pub(super) fn palette_query_for_render(&mut self, query: &str, cx: &mut Context<Self>) {
        if let Some(palette) = &self.shell.palette {
            (palette.input.clone()).update(cx, |input, cx| {
                input.set_value(query.into(), cx);
                input.move_to_end(cx);
            });
        }
        cx.notify();
    }
}

impl Workspace {
    /// The palette's rows for the page shown: its header and ⋯ actions.
    fn page_rows(&self) -> Vec<Row> {
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
            row(text::ADD_DOCUMENT_TO_PROJECT, PageAction::AddDocument),
            row(text::RENAME_PROJECT, PageAction::RenameProject),
            row(text::PROJECT_COLOUR, PageAction::ProjectColour),
            row(text::PROJECT_CATEGORIES, PageAction::ProjectCategories),
            row(text::EXPORT_PROJECT, PageAction::ExportProject),
            row(text::DELETE_PROJECT, PageAction::DeleteProject),
        ]
    }
    pub(super) fn run_page_action(
        &mut self,
        action: PageAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use super::projects::project_page::Place;
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
