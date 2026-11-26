//! The menu bar. On macOS GPUI makes the first menu the application menu, so
//! it is the app menu (About, Check for Updates…, Settings, Services, Hide,
//! Quit), followed by File, Edit, View, Window and Help. Edit items use the
//! editor's actions with their OS actions, so they also reach native
//! Open/Save panels. The Window menu also lists the live views by title,
//! the one shown ticked; choosing one shows it (#1160). The workspace
//! hands over the list as it changes, and the menu bar is set again.

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
        ExportProject, Import, New, NewProject, Open, OpenPalette, OpenProject, PasteAiResponse,
        ProjectCategories, PseudonymizeClipboard, Quit, RenameProject, RestoreClipboard, Settings,
        ShowHome, SwitchDocument, SwitchDocumentBack, ToggleSidebar, shortcuts_ui::ShowShortcuts,
    },
};
use gpui::{App, Menu, MenuItem, OsAction, SharedString, SystemMenuType, actions};

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
    /// The one the window shows: ticked.
    pub(crate) shown: bool,
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
    let bindings = crate::shortcuts::bindings(crate::shortcuts::Owner::Menus, cx);
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

/// Window's own items, then each live view, the one shown ticked.
fn window_items(views: &[LiveViewItem]) -> Vec<MenuItem> {
    let mut items = vec![
        MenuItem::action("Minimize", MinimizeWindow),
        MenuItem::action("Zoom", ZoomWindow),
        MenuItem::separator(),
        MenuItem::action("Switch Document", SwitchDocument),
        MenuItem::action("Switch Document Back", SwitchDocumentBack),
    ];
    if !views.is_empty() {
        items.push(MenuItem::separator());
    }
    items.extend(views.iter().map(|view| {
        MenuItem::action(view.title.clone(), ShowLiveView(view.id)).checked(view.shown)
    }));
    items
}

fn menus(views: &[LiveViewItem]) -> Vec<Menu> {
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
            MenuItem::action("New Project…", NewProject),
            MenuItem::separator(),
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
        // so macOS lists the open windows there, after the live views.
        Menu::new("Window").items(window_items(views)),
        Menu::new("Help").items(help),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(id: u64, title: &str, shown: bool) -> LiveViewItem {
        LiveViewItem {
            id,
            title: title.to_string().into(),
            shown,
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
    /// own items, the one shown ticked, each choosing its own view; with
    /// none, Window is as before.
    #[test]
    fn the_window_menu_lists_the_live_views() {
        let own = vec![
            ("Minimize".to_string(), false, None),
            ("Zoom".to_string(), false, None),
            ("Switch Document".to_string(), false, None),
            ("Switch Document Back".to_string(), false, None),
        ];
        assert_eq!(window_rows(&[]), own);
        let views = [view(3, "Lease.md", false), view(7, "Letter.md", true)];
        let mut expected = own.clone();
        expected.extend([
            ("Lease.md".to_string(), false, Some(3)),
            ("Letter.md".to_string(), true, Some(7)),
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
