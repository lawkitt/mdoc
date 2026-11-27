//! The window's view: its render, with the content card beside the sidebar.
use super::{
    Back, Close, Forward, Import, PasteAiResponse, PseudonymizeClipboard, RAIL_WIDTH,
    RestoreClipboard, RetryDocument, SwitchDocument, SwitchDocumentBack, ToggleNotices, Workspace,
    activate_for_drop, home, notices, palette, text, window_title,
};
use crate::review::PiiApplyAll;
use crate::ui::{self, style};
use gpui::{AnyElement, Context, Window, div, prelude::*, px};

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Collapsed by a narrowing window, the list's focus moves to the
        // rail's toggle, as a collapse by the toggle does.
        if self.sidebar.settle(window.viewport_size().width / px(1.))
            && self.focus_regions.sidebar().contains_focused(window, cx)
        {
            window.focus(&self.rail_toggle_focus, cx);
            self.sidebar.escape_to(self.rail_toggle_focus.clone());
        }
        // The move menu goes with the document it was opened for.
        if let Some(tab) = self.move_menu
            && (tab != self.tabs.active() || self.showing_overview(cx))
        {
            self.document_menu = None;
            self.move_menu = None;
        }
        // A finished edge drag saves the width it left.
        if !cx.has_active_drag() && self.sidebar.resizing() {
            self.end_sidebar_resize(cx);
        }
        self.note_region_focus(window, cx);
        self.reveal_for_keyboard(window, cx);
        self.end_reveal_slide_later(cx);
        // A finished or abandoned OS drag leaves no insertion line or reveal.
        if !cx.has_active_drag() && self.sidebar.dragging() {
            self.clear_file_drag(window, cx);
        }
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let home = self.showing_overview(cx);
        let title = self
            .active_view()
            .filter(|_| !home)
            .map(|view| view.read(cx).window_title())
            .unwrap_or_else(|| "mdoc".into());
        let title = window_title(title, mdoc_store::app_paths::dev_profile().is_some());
        if title != self.window_title {
            window.set_window_title(&title);
            self.window_title = title;
        }
        // Window lists the live views: the menu bar is set again only when
        // they change, and not while this window is borrowed.
        let views = self.live_view_items(cx);
        if views != self.live_view_menu {
            self.live_view_menu = views.clone();
            cx.defer(move |cx| crate::commands::menus::set_live_views(&views, cx));
        }
        if !cx.has_active_drag() && self.shell.drop.is_some() {
            self.shell.drop = None;
        }
        // Notices wait behind the header's bell; a new one opens it, and so
        // does a locked or unavailable Vault. A Vault sealed with another key
        // waits behind it: Home asks for its recovery key.
        let (locked, needed) = self.vault_notice();
        self.notices.set_recovery_nudge(self.recovery_nudge());
        let (notices, effect) = self.notices.count(locked, needed);
        if let Some(effect) = effect {
            self.carry_out_notices(effect);
        }
        self.title.notices.set(notices);
        // A crash report says what the window showed (no text, no names).
        crate::app::crash::note_state(text::crash_state(
            (!home).then(|| {
                self.active_view()
                    .map(|v| v.read(cx).crash_state(cx))
                    .unwrap_or_default()
            }),
            self.tabs.len(),
            self.sidebar.choice().unwrap_or(true),
            theme,
            self.shell.palette.is_some(),
        ));
        // Collapsed, headers join the title row: they leave room for the
        // traffic lights and the expand toggle that sit in it.
        self.title.detached.set(!self.sidebar.visible());
        self.title.inset.set(if self.sidebar.visible() {
            0.
        } else {
            (style::TRAFFIC_LIGHT_INSET + style::WORKSPACE_NAV_WIDTH + 12. - RAIL_WIDTH - 16.)
                .max(0.)
        });
        div()
            .track_focus(&self.focus)
            .key_context("Workspace")
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.sidebar_bg())
            .child(self.navigation_mouse(cx))
            .on_action(cx.listener(|this, _: &Back, window, cx| {
                this.navigate(true, window, cx);
            }))
            .on_action(cx.listener(|this, _: &Forward, window, cx| {
                this.navigate(false, window, cx);
            }))
            .on_action(cx.listener(|this, _: &ui::NextRegion, window, cx| {
                this.cycle_region(false, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ui::PreviousRegion, window, cx| {
                this.cycle_region(true, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ui::FocusEditor, window, cx| {
                this.escape_to_editor(window, cx)
            }))
            .on_action(cx.listener(|this, _: &notices::CloseNotices, window, cx| {
                if this.title.notices_open.get() {
                    this.close_notices(window, cx);
                } else {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &ui::SidebarPrevious, window, cx| {
                this.tree_step(-1, window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &ui::SidebarNext, window, cx| this.tree_step(1, window, cx)),
            )
            .on_action(cx.listener(|this, _: &ui::SidebarDelete, window, cx| {
                this.tree_delete_focused(window, cx)
            }))
            .on_action(cx.listener(|this, _: &ui::SidebarCollapse, window, cx| {
                this.tree_fold(false, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ui::SidebarExpand, window, cx| {
                this.tree_fold(true, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ui::SidebarExtendUp, window, cx| {
                this.tree_extend(-1, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ui::SidebarExtendDown, window, cx| {
                this.tree_extend(1, window, cx)
            }))
            .text_color(palette.header_fg)
            .on_action(cx.listener(|this, _: &palette::OpenPalette, window, cx| {
                this.open_palette(window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &home::ShowHome, window, cx| this.show_home(window, cx)),
            )
            .on_action(cx.listener(
                |this, _: &palette::shortcuts_ui::ShowShortcuts, window, cx| {
                    this.toggle_shortcuts(window, cx)
                },
            ))
            .on_action(cx.listener(|this, _: &home::PasteAsNewTab, window, cx| {
                this.paste_as_new_tab(window, cx)
            }))
            .on_action(cx.listener(|this, _: &ToggleNotices, _, cx| {
                this.title.notices_open.set(!this.title.notices_open.get());
                cx.notify();
            }))
            .on_action(cx.listener(|this, action: &PiiApplyAll, window, cx| {
                if let Some(view) = this.active_view().filter(|_| !this.showing_overview(cx)) {
                    view.update(cx, |view, cx| view.apply_all_pii(action, window, cx));
                }
            }))
            .on_action(cx.listener(|this, action: &mdoc_editor::Undo, window, cx| {
                if let Some(view) = this.history_document(window, cx) {
                    view.update(cx, |view, cx| {
                        view.editor
                            .update(cx, |editor, cx| editor.undo(action, window, cx));
                    });
                }
            }))
            .on_action(cx.listener(|this, action: &mdoc_editor::Redo, window, cx| {
                if let Some(view) = this.history_document(window, cx) {
                    view.update(cx, |view, cx| {
                        view.editor
                            .update(cx, |editor, cx| editor.redo(action, window, cx));
                    });
                }
            }))
            // Home's clipboard actions need a document; documents handle them first.
            .on_action(cx.listener(|this, action: &PasteAiResponse, window, cx| {
                if let Some(view) = this.clipboard_document(text::PASTE_AI_REPLY, window, cx) {
                    view.update(cx, |view, cx| view.paste_ai_response(action, window, cx));
                }
            }))
            .on_action(
                cx.listener(|this, action: &PseudonymizeClipboard, window, cx| {
                    if let Some(view) =
                        this.clipboard_document(text::PSEUDONYMIZE_CLIPBOARD, window, cx)
                    {
                        view.update(cx, |view, cx| {
                            view.pseudonymize_clipboard(action, window, cx)
                        });
                    }
                }),
            )
            .on_action(cx.listener(|this, action: &RestoreClipboard, window, cx| {
                if let Some(view) = this.clipboard_document(text::RESTORE_CLIPBOARD, window, cx) {
                    view.update(cx, |view, cx| view.restore_clipboard(action, window, cx));
                }
            }))
            .on_drag_move(cx.listener(
                |this, event: &gpui::DragMoveEvent<gpui::ExternalPaths>, _, cx| {
                    this.home_drag(event, cx)
                },
            ))
            // Nothing is open or closed: Close returns to Home, and the view
            // stays live for its undo and scroll.
            .on_action(cx.listener(|this, _: &Close, window, cx| {
                if !this.quit_flow.quitting() && !this.showing_home(cx) {
                    this.show_home(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Import, window, cx| {
                if let Some(view) = this.active_view() {
                    view.update(cx, |view, cx| view.import(&Import, window, cx));
                }
            }))
            .on_action(cx.listener(|this, _: &SwitchDocument, window, cx| {
                this.switch_document(false, window, cx)
            }))
            .on_action(cx.listener(|this, _: &SwitchDocumentBack, window, cx| {
                this.switch_document(true, window, cx)
            }))
            // Esc closes Notices first, wherever focus is: a drop's notice
            // opens while the document keeps focus.
            .capture_action(cx.listener(|this, _: &mdoc_editor::Dismiss, window, cx| {
                if this.title.notices_open.get() {
                    this.close_notices(window, cx);
                    cx.stop_propagation();
                }
            }))
            .on_action(cx.listener(|this, _: &ui::NextControl, window, cx| {
                this.tab_from_window(false, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ui::PreviousControl, window, cx| {
                this.tab_from_window(true, window, cx)
            }))
            .on_action(cx.listener(|this, _: &RetryDocument, window, cx| {
                if let Some(tab) = this.tabs.get(this.tabs.active())
                    && let Some(document) = tab.record().document()
                {
                    this.load_document(tab.id(), document, tab.record().clone(), window, cx);
                }
            }))
            .when_some(self.document_menu.clone(), |view, (id, position)| {
                view.child(
                    gpui::deferred(
                        gpui::anchored().position(position).snap_to_window().child(
                            ui::panel("document-context-menu", self.theme.get())
                                .when(cfg!(test), |v| {
                                    v.debug_selector(|| "document-context-menu".into())
                                })
                                .key_context("UiPanel UiMenu")
                                .track_focus(&self.menu_focus)
                                .tab_group()
                                .tab_stop(false)
                                .on_action(cx.listener(|this, _: &ui::CloseMenu, window, cx| {
                                    this.close_document_menu(window, cx)
                                }))
                                .on_action(cx.listener(|this, _: &ui::NextControl, window, cx| {
                                    ui::cycle(window, cx, Some(&this.menu_focus), false);
                                    cx.stop_propagation();
                                }))
                                .on_action(cx.listener(
                                    |this, _: &ui::PreviousControl, window, cx| {
                                        ui::cycle(window, cx, Some(&this.menu_focus), true);
                                        cx.stop_propagation();
                                    },
                                ))
                                .p_1()
                                .on_mouse_down_out(cx.listener(
                                    |this, _: &gpui::MouseDownEvent, _, cx| {
                                        this.document_menu = None;
                                        this.move_menu = None;
                                        cx.notify();
                                    },
                                ))
                                .children(self.document_menu_items(&id, cx)),
                        ),
                    )
                    // Above the sidebar reveal it was opened from.
                    .with_priority(1),
                )
            })
            .children(self.project_list(cx))
            .children(self.switcher_view(cx))
            .children(self.move_toast(cx))
            .children(self.ai_prompt_view(window, cx))
            .children(self.project_header_menu(cx))
            .children(self.project_ask_menu(cx))
            .children(self.folder_sheet_view(window, cx))
            .children(self.new_project_view(window, cx))
            .children(self.keychain_explainer(window, cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.sidebar(cx))
                    .child(self.card(home, window, cx)),
            )
            .children(self.notices_view(window, cx))
            .children(self.palette_view(cx))
            .when(self.settings.read(cx).open, |v| {
                v.child(self.settings.clone())
            })
            .children(self.shortcuts_view(window, cx))
    }
}

impl Workspace {
    /// The inset content card on the window ground: workspace bars,
    /// then Home or the active document.
    pub(super) fn card(
        &mut self,
        home: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme.get();
        let content = if self.reply_page.is_some() {
            self.reply_page_view(window, cx)
        } else if self.project_ui.page.is_some() {
            self.project_page(window, cx)
        } else if home {
            self.home_view(window, cx)
        } else {
            match self.active_view() {
                // While the sidebar slides, the document beside it reuses its
                // last frame unless it changed itself: only a view
                // notified on its own redraws, so this holds only for the
                // slide, never as the document's steady state.
                Some(active)
                    if self.sidebar.animating(std::time::Instant::now()) && !cx.reduce_motion() =>
                {
                    active
                        .cached(gpui::StyleRefinement::default().size_full())
                        .into_any_element()
                }
                Some(active) => active.into_any_element(),
                None => div().into_any_element(),
            }
        };
        div()
            .flex_1()
            .min_w_0()
            .h_full()
            // Expanded: one margin all round. Collapsed: the view's header
            // fills the title row and it draws its own card below.
            .when(self.sidebar.visible(), |v| v.p(px(style::CARD_MARGIN)))
            .when(!self.sidebar.visible(), |v| {
                v.pr(px(style::CARD_MARGIN)).pb(px(style::CARD_MARGIN))
            })
            .child(
                div()
                    .id("content-card")
                    .size_full()
                    .flex()
                    .flex_col()
                    .when(self.sidebar.visible(), |v| ui::card_surface(v, theme))
                    .when(home && self.project_ui.page.is_none(), |v| {
                        v.on_drop(
                            cx.listener(|this, paths: &gpui::ExternalPaths, window, cx| {
                                this.shell.drop = None;
                                activate_for_drop(window, cx);
                                this.open_paths(paths.paths().to_vec(), window, cx);
                            }),
                        )
                    })
                    .child(div().flex_1().min_h_0().child(content)),
            )
            .into_any_element()
    }
}
