//! Commands whose availability must not depend on a focused element.
use super::Workspace;
use crate::workspace::{
    AddDocumentToProject, ChangeProjectColour, CheckForUpdates, DeleteProject, ExportProject, New,
    NewProject, Open, OpenProject, ProjectCategories, Quit, RenameProject, Settings, ToggleSidebar,
    ToggleTheme, shell::PageAction, shortcuts_ui::ShowShortcuts,
};
use gpui::{App, Context, Window};

pub(crate) fn bind(cx: &mut App) {
    app_action::<Quit>(cx, |this, window, cx| this.quit(window, cx));
    app_action::<Settings>(cx, |this, window, cx| {
        this.show_settings(window, cx);
    });
    app_action::<crate::menus::About>(cx, |this, window, cx| {
        window.activate_window();
        this.settings.update(cx, |panel, cx| {
            panel.show_category(crate::settings_ui::Category::About, window, cx)
        });
    });
    // Settings at About, where the result shows, and a check now.
    app_action::<CheckForUpdates>(cx, |this, window, cx| {
        window.activate_window();
        this.settings.update(cx, |panel, cx| {
            panel.show_category(crate::settings_ui::Category::About, window, cx)
        });
        let updates = this
            .preferences
            .borrow()
            .snapshot()
            .unwrap_or_default()
            .updates;
        crate::updater::spawn_check(updates.include_prereleases, false, cx);
    });
    app_action::<New>(cx, |this, window, cx| {
        if this.ready && !this.quit_flow.quitting() && !this.intake_refused(window, cx) {
            this.new_tab(window, cx);
        }
    });
    app_action::<OpenProject>(cx, |this, window, cx| this.show_project_list(window, cx));
    app_action::<NewProject>(cx, |this, window, cx| this.ask_new_project(window, cx));
    // The Project menu: the Project shown, or the active document's.
    app_action::<AddDocumentToProject>(cx, |this, window, cx| {
        this.run_page_action(PageAction::AddDocument, window, cx)
    });
    app_action::<RenameProject>(cx, |this, window, cx| {
        this.run_page_action(PageAction::RenameProject, window, cx)
    });
    app_action::<ChangeProjectColour>(cx, |this, window, cx| {
        this.run_page_action(PageAction::ProjectColour, window, cx)
    });
    app_action::<ProjectCategories>(cx, |this, window, cx| {
        this.run_page_action(PageAction::ProjectCategories, window, cx)
    });
    app_action::<ExportProject>(cx, |this, window, cx| {
        this.run_page_action(PageAction::ExportProject, window, cx)
    });
    app_action::<DeleteProject>(cx, |this, window, cx| {
        this.run_page_action(PageAction::DeleteProject, window, cx)
    });
    // ⌘/ and Help › Keyboard Shortcuts from a comparison window too.
    app_action::<ShowShortcuts>(cx, |this, window, cx| {
        window.activate_window();
        this.toggle_shortcuts(window, cx);
    });
    app_action::<Open>(cx, |this, window, cx| {
        if this.intake_refused(window, cx) {
            return;
        }
        if let Some(view) = this.active_view() {
            view.update(cx, |view, cx| view.open(&Open, window, cx));
        }
    });
    app_action::<ToggleTheme>(cx, |this, window, cx| this.cycle_theme(window, cx));
    app_action_with::<crate::menus::ShowLiveView>(cx, |this, show, window, cx| {
        this.show_live_view(show.0, window, cx)
    });
    app_action::<ToggleSidebar>(cx, |this, window, cx| {
        let effect = this.sidebar.toggle(std::time::Instant::now());
        this.carry_out_sidebar(Some(effect), window, cx);
        // Keep keyboard focus on the toggle that replaces the clicked one.
        let visible = this.sidebar.visible();
        if visible && this.rail_toggle_focus.is_focused(window) {
            window.focus(&this.panel_toggle_focus, cx);
        } else if !visible && this.panel_toggle_focus.is_focused(window) {
            window.focus(&this.rail_toggle_focus, cx);
            this.sidebar.escape_to(this.rail_toggle_focus.clone());
        }
        cx.notify();
    });
}

fn app_action<A: gpui::Action + Clone>(
    cx: &mut App,
    handle: fn(&mut Workspace, &mut Window, &mut Context<Workspace>),
) {
    app_action_with::<A>(cx, move |workspace, _, window, cx| {
        handle(workspace, window, cx)
    });
}

/// [`app_action`], for an action that carries what it acts on.
fn app_action_with<A: gpui::Action + Clone>(
    cx: &mut App,
    handle: impl Fn(&mut Workspace, &A, &mut Window, &mut Context<Workspace>) + Copy + 'static,
) {
    cx.on_action(move |action: &A, cx| {
        let action = action.clone();
        // mdoc has one workspace plus optional comparison windows. Always
        // route app commands through that workspace and its normal lifecycle.
        if let Some(workspace) = cx
            .windows()
            .into_iter()
            .find_map(|w| w.downcast::<Workspace>())
        {
            // Keyboard dispatch may already have borrowed the workspace window.
            cx.defer(move |cx| {
                let _ = workspace.update(cx, |workspace, window, cx| {
                    // Confirmations have their own dispatch root. App routing
                    // must not move focus out of that modal surface.
                    if !window.has_active_prompt() {
                        handle(workspace, &action, window, cx);
                    }
                });
            });
        }
    });
}
