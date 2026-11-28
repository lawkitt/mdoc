//! The menu bar. On macOS GPUI makes the first menu the application menu, so
//! it is the app menu (About, Check for Updates…, Settings, Services, Hide,
//! Quit), followed by File, Edit, View, Window and Help. Edit items use the
//! editor's actions with their OS actions, so they also reach native
//! Open/Save panels. The Window menu also lists the live views by title;
//! choosing one shows it (#1160). The workspace hands over the list as it
//! changes, and the menu bar is set again. Nothing is ticked: GPUI can
//! only rebuild the whole bar, and its macOS platform keeps every
//! rebuild's actions, so switching documents must not rebuild it. Each
//! item is a command of the command table, by its menu bar name.

use crate::commands::{
    table::{self, Command},
    text,
};
use gpui::{App, Menu, MenuItem, SharedString, SystemMenuType, actions};

/// What Help › mdoc Help and the sidebar's ? open.
pub(crate) const HELP_URL: &str = "https://github.com/lawkitt/mdoc#readme";

/// Window › a live view's title: show live view `.0`.
#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = mdoc, no_json)]
pub(crate) struct ShowLiveView(pub u64);

/// One live view as the Window menu lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LiveViewItem {
    pub(crate) id: u64,
    pub(crate) title: SharedString,
}

actions!(
    mdoc,
    [
        About,
        HideApp,
        HideOtherApps,
        ShowAllApps,
        MinimizeWindow,
        ZoomWindow,
        OpenHelp
    ]
);

/// Install the menu bar and the app-level actions only it needs.
pub(crate) fn install(cx: &mut App) {
    cx.on_action(|_: &HideApp, cx| cx.hide());
    cx.on_action(|_: &HideOtherApps, cx| cx.hide_other_apps());
    cx.on_action(|_: &ShowAllApps, cx| cx.unhide_other_apps());
    cx.on_action(|_: &MinimizeWindow, cx| {
        if let Some(window) = cx.active_window() {
            let _ = window.update(cx, |_, window, _| window.minimize_window());
        }
    });
    cx.on_action(|_: &ZoomWindow, cx| {
        if let Some(window) = cx.active_window() {
            let _ = window.update(cx, |_, window, _| window.zoom_window());
        }
    });
    cx.on_action(|_: &OpenHelp, cx| cx.open_url(HELP_URL));
    // macOS only: the table has no Windows keys for these.
    let bindings =
        crate::commands::shortcuts::bindings(crate::commands::shortcuts::Owner::Menus, cx);
    cx.bind_keys(bindings);
    cx.set_menus(menus(&[]));
}

/// The live views changed: the menu bar is set again with `views` under
/// Window.
pub(crate) fn set_live_views(views: &[LiveViewItem], cx: &mut App) {
    cx.set_menus(menus(views));
}

/// Every action item, with its title, in menu order (tests).
#[cfg(test)]
pub(crate) fn items() -> Vec<(gpui::SharedString, Box<dyn gpui::Action>)> {
    items_with(&[])
}

/// [`items`], with `views` listed under Window (tests).
#[cfg(test)]
pub(crate) fn items_with(
    views: &[LiveViewItem],
) -> Vec<(gpui::SharedString, Box<dyn gpui::Action>)> {
    fn walk(items: Vec<MenuItem>, out: &mut Vec<(gpui::SharedString, Box<dyn gpui::Action>)>) {
        for item in items {
            match item {
                MenuItem::Action { name, action, .. } => out.push((name, action)),
                MenuItem::Submenu(menu) => walk(menu.items, out),
                MenuItem::Separator | MenuItem::SystemMenu(_) => {}
            }
        }
    }
    let mut out = Vec::new();
    for menu in menus(views) {
        walk(menu.items, &mut out);
    }
    out
}

/// A command as the menu bar lists it: its menu name and its action.
fn item(command: &Command) -> MenuItem {
    MenuItem::Action {
        name: command.menu.into(),
        action: command.action(),
        os_action: command.os,
        checked: false,
        disabled: false,
    }
}

/// Window's own items, then each live view.
fn window_items(views: &[LiveViewItem]) -> Vec<MenuItem> {
    let mut items = vec![
        item(&table::MINIMIZE),
        item(&table::ZOOM_WINDOW),
        MenuItem::separator(),
        item(&table::SWITCH_DOCUMENT),
        item(&table::SWITCH_DOCUMENT_BACK),
    ];
    if !views.is_empty() {
        items.push(MenuItem::separator());
    }
    items.extend(
        (views.iter()).map(|view| MenuItem::action(view.title.clone(), ShowLiveView(view.id))),
    );
    items
}

fn menus(views: &[LiveViewItem]) -> Vec<Menu> {
    let mut app = vec![item(&table::ABOUT)];
    if cfg!(target_os = "macos") {
        app.push(item(&table::CHECK_FOR_UPDATES));
    }
    app.extend([MenuItem::separator(), item(&table::SETTINGS)]);
    if cfg!(target_os = "macos") {
        app.extend([
            MenuItem::separator(),
            MenuItem::os_submenu(text::SERVICES, SystemMenuType::Services),
            MenuItem::separator(),
            item(&table::HIDE_MDOC),
            item(&table::HIDE_OTHERS),
            item(&table::SHOW_ALL),
        ]);
    }
    app.extend([MenuItem::separator(), item(&table::QUIT)]);
    let mut help = vec![item(&table::HELP), item(&table::KEYBOARD_SHORTCUTS)];
    if !cfg!(target_os = "macos") {
        help.extend([MenuItem::separator(), item(&table::CHECK_FOR_UPDATES)]);
    }
    vec![
        Menu::new(text::MENU_APP).items(app),
        Menu::new(text::MENU_FILE).items([
            item(&table::NEW),
            item(&table::OPEN),
            item(&table::OPEN_PROJECT),
            item(&table::CONVERT_TO_MARKDOWN),
            MenuItem::separator(),
            item(&table::SAVE),
            item(&table::EXPORT_MARKDOWN),
            MenuItem::separator(),
            item(&table::COPY_MARKDOWN),
            item(&table::COPY_FOR_AI),
            item(&table::PASTE_AI_REPLY),
            MenuItem::separator(),
            item(&table::CLOSE),
        ]),
        Menu::new(text::MENU_PROJECT).items([
            item(&table::NEW_PROJECT),
            item(&table::MOVE_TO_PROJECT),
            item(&table::MOVE_TO_NEW_PROJECT),
            MenuItem::separator(),
            item(&table::ADD_DOCUMENT_TO_PROJECT),
            item(&table::RENAME_PROJECT),
            item(&table::PROJECT_COLOUR),
            item(&table::PROJECT_CATEGORIES),
            MenuItem::separator(),
            item(&table::EXPORT_PROJECT),
            item(&table::DELETE_PROJECT),
        ]),
        Menu::new(text::MENU_EDIT).items([
            item(&table::UNDO),
            item(&table::REDO),
            MenuItem::separator(),
            item(&table::CUT),
            item(&table::COPY),
            item(&table::PASTE),
            item(&table::SELECT_ALL),
            MenuItem::separator(),
            item(&table::FIND),
            item(&table::FIND_AND_REPLACE),
            item(&table::FIND_NEXT),
            item(&table::FIND_PREVIOUS),
            MenuItem::separator(),
            item(&table::PSEUDONYMIZE),
            item(&table::PSEUDONYMIZE_CLIPBOARD),
            item(&table::RESTORE_CLIPBOARD),
        ]),
        Menu::new(text::MENU_VIEW).items([
            item(&table::HOME),
            item(&table::COMMAND_PALETTE),
            item(&table::TOGGLE_SIDEBAR),
            MenuItem::separator(),
            item(&table::SHOW_EDIT_VIEW),
            item(&table::SHOW_ORIGINAL_VIEW),
            item(&table::SHOW_PSEUDONYMIZED_VIEW),
            item(&table::SIDE_BY_SIDE),
            MenuItem::separator(),
            item(&table::SHOW_REPLACEMENTS),
            item(&table::NOTES),
            item(&table::OPEN_SOURCE_REPLY),
            MenuItem::separator(),
            item(&table::ZOOM_IN),
            item(&table::ZOOM_OUT),
            item(&table::ACTUAL_SIZE),
        ]),
        // GPUI registers the menu named "Window" as the app's Windows menu,
        // so macOS lists the open windows there, after the live views.
        Menu::new(text::MENU_WINDOW).items(window_items(views)),
        Menu::new(text::MENU_HELP).items(help),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(id: u64, title: &str) -> LiveViewItem {
        LiveViewItem {
            id,
            title: title.to_string().into(),
        }
    }

    /// The Window menu's rows: (title, ticked, the live view it shows).
    fn window_rows(views: &[LiveViewItem]) -> Vec<(String, bool, Option<u64>)> {
        let window = menus(views)
            .into_iter()
            .find(|menu| menu.name == "Window")
            .expect("a Window menu");
        (window.items.into_iter())
            .filter_map(|item| match item {
                MenuItem::Action {
                    name,
                    action,
                    checked,
                    ..
                } => Some((
                    name.to_string(),
                    checked,
                    (action.as_any().downcast_ref::<ShowLiveView>()).map(|show| show.0),
                )),
                _ => None,
            })
            .collect()
    }

    /// Done when (#1160): Window lists each live view by title after its
    /// own items, none ticked, each choosing its own view; with none,
    /// Window is as before.
    #[test]
    fn the_window_menu_lists_the_live_views() {
        let own = vec![
            ("Minimize".to_string(), false, None),
            ("Zoom".to_string(), false, None),
            ("Switch Document".to_string(), false, None),
            ("Switch Document Back".to_string(), false, None),
        ];
        assert_eq!(window_rows(&[]), own);
        let views = [view(3, "Lease.md"), view(7, "Letter.md")];
        let mut expected = own.clone();
        expected.extend([
            ("Lease.md".to_string(), false, Some(3)),
            ("Letter.md".to_string(), false, Some(7)),
        ]);
        assert_eq!(window_rows(&views), expected);
        // Only Window changes.
        let others = |views: &[LiveViewItem]| {
            (menus(views).into_iter())
                .filter(|menu| menu.name != "Window")
                .map(|menu| (menu.name, menu.items.len()))
                .collect::<Vec<_>>()
        };
        assert_eq!(others(&views), others(&[]));
    }
}
