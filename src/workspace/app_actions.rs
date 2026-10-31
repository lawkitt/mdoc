//! Commands whose availability must not depend on a focused element.
use super::Workspace;
use crate::workspace::{
    CheckForUpdates, New, Open, OpenCase, Quit, Settings, ToggleSidebar, ToggleTheme,
    shortcuts_ui::ShowShortcuts,
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
    // Settings at About, where the result shows, and a check now (ADR 0038).
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
        if this.ready && !this.quit_flow.quitting() {
            this.new_tab(window, cx);
        }
    });
    app_action::<OpenCase>(cx, |this, window, cx| this.show_case_list(window, cx));
    // ⌘/ and Help › Keyboard Shortcuts from a comparison window too.
    app_action::<ShowShortcuts>(cx, |this, window, cx| {
        window.activate_window();
        this.toggle_shortcuts(window, cx);
    });
    app_action::<Open>(cx, |this, window, cx| {
        if let Some(view) = this.active_view() {
            view.update(cx, |view, cx| view.open(&Open, window, cx));
        }
    });
    app_action::<ToggleTheme>(cx, |this, window, cx| this.cycle_theme(window, cx));
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

fn app_action<A: gpui::Action>(
    cx: &mut App,
    handle: fn(&mut Workspace, &mut Window, &mut Context<Workspace>),
) {
    cx.on_action(move |_: &A, cx| {
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
                    // must not move focus out of that modal surface (ADR 0062).
                    if !window.has_active_prompt() {
                        handle(workspace, window, cx);
                    }
                });
            });
        }
    });
}
