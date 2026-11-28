//! The folder sheet's view: a modal dialog listing a dropped folder's
//! files as checkboxes, with Open as New Project (or Add to “X”), Open
//! files and Cancel.

use crate::workspace::projects::text;
use crate::workspace::{
    Close, Import, New, Open, OpenPalette, OpenProject, PasteAiResponse, PseudonymizeClipboard,
    RetryDocument, Settings, SwitchDocument, SwitchDocumentBack, ToggleSidebar, Workspace,
    palette::shortcuts_ui::ShowShortcuts, projects::folder_sheet::CAP_NOTE,
};
use crate::{ui, ui::style};
use gpui::{AnyElement, ClickEvent, Context, Window, div, prelude::*, px};

impl Workspace {
    pub(in crate::workspace) fn folder_sheet_view(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let sheet = self.project_ui.folder_sheets.sheet()?;
        let focus = self.project_ui.folder_sheet_focus.as_ref()?;
        // The sheet keeps focus while it is the top dialog: a document that
        // finishes loading behind it must not take the keyboard.
        if !self.project_ui.explainer
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
                Some(status) => text::row_label(&row.name, status),
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
        let count = text::selected(ticked, total);
        let (note, refused) = if sheet.refused() {
            (text::with_cap_note(&count, CAP_NOTE), true)
        } else if sheet.over_cap() {
            (text::with_cap_note(&count, CAP_NOTE), false)
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
                            .aria_label(text::open_folder(sheet.name()))
                            // Window commands wait until the sheet is answered.
                            .on_action(|_: &Close, _, _| {})
                            .on_action(|_: &New, _, _| {})
                            .on_action(|_: &Open, _, _| {})
                            .on_action(|_: &Import, _, _| {})
                            .on_action(|_: &OpenProject, _, _| {})
                            .on_action(|_: &crate::workspace::NewProject, _, _| {})
                            .on_action(|_: &Settings, _, _| {})
                            .on_action(|_: &OpenPalette, _, _| {})
                            .on_action(|_: &ShowShortcuts, _, _| {})
                            .on_action(|_: &crate::commands::menus::About, _, _| {})
                            .on_action(|_: &SwitchDocument, _, _| {})
                            .on_action(|_: &SwitchDocumentBack, _, _| {})
                            .on_action(|_: &ToggleSidebar, _, _| {})
                            .on_action(|_: &RetryDocument, _, _| {})
                            .on_action(|_: &crate::workspace::home::ShowHome, _, _| {})
                            .on_action(|_: &crate::workspace::home::PasteAsNewTab, _, _| {})
                            .on_action(|_: &PasteAiResponse, _, _| {})
                            .on_action(|_: &PseudonymizeClipboard, _, _| {})
                            .on_action(|_: &crate::workspace::RestoreClipboard, _, _| {})
                            .on_action(|_: &crate::workspace::CheckForUpdates, _, _| {})
                            .on_action(|_: &crate::review::PiiApplyAll, _, _| {})
                            .key_context("FolderSheet UiPanel")
                            .track_focus(&focus.group)
                            .tab_group()
                            .tab_stop(false)
                            .on_action(cx.listener(|this, _: &ui::NextControl, window, cx| {
                                if let Some(f) = &this.project_ui.folder_sheet_focus {
                                    ui::cycle(window, cx, Some(&f.group), false);
                                }
                                cx.stop_propagation();
                            }))
                            .on_action(cx.listener(|this, _: &ui::PreviousControl, window, cx| {
                                if let Some(f) = &this.project_ui.folder_sheet_focus {
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
                            .child(ui::card_title(text::quoted_name(sheet.name())))
                            .children(sheet.skipped().map(|s| ui::card_text(s.to_string(), theme)))
                            .child(
                                div()
                                    .id("folder-sheet-list")
                                    .role(gpui::Role::List)
                                    .aria_label(text::FILES)
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
                                            crate::ui::text::CANCEL,
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
                                            text::OPEN_FILES,
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
