//! The folder sheet in the window (ADR 0075): a modal dialog listing a
//! dropped folder's files as checkboxes, with Open as case (or Add to “X”),
//! Open files and Cancel. Open files never touches the case key; the case
//! actions read it only after the click.
use super::{
    Close, Import, New, NextTab, Open, OpenCase, OpenPalette, PasteAiResponse, PreviousTab,
    PseudonymizeClipboard, RetryDocument, Settings, ToggleSidebar, Workspace,
    folder_sheet::{CAP, CAP_NOTE, DroppedFolder, Pending, Sheet, Status, Target},
    shortcuts_ui::ShowShortcuts,
};
use crate::{style, ui};
use gpui::{
    AnyElement, ClickEvent, Context, FocusHandle, KeyDownEvent, ScrollHandle, Window, div,
    prelude::*, px,
};
use mdoc_store::cases::{Case, CaseError};
use std::path::{Path, PathBuf};

/// The sheet's focus: its group, one handle per row, and where focus goes
/// back when it closes. View state only; the sheet itself is `FolderSheets`.
pub(super) struct SheetFocus {
    pub(super) group: FocusHandle,
    rows: Vec<FocusHandle>,
    scroll: ScrollHandle,
    return_focus: Option<FocusHandle>,
}

impl Workspace {
    /// A dropped folder: a notice when it has nothing to open, else a sheet
    /// once the ones before it are answered.
    pub(super) fn queue_folder(
        &mut self,
        pending: Pending,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if pending.folder.files.is_empty() {
            self.notices.post(pending.folder.empty_notice());
            cx.notify();
            return;
        }
        self.folder_sheets.push(pending);
        self.show_next_folder(window, cx);
    }

    /// Show the next waiting folder's sheet, if none shows.
    fn show_next_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(pending) = self.folder_sheets.next() else {
            return;
        };
        let statuses = pending
            .folder
            .files
            .iter()
            .map(|path| self.folder_file_status(path, cx))
            .collect();
        let target = self.folder_target(&pending.folder, pending.case.as_deref(), cx);
        let sheet = Sheet::new(&pending.folder, statuses, target, pending.at);
        let rows = sheet
            .rows()
            .iter()
            .map(|_| cx.focus_handle().tab_stop(true))
            .collect::<Vec<_>>();
        let return_focus = window.focused(cx);
        if let Some(first) = rows.first() {
            window.focus(first, cx);
        }
        self.folder_sheets.show(sheet);
        self.folder_sheet_focus = Some(SheetFocus {
            group: cx.focus_handle(),
            rows,
            scroll: ScrollHandle::new(),
            return_focus,
        });
        cx.notify();
    }

    /// The tab a file already has: its Markdown, or the source it converts.
    pub(super) fn open_tab_for(&self, path: &Path, cx: &gpui::App) -> Option<u64> {
        self.find_path(path, None, cx).or_else(|| {
            self.tabs
                .iter()
                .find(|tab| {
                    let source = tab
                        .view()
                        .and_then(|view| {
                            let view = view.read(cx);
                            view.session
                                .source
                                .as_ref()
                                .or(view.preview.attachment())
                                .or(view.preview.source())
                        })
                        .or(tab.record().attachment.as_ref());
                    source.map(PathBuf::as_path) == Some(path)
                })
                .map(|tab| tab.id())
        })
    }

    fn folder_file_status(&self, path: &Path, cx: &gpui::App) -> Status {
        let Some(tab) = self.open_tab_for(path, cx).and_then(|id| self.tabs.get(id)) else {
            return Status::New;
        };
        if let Some(link) = tab.view().and_then(|v| v.read(cx).case.link().cloned()) {
            return Status::InCase(Some(link.name));
        }
        match &tab.record().case {
            // A member of a locked case, or of one not loaded yet.
            Some(id) => Status::InCase(self.case_coordinator.case(id).map(|c| c.name.clone())),
            None => Status::Open,
        }
    }

    /// The loaded case the folder belongs to: the case header it was dropped
    /// on, else the one holding most of its files. Locked cases are not
    /// checked, so a new case is offered.
    fn folder_target(
        &self,
        folder: &DroppedFolder,
        header: Option<&str>,
        cx: &gpui::App,
    ) -> Option<Target> {
        let target = |case: &Case| Target {
            id: case.id.clone(),
            name: case.name.clone(),
        };
        if let Some(case) = header.and_then(|id| self.case_coordinator.case(id)) {
            return Some(target(case));
        }
        let inside = |path: &Path| path.parent() == Some(folder.path.as_path());
        let mut best: Option<(&Case, usize)> = None;
        for case in self.case_coordinator.cases() {
            let mut count = case.members.iter().filter(|m| inside(m)).count();
            count += self
                .tabs
                .iter()
                .filter(|tab| {
                    tab.view().is_some_and(|v| {
                        let v = v.read(cx);
                        v.case.link().is_some_and(|l| l.id == case.id)
                            && [v.session.document.path.as_ref(), v.session.source.as_ref()]
                                .into_iter()
                                .flatten()
                                .any(|p| inside(p))
                    })
                })
                .count();
            if count > 0 && best.is_none_or(|(_, most)| count > most) {
                best = Some((case, count));
            }
        }
        best.map(|(case, _)| target(case))
    }

    /// Hide the sheet and give focus back where it was.
    fn close_folder_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<Sheet> {
        let sheet = self.folder_sheets.take()?;
        if let Some(focus) = self.folder_sheet_focus.take() {
            match focus.return_focus {
                Some(focus) => window.focus(&focus, cx),
                None => self.focus_active(window, cx),
            }
        }
        cx.notify();
        Some(sheet)
    }

    /// Cancel: nothing opens; the next folder's sheet shows.
    pub(super) fn cancel_folder_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.folder_sheets.waiting() {
            return;
        }
        self.close_folder_sheet(window, cx);
        self.show_next_folder(window, cx);
    }

    /// Open files: the ticked files open without a case. The case key is
    /// never read.
    pub(super) fn open_folder_files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.folder_sheets.waiting()
            || self
                .folder_sheets
                .sheet()
                .is_none_or(|s| s.ticked_count() == 0)
        {
            return;
        }
        if let Some(sheet) = self.close_folder_sheet(window, cx) {
            self.open_ticked(&sheet, sheet.at(), window, cx);
        }
        self.show_next_folder(window, cx);
    }

    fn open_ticked(
        &mut self,
        sheet: &Sheet,
        at: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Vec<u64>, Vec<u64>) {
        // The cap holds against the tabs open now, not when the sheet showed.
        let mut new = 0;
        let paths = sheet
            .ticked()
            .into_iter()
            .filter(|path| {
                self.open_tab_for(path, cx).is_some() || {
                    new += 1;
                    new <= CAP
                }
            })
            .map(|path| (path, true))
            .collect();
        self.finish_open(paths, at, window, cx)
    }

    /// Open as case / Add to “X”: read the key now, after the Keychain
    /// explainer when reading might ask (ADR 0043). Not now returns to the
    /// sheet.
    pub(super) fn folder_primary(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.folder_sheets.waiting()
            || self
                .folder_sheets
                .sheet()
                .is_none_or(|s| s.ticked_count() == 0)
        {
            return;
        }
        let id = self.folder_sheets.sheet().map_or(0, Sheet::id);
        if !self.case_key_ready(
            move |this, window, cx| this.folder_case_now(id, window, cx),
            cx,
        ) {
            self.folder_sheets.set_waiting(true);
            return;
        }
        self.folder_case_now(id, window, cx);
    }

    /// The key is readable: open the ticked files into the case. Without it
    /// they open without a case, and a notice says so.
    fn folder_case_now(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        // Only for the sheet that asked: after a declined dialog its files
        // opened without a case, and Try again must not answer the next.
        if self.folder_sheets.sheet().is_none_or(|s| s.id() != id) {
            return;
        }
        if let Err(error) = self.case_store() {
            self.folder_without_case(&error, window, cx);
            return;
        }
        let Some(sheet) = self.close_folder_sheet(window, cx) else {
            return;
        };
        self.resume_locked(window, cx);
        let target = sheet
            .target()
            .filter(|t| self.case_coordinator.case(&t.id).is_some());
        let (case_id, at) = match target {
            Some(target) => (target.id.clone(), None),
            None => {
                let case = self.new_case(sheet.name().to_string(), &[], None);
                let id = case.id.clone();
                let effect = self.case_coordinator.create(case);
                self.carry_out_cases(effect.into_iter().collect(), cx);
                (id, sheet.at())
            }
        };
        let (ids, created) = self.open_ticked(&sheet, at, window, cx);
        if target.is_some() {
            // New members sit after the case's others in the sidebar.
            let last = self.tabs.iter().rposition(|tab| {
                !created.contains(&tab.id())
                    && (tab.record().case.as_deref() == Some(case_id.as_str())
                        || tab.view().is_some_and(|v| {
                            v.read(cx).case.link().is_some_and(|l| l.id == case_id)
                        }))
            });
            if let Some(last) = last {
                let anchor = self
                    .tabs
                    .iter()
                    .skip(last + 1)
                    .find(|tab| !created.contains(&tab.id()))
                    .map(|tab| tab.id());
                self.tabs.place(&created, anchor);
            }
        }
        for id in ids {
            let Some(tab) = self.tabs.get(id) else {
                continue;
            };
            // A tab moves between cases only through Move to case.
            let linked = tab.view().is_some_and(|v| v.read(cx).case.link().is_some());
            if linked || tab.record().case.is_some() {
                continue;
            }
            let ready = !created.contains(&id)
                && tab.view().is_some_and(|v| v.read(cx).can_copy_markdown());
            if ready {
                self.join_case(id, &case_id, window, cx);
            } else {
                // Joins when its Markdown loads or conversion finishes.
                self.tabs.set_case(id, Some(case_id.clone()));
                self.attach_case(id, window, cx);
            }
        }
        self.checkpoint(window, cx);
        cx.notify();
        self.show_next_folder(window, cx);
    }

    /// The key could not be had: the ticked files open without a case.
    fn folder_without_case(
        &mut self,
        error: &CaseError,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(sheet) = self.close_folder_sheet(window, cx) else {
            return;
        };
        self.open_ticked(&sheet, sheet.at(), window, cx);
        self.notices
            .post(format!("“{}” opened without a case: {error}", sheet.name()));
        cx.notify();
        self.show_next_folder(window, cx);
    }

    /// The explainer's Continue failed: a sheet waiting on it opens its
    /// files without a case. True if one did.
    pub(super) fn folder_key_failed(
        &mut self,
        error: &CaseError,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let waiting = self.folder_sheets.waiting();
        if waiting {
            self.folder_without_case(error, window, cx);
        }
        waiting
    }

    /// The explainer's Not now: back to the sheet.
    pub(super) fn folder_key_cancelled(&mut self) {
        self.folder_sheets.set_waiting(false);
    }

    pub(super) fn toggle_folder_row(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(sheet) = self.folder_sheets.sheet_mut() {
            sheet.toggle(index);
        }
        self.focus_folder_row(index, window, cx);
    }

    fn focus_folder_row(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(focus) = &self.folder_sheet_focus
            && let Some(row) = focus.rows.get(index)
        {
            window.focus(row, cx);
            focus.scroll.scroll_to_item(index);
        }
        cx.notify();
    }

    /// The focused row, if focus is on one.
    pub(super) fn focused_folder_row(&self, window: &Window) -> Option<usize> {
        self.folder_sheet_focus
            .as_ref()?
            .rows
            .iter()
            .position(|row| row.is_focused(window))
    }

    /// ↑ / ↓ between rows, Space ticks, Enter is the primary button, Esc
    /// cancels. A focused button takes Enter and Space itself.
    fn folder_sheet_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let keystroke = &event.keystroke;
        if keystroke.modifiers.modified() {
            return;
        }
        let row = self.focused_folder_row(window);
        let on_group = self
            .folder_sheet_focus
            .as_ref()
            .is_some_and(|f| f.group.is_focused(window));
        match keystroke.key.as_str() {
            "escape" => self.cancel_folder_sheet(window, cx),
            "enter" if row.is_some() || on_group => self.folder_primary(window, cx),
            "space" if row.is_some() => self.toggle_folder_row(row.unwrap_or(0), window, cx),
            "up" | "down" if row.is_some() => {
                let next = self.folder_sheets.sheet_mut().and_then(|sheet| {
                    sheet.set_cursor(row.unwrap_or(0));
                    sheet.step(keystroke.key == "down")
                });
                if let Some(next) = next {
                    self.focus_folder_row(next, window, cx);
                }
            }
            _ => return,
        }
        cx.stop_propagation();
    }

    pub(super) fn folder_sheet_view(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let sheet = self.folder_sheets.sheet()?;
        let focus = self.folder_sheet_focus.as_ref()?;
        // The sheet keeps focus while it is the top dialog: a document that
        // finishes loading behind it must not take the keyboard.
        if !self.cases.explainer
            && !window.has_active_prompt()
            && !focus.group.contains_focused(window, cx)
            && let Some(row) = focus.rows.get(sheet.cursor())
            && !row.is_focused(window)
        {
            window.focus(row, cx);
        }
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let (fill, mark) = theme.checkbox();
        let rows = sheet.rows().iter().enumerate().map(|(index, row)| {
            let status = row.status.label();
            let spoken = match &status {
                Some(status) => format!("{}, {status}", row.name),
                None => row.name.clone(),
            };
            ui::focus_ring(("folder-row", index), theme)
                .when(cfg!(test), |v| {
                    v.debug_selector(move || format!("folder-row-{index}"))
                })
                .role(gpui::Role::CheckBox)
                .aria_label(spoken)
                .aria_toggled(if row.ticked {
                    gpui::Toggled::True
                } else {
                    gpui::Toggled::False
                })
                .track_focus(&focus.rows[index].clone().tab_stop(index == sheet.cursor()))
                .tab_index(0)
                // One Tab stop for the list: the cursor's row.
                .tab_stop(index == sheet.cursor())
                .flex()
                .items_center()
                .gap_2()
                .min_h(px(28.))
                .px_2()
                .rounded_sm()
                .cursor_pointer()
                .hover(move |s| s.bg(p.placeholder_bg))
                .child(
                    div()
                        .flex_shrink_0()
                        .size(px(15.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(4.))
                        .border_1()
                        .border_color(if row.ticked {
                            fill
                        } else {
                            gpui::Hsla {
                                a: 0.7,
                                ..p.header_muted
                            }
                        })
                        .when(row.ticked, |v| {
                            v.bg(fill).child(
                                gpui::svg()
                                    .data(ui::Icon::Check.data())
                                    .size(px(11.))
                                    .text_color(mark),
                            )
                        }),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .child(row.name.clone()),
                )
                .children(status.map(|status| {
                    div()
                        .flex_shrink_0()
                        .text_size(px(12.))
                        .text_color(p.header_muted)
                        .child(status)
                }))
                .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                    // Space ticks through the sheet's own keys; Enter is
                    // the primary button.
                    if matches!(event, ClickEvent::Keyboard(_)) {
                        return;
                    }
                    this.toggle_folder_row(index, window, cx);
                }))
        });
        let ticked = sheet.ticked_count();
        let total = sheet.rows().len();
        let count = format!("{ticked} of {total} selected");
        let (note, refused) = if sheet.refused() {
            (format!("{count}. {CAP_NOTE}."), true)
        } else if sheet.over_cap() {
            (format!("{count}. {CAP_NOTE}."), false)
        } else {
            (count, false)
        };
        let enabled = ticked > 0;
        let primary = sheet.primary_label();
        let list_height = (window.viewport_size().height - px(300.)).clamp(px(120.), px(360.));
        Some(
            gpui::deferred(
                div()
                    .id("folder-sheet-overlay")
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_start()
                    .justify_center()
                    .pt(px(64.))
                    .bg(theme.scrim())
                    // A modal: nothing behind it takes the pointer or a drop.
                    .occlude()
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_down(gpui::MouseButton::Right, |_, _, cx| cx.stop_propagation())
                    .on_mouse_down(gpui::MouseButton::Middle, |_, _, cx| cx.stop_propagation())
                    .child(
                        ui::panel("folder-sheet", theme)
                            .when(cfg!(test), |v| v.debug_selector(|| "folder-sheet".into()))
                            .role(gpui::Role::Dialog)
                            .aria_label(format!("Open folder “{}”", sheet.name()))
                            // Window commands wait until the sheet is answered.
                            .on_action(|_: &Close, _, _| {})
                            .on_action(|_: &New, _, _| {})
                            .on_action(|_: &Open, _, _| {})
                            .on_action(|_: &Import, _, _| {})
                            .on_action(|_: &OpenCase, _, _| {})
                            .on_action(|_: &Settings, _, _| {})
                            .on_action(|_: &OpenPalette, _, _| {})
                            .on_action(|_: &ShowShortcuts, _, _| {})
                            .on_action(|_: &crate::menus::About, _, _| {})
                            .on_action(|_: &NextTab, _, _| {})
                            .on_action(|_: &PreviousTab, _, _| {})
                            .on_action(|_: &ToggleSidebar, _, _| {})
                            .on_action(|_: &RetryDocument, _, _| {})
                            .on_action(|_: &super::shell::ShowHome, _, _| {})
                            .on_action(|_: &super::shell::PasteAsNewTab, _, _| {})
                            .on_action(|_: &PasteAiResponse, _, _| {})
                            .on_action(|_: &PseudonymizeClipboard, _, _| {})
                            .on_action(|_: &super::RestoreClipboard, _, _| {})
                            .on_action(|_: &super::OpenRecent, _, _| {})
                            .on_action(|_: &super::CheckForUpdates, _, _| {})
                            .on_action(|_: &crate::pii::ui::PiiApplyAll, _, _| {})
                            .key_context("FolderSheet UiPanel")
                            .track_focus(&focus.group)
                            .tab_group()
                            .tab_stop(false)
                            .on_action(cx.listener(|this, _: &ui::NextControl, window, cx| {
                                if let Some(f) = &this.folder_sheet_focus {
                                    ui::cycle(window, cx, Some(&f.group), false);
                                }
                                cx.stop_propagation();
                            }))
                            .on_action(cx.listener(|this, _: &ui::PreviousControl, window, cx| {
                                if let Some(f) = &this.folder_sheet_focus {
                                    ui::cycle(window, cx, Some(&f.group), true);
                                }
                                cx.stop_propagation();
                            }))
                            .on_key_down(cx.listener(Self::folder_sheet_key))
                            .w(px(480.))
                            .max_w_full()
                            .p_5()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .child(ui::card_title(format!("“{}”", sheet.name())))
                            .children(sheet.skipped().map(|s| ui::card_text(s.to_string(), theme)))
                            .child(
                                div()
                                    .id("folder-sheet-list")
                                    .role(gpui::Role::List)
                                    .aria_label("Files")
                                    .track_scroll(&focus.scroll)
                                    .overflow_y_scroll()
                                    .max_h(list_height)
                                    .flex()
                                    .flex_col()
                                    .gap_px()
                                    // Room for the focus ring's outside stroke.
                                    .p_1()
                                    .children(rows),
                            )
                            .child(
                                ui::live_text("folder-sheet-note", note.clone())
                                    .when(cfg!(test), |v| {
                                        v.debug_selector(|| "folder-sheet-note".into())
                                    })
                                    .text_size(px(12.))
                                    .text_color(if refused {
                                        style::markdown_style(theme).alert_caution
                                    } else {
                                        p.header_muted
                                    })
                                    .child(note),
                            )
                            .child(
                                div()
                                    .flex()
                                    .justify_end()
                                    .gap_2()
                                    .pt_1()
                                    .child(
                                        ui::action_control(
                                            "folder-cancel",
                                            "Cancel",
                                            theme,
                                            true,
                                            false,
                                        )
                                        .when(cfg!(test), |v| {
                                            v.debug_selector(|| "folder-cancel".into())
                                        })
                                        .on_click(
                                            cx.listener(|this, _, window, cx| {
                                                this.cancel_folder_sheet(window, cx)
                                            }),
                                        ),
                                    )
                                    .child(
                                        ui::action_control(
                                            "folder-open-files",
                                            "Open files",
                                            theme,
                                            enabled,
                                            false,
                                        )
                                        .when(cfg!(test), |v| {
                                            v.debug_selector(|| "folder-open-files".into())
                                        })
                                        .on_click(
                                            cx.listener(|this, _, window, cx| {
                                                this.open_folder_files(window, cx)
                                            }),
                                        ),
                                    )
                                    .child(
                                        ui::primary_button(
                                            "folder-primary",
                                            primary,
                                            theme,
                                            enabled,
                                        )
                                        .when(cfg!(test), |v| {
                                            v.debug_selector(|| "folder-primary".into())
                                        })
                                        .on_click(
                                            cx.listener(|this, _, window, cx| {
                                                this.folder_primary(window, cx)
                                            }),
                                        ),
                                    ),
                            ),
                    ),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }
}
