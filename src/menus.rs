//! The menu bar. On macOS GPUI makes the first menu the application menu, so
//! it is the app menu (About, Check for Updates…, Settings, Services, Hide,
//! Quit), followed by File, Edit, View, Window and Help. Edit items use the
//! editor's actions with their OS actions, so they also reach native
//! Open/Save panels. File › Open Recent follows the window's Recents.

use crate::{
    document_view::{
        CopyMarkdown, Save, SaveAs, ShowEditView, ShowOriginalView, ShowPseudonymizedView,
        ShowReplacements, ToggleCompare, ZoomIn, ZoomOut, ZoomReset,
    },
    pii::ui::{CopyForAi, Pseudonymize},
    search_session::{FindMarkdown, FindNextMarkdown, FindPreviousMarkdown, FindReplaceMarkdown},
    workspace::{
        CheckForUpdates, ClearRecentFiles, Close, Import, New, NextTab, Open, OpenCase,
        OpenPalette, OpenRecent, PasteAiResponse, PreviousTab, PseudonymizeClipboard, Quit,
        RestoreClipboard, Settings, ShowHome, ToggleSidebar, shortcuts_ui::ShowShortcuts,
    },
};
use gpui::{App, Global, Menu, MenuItem, OsAction, SystemMenuType, actions};
use std::path::PathBuf;

/// What Help › mdoc Help and the sidebar's ? open.
pub(crate) const HELP_URL: &str = "https://github.com/lawkitt/mdoc#readme";

/// File › Open Recent lists at most this many files.
const RECENT_LIMIT: usize = 10;

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

/// The files File › Open Recent lists, most recent first.
#[derive(Default)]
struct RecentFiles(Vec<PathBuf>);

impl Global for RecentFiles {}

/// Set once [`install`] has put up the menu bar, so Recents changes rebuild it.
struct Installed;

impl Global for Installed {}

/// The window's Recents changed: rebuild File › Open Recent.
pub(crate) fn set_recent_files(paths: Vec<PathBuf>, cx: &mut App) {
    cx.set_global(RecentFiles(paths));
    if cx.has_global::<Installed>() {
        cx.set_menus(menus(&recent_files(cx)));
    }
}

/// File › Open Recent's item titles, separators left out (tests).
#[cfg(test)]
pub(crate) fn open_recent_titles(cx: &App) -> Vec<gpui::SharedString> {
    titles(open_recent(&recent_files(cx)))
}

#[cfg(test)]
fn titles(menu: Menu) -> Vec<gpui::SharedString> {
    menu.items
        .into_iter()
        .filter_map(|item| match item {
            MenuItem::Action { name, .. } => Some(name),
            _ => None,
        })
        .collect()
}

fn recent_files(cx: &App) -> Vec<PathBuf> {
    cx.try_global::<RecentFiles>()
        .map(|recent| recent.0.clone())
        .unwrap_or_default()
}

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
    cx.bind_keys(crate::shortcuts::bindings(crate::shortcuts::Owner::Menus));
    cx.set_global(Installed);
    cx.set_menus(menus(&recent_files(cx)));
}

/// Every action item, with its title, in menu order (tests).
#[cfg(test)]
pub(crate) fn items(recent: &[PathBuf]) -> Vec<(gpui::SharedString, Box<dyn gpui::Action>)> {
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
    for menu in menus(recent) {
        walk(menu.items, &mut out);
    }
    out
}

/// File › Open Recent: file names, with the folder when two share a name,
/// then Clear Menu.
fn open_recent(recent: &[PathBuf]) -> Menu {
    let recent = &recent[..recent.len().min(RECENT_LIMIT)];
    let name = |path: &PathBuf| {
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned())
    };
    let mut items: Vec<MenuItem> = recent
        .iter()
        .map(|path| {
            let file = name(path);
            let shared = recent.iter().filter(|other| name(other) == file).count() > 1;
            let folder = path
                .parent()
                .and_then(|folder| folder.file_name())
                .map(|folder| folder.to_string_lossy().into_owned());
            let label = match folder {
                Some(folder) if shared => format!("{file} — {folder}"),
                _ => file,
            };
            MenuItem::action(label, OpenRecent(path.clone()))
        })
        .collect();
    if !items.is_empty() {
        items.push(MenuItem::separator());
    }
    items.push(MenuItem::action("Clear Menu", ClearRecentFiles).disabled(recent.is_empty()));
    Menu::new("Open Recent").items(items)
}

fn menus(recent: &[PathBuf]) -> Vec<Menu> {
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
            MenuItem::submenu(open_recent(recent)),
            MenuItem::action("Open Case…", OpenCase),
            MenuItem::action("Convert to Markdown", Import),
            MenuItem::separator(),
            MenuItem::action("Save", Save),
            MenuItem::action("Save As…", SaveAs),
            MenuItem::separator(),
            MenuItem::action("Copy Markdown", CopyMarkdown),
            MenuItem::action("Copy for AI", CopyForAi),
            MenuItem::action("Paste AI Reply", PasteAiResponse),
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
        Menu::new("Help").items(help),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_recent_lists_ten_names_then_clear_menu() {
        assert_eq!(titles(open_recent(&[])), ["Clear Menu"]);
        assert!(open_recent(&[]).items[0].is_disabled(), "nothing to clear");

        let mut recent: Vec<PathBuf> = (0..12)
            .map(|i| PathBuf::from(format!("/docs/note-{i}.md")))
            .collect();
        recent[1] = PathBuf::from("/a/Contract.md");
        recent[2] = PathBuf::from("/b/Contract.md");
        let titles = titles(open_recent(&recent));
        assert_eq!(titles.len(), 11, "ten files, then Clear Menu");
        assert_eq!(titles[0], "note-0.md");
        assert_eq!(
            titles[1], "Contract.md — a",
            "a shared name shows its folder"
        );
        assert_eq!(titles[2], "Contract.md — b");
        assert_eq!(titles[10], "Clear Menu");
        assert!(!open_recent(&recent).items.last().unwrap().is_disabled());
    }
}
