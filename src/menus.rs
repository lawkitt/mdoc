//! The menu bar. On macOS GPUI makes the first menu the application menu, so
//! it is the app menu (About, Settings, Services, Hide, Quit), followed by
//! File, Edit, View and Window. Edit items use the editor's actions with their
//! OS actions, so they also reach native Open/Save panels.

use gpui::{App, KeyBinding, Menu, MenuItem, OsAction, SystemMenuType, actions};

use crate::*;

actions!(
    mdoc,
    [
        About,
        HideApp,
        HideOtherApps,
        ShowAllApps,
        MinimizeWindow,
        ZoomWindow
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
    if cfg!(target_os = "macos") {
        cx.bind_keys([
            KeyBinding::new("cmd-h", HideApp, None),
            KeyBinding::new("cmd-alt-h", HideOtherApps, None),
            KeyBinding::new("cmd-m", MinimizeWindow, None),
        ]);
    }
    cx.set_menus(menus());
}

/// Every action item, with its title, in menu order (tests).
#[cfg(test)]
pub(crate) fn items() -> Vec<(gpui::SharedString, Box<dyn gpui::Action>)> {
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
    for menu in menus() {
        walk(menu.items, &mut out);
    }
    out
}

fn menus() -> Vec<Menu> {
    let mut app = vec![
        MenuItem::action("About mdoc", About),
        MenuItem::separator(),
        MenuItem::action("Settings…", Settings),
    ];
    if cfg!(target_os = "macos") {
        app.extend([
            MenuItem::separator(),
            MenuItem::os_submenu("Services", SystemMenuType::Services),
            MenuItem::separator(),
            MenuItem::action("Hide mdoc", HideApp),
            MenuItem::action("Hide Others", HideOtherApps),
            MenuItem::action("Show All", ShowAllApps),
        ]);
    }
    app.extend([MenuItem::separator(), MenuItem::action("Quit mdoc", Quit)]);
    vec![
        Menu::new("mdoc").items(app),
        Menu::new("File").items([
            MenuItem::action("New", New),
            MenuItem::action("Open…", Open),
            MenuItem::action("Open Case…", OpenCase),
            MenuItem::action("Convert to Markdown", Import),
            MenuItem::separator(),
            MenuItem::action("Save", Save),
            MenuItem::action("Save As…", SaveAs),
            MenuItem::separator(),
            MenuItem::action("Copy Markdown", CopyMarkdown),
            MenuItem::action("Copy for AI", CopyForAi),
            MenuItem::action("Paste AI Response", PasteAiResponse),
            MenuItem::separator(),
            MenuItem::action("Close Tab", Close),
        ]),
        Menu::new("Edit").items([
            MenuItem::os_action("Undo", mdoc_editor::Undo, OsAction::Undo),
            MenuItem::os_action("Redo", mdoc_editor::Redo, OsAction::Redo),
            MenuItem::separator(),
            MenuItem::os_action("Cut", mdoc_editor::Cut, OsAction::Cut),
            MenuItem::os_action("Copy", mdoc_editor::Copy, OsAction::Copy),
            MenuItem::os_action("Paste", mdoc_editor::Paste, OsAction::Paste),
            MenuItem::os_action("Select All", mdoc_editor::SelectAll, OsAction::SelectAll),
            MenuItem::separator(),
            MenuItem::action("Find…", FindMarkdown),
            MenuItem::action("Find and Replace…", FindReplaceMarkdown),
            MenuItem::action("Find Next", FindNextMarkdown),
            MenuItem::action("Find Previous", FindPreviousMarkdown),
            MenuItem::separator(),
            MenuItem::action("Pseudonymize", Pseudonymize),
            MenuItem::action("Pseudonymize Clipboard", PseudonymizeClipboard),
        ]),
        Menu::new("View").items([
            MenuItem::action("Toggle Sidebar", ToggleSidebar),
            MenuItem::action("Before / After", ToggleCompare),
            MenuItem::separator(),
            MenuItem::action("Zoom In", ZoomIn),
            MenuItem::action("Zoom Out", ZoomOut),
            MenuItem::action("Actual Size", ZoomReset),
        ]),
        // GPUI registers the menu named "Window" as the app's Windows menu,
        // so macOS lists the open windows there.
        Menu::new("Window").items([
            MenuItem::action("Minimize", MinimizeWindow),
            MenuItem::action("Zoom", ZoomWindow),
            MenuItem::separator(),
            MenuItem::action("Next Tab", NextTab),
            MenuItem::action("Previous Tab", PreviousTab),
        ]),
    ]
}
