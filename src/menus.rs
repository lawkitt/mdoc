//! The menu bar. On macOS GPUI makes the first menu the application menu, so
//! it is the app menu (About, Check for Updates…, Settings, Services, Hide,
//! Quit), followed by File, Edit, View, Window and Help. Edit items use the
//! editor's actions with their OS actions, so they also reach native
//! Open/Save panels.

use crate::{
    document_view::{
        CopyMarkdown, ExportMarkdown, OpenSourceReply, Save, ShowEditView, ShowNotes,
        ShowOriginalView, ShowPseudonymizedView, ShowReplacements, ToggleCompare, ZoomIn, ZoomOut,
        ZoomReset,
    },
    pii::ui::{CopyForAi, Pseudonymize},
    search_session::{FindMarkdown, FindNextMarkdown, FindPreviousMarkdown, FindReplaceMarkdown},
    workspace::{
        AddDocumentToProject, ChangeProjectColour, CheckForUpdates, Close, DeleteProject,
        ExportProject, Import, New, Open, OpenPalette, OpenProject, PasteAiResponse,
        ProjectCategories, PseudonymizeClipboard, Quit, RenameProject, RestoreClipboard, Settings,
        ShowHome, SwitchDocument, SwitchDocumentBack, ToggleSidebar, shortcuts_ui::ShowShortcuts,
    },
};
use gpui::{App, Menu, MenuItem, OsAction, SystemMenuType, actions};

/// What Help › mdoc Help and the sidebar's ? open.
pub(crate) const HELP_URL: &str = "https://github.com/lawkitt/mdoc#readme";

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
    let bindings = crate::shortcuts::bindings(crate::shortcuts::Owner::Menus, cx);
    cx.bind_keys(bindings);
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
    let mut app = vec![MenuItem::action("About mdoc", About)];
    if cfg!(target_os = "macos") {
        app.push(MenuItem::action("Check for Updates…", CheckForUpdates));
    }
    app.extend([
        MenuItem::separator(),
        MenuItem::action("Settings…", Settings),
    ]);
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
    let mut help = vec![
        MenuItem::action("mdoc Help", OpenHelp),
        MenuItem::action("Keyboard Shortcuts", ShowShortcuts),
    ];
    if !cfg!(target_os = "macos") {
        help.extend([
            MenuItem::separator(),
            MenuItem::action("Check for Updates…", CheckForUpdates),
        ]);
    }
    vec![
        Menu::new("mdoc").items(app),
        Menu::new("File").items([
            MenuItem::action("New", New),
            MenuItem::action("Open…", Open),
            MenuItem::action("Open Project…", OpenProject),
            MenuItem::action("Convert to Markdown", Import),
            MenuItem::separator(),
            MenuItem::action("Save", Save),
            MenuItem::action("Export Markdown…", ExportMarkdown),
            MenuItem::separator(),
            MenuItem::action("Copy Markdown", CopyMarkdown),
            MenuItem::action("Copy for AI", CopyForAi),
            MenuItem::action("Paste AI Reply", PasteAiResponse),
            MenuItem::separator(),
            MenuItem::action("Close", Close),
        ]),
        Menu::new("Project").items([
            MenuItem::action("Add Document…", AddDocumentToProject),
            MenuItem::action("Rename Project", RenameProject),
            MenuItem::action("Project Colour", ChangeProjectColour),
            MenuItem::action("Categories to Detect", ProjectCategories),
            MenuItem::separator(),
            MenuItem::action("Export Project…", ExportProject),
            MenuItem::action("Delete Project…", DeleteProject),
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
            MenuItem::action("Restore Clipboard", RestoreClipboard),
        ]),
        Menu::new("View").items([
            MenuItem::action("Home", ShowHome),
            MenuItem::action("Search", OpenPalette),
            MenuItem::action("Toggle Sidebar", ToggleSidebar),
            MenuItem::separator(),
            MenuItem::action("Edit", ShowEditView),
            MenuItem::action("Original", ShowOriginalView),
            MenuItem::action("Pseudonymized", ShowPseudonymizedView),
            MenuItem::action("Side by Side", ToggleCompare),
            MenuItem::separator(),
            MenuItem::action("Show Replacements", ShowReplacements),
            MenuItem::action("Notes", ShowNotes),
            MenuItem::action("Open Source AI Reply", OpenSourceReply),
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
            MenuItem::action("Switch Document", SwitchDocument),
            MenuItem::action("Switch Document Back", SwitchDocumentBack),
        ]),
        Menu::new("Help").items(help),
    ]
}
