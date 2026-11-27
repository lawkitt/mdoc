//! The palette: the command palette's rows, filter, actions and headings,
//! and the keyboard shortcuts sheet, opened by `?` outside text fields, ⌘/
//! (Ctrl+/) everywhere and the palette, with every table row drawn.
use super::*;
use crate::{commands::shortcuts, workspace::palette::shortcuts_ui};

fn sheet_open(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> bool {
    tabs.read_with(cx, |tabs, _| tabs.shell.shortcuts.is_some())
}

/// How many rows the sheet draws.
fn drawn_rows(cx: &mut VisualTestContext) -> usize {
    draw(cx);
    let mut count = 0;
    while cx
        .debug_bounds(Box::leak(
            format!("shortcut-row-{}", count + 1).into_boxed_str(),
        ))
        .is_some()
    {
        count += 1;
    }
    count
}

#[gpui::test]
fn question_mark_on_home_opens_every_shortcut(cx: &mut TestAppContext) {
    cx.update(crate::bind_all_keys);
    let (tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    let home = cx.update(|window, cx| {
        let focus = tabs.read(cx).focus.clone();
        window.focus(&focus, cx);
        focus
    });
    cx.simulate_keystrokes("?");
    draw(cx);
    assert!(sheet_open(&tabs, cx));
    assert!(cx.debug_bounds("shortcuts-sheet").is_some());
    // Every row of the one table, this platform's keys only.
    let rows = shortcuts::rows().count();
    assert!(rows > 80, "{rows} rows");
    assert_eq!(drawn_rows(cx), rows);
    for label in ["Back", "Forward"] {
        assert!(shortcuts_ui::visible(label).iter().any(|(group, rows)| {
            *group == shortcuts::Group::Everywhere && rows.iter().any(|row| row.label == label)
        }));
    }
    // The filter has focus: `?` types there instead of closing the sheet.
    cx.simulate_input("zoom");
    cx.simulate_keystrokes("?");
    assert!(sheet_open(&tabs, cx));
    // "zoom?" matches nothing.
    assert_eq!(drawn_rows(cx), 0);
    cx.simulate_keystrokes("backspace");
    let labels: Vec<_> = shortcuts_ui::visible("zoom")
        .into_iter()
        .flat_map(|(_, rows)| rows.into_iter().map(|row| row.label))
        .collect();
    assert_eq!(labels, ["Zoom in", "Zoom out"]);
    assert_eq!(drawn_rows(cx), 2);
    // A key finds its row too.
    let key = shortcuts::label(&crate::workspace::New).unwrap();
    assert!(
        shortcuts_ui::visible(&key)
            .iter()
            .any(|(_, rows)| rows.iter().any(|row| row.label == "New tab"))
    );
    // F6 stays inside, like the palette.
    cx.simulate_keystrokes("f6");
    assert!(sheet_open(&tabs, cx));
    // Esc closes it and focus goes back.
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(!sheet_open(&tabs, cx));
    assert!(cx.debug_bounds("shortcuts-sheet").is_none());
    cx.update(|window, _| assert!(home.is_focused(window)));
}

#[gpui::test]
fn question_mark_types_in_the_editor_and_mod_slash_opens_anywhere(cx: &mut TestAppContext) {
    cx.update(crate::bind_all_keys);
    let (tabs, cx) = boot(cx, Session::default());
    let view = active(&tabs, cx);
    cx.simulate_keystrokes("?");
    cx.run_until_parked();
    assert!(!sheet_open(&tabs, cx));
    cx.update(|_, cx| assert_eq!(view.read(cx).editor.read(cx).text(), "?"));
    let editor = cx.update(|_, cx| view.read(cx).editor.focus_handle(cx));
    cx.simulate_keystrokes("secondary-/");
    draw(cx);
    assert!(sheet_open(&tabs, cx));
    // ⌘/ again closes it, back in the editor.
    cx.simulate_keystrokes("secondary-/");
    draw(cx);
    assert!(!sheet_open(&tabs, cx));
    cx.update(|window, _| assert!(editor.is_focused(window)));
    cx.update(|_, cx| assert_eq!(view.read(cx).editor.read(cx).text(), "?"));
}

#[gpui::test]
fn the_palette_opens_the_sheet_and_shows_keys_from_the_table(cx: &mut TestAppContext) {
    cx.update(crate::bind_all_keys);
    let (tabs, cx) = boot(cx, Session::default());
    // Every key the palette shows is one the table lists.
    let listed: Vec<String> = shortcuts::rows().flat_map(|row| row.key_labels()).collect();
    let shown = tabs.read_with(cx, |tabs, cx| tabs.palette_shortcuts(cx));
    assert!(shown.len() >= 10, "{shown:?}");
    for (label, key) in &shown {
        assert!(listed.contains(key), "palette row {label} shows {key}");
    }
    let mod_ = if cfg!(target_os = "macos") {
        "⌘"
    } else {
        "Ctrl+"
    };
    assert!(shown.contains(&("Keyboard shortcuts".into(), format!("{mod_}/"))));
    assert!(shown.contains(&("New document".into(), format!("{mod_}N"))));
    cx.simulate_keystrokes("secondary-k");
    cx.simulate_input("keyboard shortcuts");
    cx.simulate_keystrokes("enter");
    draw(cx);
    assert!(tabs.read_with(cx, |tabs, _| tabs.shell.palette.is_none()));
    assert!(sheet_open(&tabs, cx));
}

#[gpui::test]
fn a_command_that_moves_focus_closes_the_sheet(cx: &mut TestAppContext) {
    cx.update(crate::bind_all_keys);
    let (tabs, cx) = boot(cx, Session::default());
    // Focus events reach an active window only, as on screen.
    cx.update(|window, _| window.activate_window());
    let before = tabs.read_with(cx, |tabs, _| tabs.tabs.active());
    cx.simulate_keystrokes("secondary-/");
    draw(cx);
    assert!(sheet_open(&tabs, cx));
    // Leaving the window does not close it.
    cx.deactivate_window();
    cx.update(|window, _| window.activate_window());
    draw(cx);
    assert!(sheet_open(&tabs, cx));
    // The palette would take focus the closed sheet cannot give back.
    cx.simulate_keystrokes("secondary-k");
    draw(cx);
    assert!(tabs.read_with(cx, |tabs, _| tabs.shell.palette.is_none()));
    assert!(sheet_open(&tabs, cx));
    // ⌘N opens a tab in front, not behind the sheet.
    cx.simulate_keystrokes("secondary-n");
    draw(cx);
    assert!(!sheet_open(&tabs, cx));
    assert_ne!(tabs.read_with(cx, |tabs, _| tabs.tabs.active()), before);
    let view = active(&tabs, cx);
    cx.simulate_input("typed");
    cx.update(|_, cx| assert_eq!(view.read(cx).editor.read(cx).text(), "typed"));
}

#[gpui::test]
fn shortcuts_over_settings_keep_document_commands_blocked(cx: &mut TestAppContext) {
    cx.update(crate::bind_all_keys);
    let (tabs, cx) = boot(cx, Session::default());
    cx.update(|window, _| window.activate_window());
    let view = active(&tabs, cx);
    cx.simulate_input("original");
    let count = tabs.read_with(cx, |tabs, _| tabs.tabs.len());
    tabs.update_in(cx, |tabs, window, cx| tabs.show_settings(window, cx));
    draw(cx);
    cx.simulate_keystrokes("secondary-/");
    draw(cx);
    assert!(sheet_open(&tabs, cx));
    // Settings must keep its command restrictions while its sibling sheet
    // holds focus. Typing after New still belongs to the shortcut filter.
    cx.simulate_keystrokes("secondary-n");
    draw(cx);
    cx.simulate_input("typed");
    assert_eq!(tabs.read_with(cx, |tabs, _| tabs.tabs.len()), count);
    assert!(sheet_open(&tabs, cx));
    cx.update(|_, cx| assert_eq!(view.read(cx).editor.read(cx).text(), "original"));
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(!sheet_open(&tabs, cx));
    cx.update(|window, cx| {
        let settings = tabs.read(cx).settings.read(cx);
        assert!(settings.open);
        assert!(settings.focus_handle(cx).contains_focused(window, cx));
    });
    cx.simulate_keystrokes("secondary-n");
    cx.simulate_input("still settings");
    assert_eq!(tabs.read_with(cx, |tabs, _| tabs.tabs.len()), count);
    cx.update(|_, cx| assert_eq!(view.read(cx).editor.read(cx).text(), "original"));
}

#[gpui::test]
fn settings_shortcuts_row_opens_sheet_and_restores_row_focus(cx: &mut TestAppContext) {
    cx.update(crate::bind_all_keys);
    let (tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    cx.update(|window, _| window.activate_window());
    click_toolbar(cx, "Settings");
    // Reverse from the selected category: close, Reset, then shortcuts.
    key(cx, "shift-tab");
    key(cx, "shift-tab");
    key(cx, "shift-tab");
    let row_focus = cx.update(|window, cx| window.focused(cx).unwrap());
    assert!(cx.debug_bounds("settings-shortcuts").is_some());
    activate_control(cx);
    assert!(sheet_open(&tabs, cx));
    assert!(cx.debug_bounds("shortcuts-sheet").is_some());
    assert_eq!(drawn_rows(cx), shortcuts::rows().count());
    key(cx, "escape");
    assert!(!sheet_open(&tabs, cx));
    cx.update(|window, cx| {
        let panel = tabs.read(cx).settings.read(cx);
        assert!(panel.open);
        assert_eq!(panel.category(), crate::settings::Category::General);
        assert_eq!(window.focused(cx), Some(row_focus));
    });
}

struct Other(gpui::FocusHandle);
impl Render for Other {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        div().track_focus(&self.0).size_full()
    }
}

#[gpui::test]
fn the_sheet_opens_from_another_window(cx: &mut TestAppContext) {
    cx.update(crate::bind_all_keys);
    let (tabs, cx) = boot(cx, Session::default());
    let other = cx.update(|_, cx| {
        cx.open_window(Default::default(), |window, cx| {
            cx.new(|cx| {
                let focus = cx.focus_handle();
                window.focus(&focus, cx);
                Other(focus)
            })
        })
        .unwrap()
    });
    cx.run_until_parked();
    other
        .update(cx, |_, window, cx| {
            window.dispatch_action(Box::new(shortcuts_ui::ShowShortcuts), cx)
        })
        .unwrap();
    cx.run_until_parked();
    assert!(sheet_open(&tabs, cx));
}

#[gpui::test]
fn command_palette_filters_and_runs_actions(cx: &mut TestAppContext) {
    cx.update(crate::workspace::bind_keys);
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    draw(cx);
    // ⌘K opens it from the document.
    cx.simulate_keystrokes("secondary-k");
    draw(cx);
    assert!(cx.debug_bounds("command-palette").is_some());
    cx.simulate_input("home");
    draw(cx);
    cx.simulate_keystrokes("enter");
    draw(cx);
    assert!(cx.debug_bounds("command-palette").is_none());
    assert!(cx.debug_bounds("home").is_some());
    // From Home, Esc closes it again.
    cx.simulate_keystrokes("secondary-k");
    draw(cx);
    assert!(cx.debug_bounds("command-palette").is_some());
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(cx.debug_bounds("command-palette").is_none());
    cx.update(|_, cx| assert!(tabs.read(cx).shell.palette.is_none()));
    // View actions without a menu bar on Windows are in the palette too.
    let visible = tabs.read_with(cx, |tabs, _| tabs.sidebar.visible());
    cx.simulate_keystrokes("secondary-k");
    draw(cx);
    cx.simulate_input("toggle sidebar");
    draw(cx);
    cx.simulate_keystrokes("enter");
    draw(cx);
    tabs.read_with(cx, |tabs, _| assert_eq!(tabs.sidebar.visible(), !visible));
}

/// The palette's theme row names the theme it switches to (U14), and running
/// it moves the name on to the next one.
#[gpui::test]
fn command_palette_theme_row_names_its_target(cx: &mut TestAppContext) {
    cx.update(crate::workspace::bind_keys);
    let (tabs, cx) = boot_home(cx, Session::default());
    for (now, row, next) in [
        (
            ThemeChoice::System,
            "Switch to light theme",
            ThemeChoice::Light,
        ),
        (
            ThemeChoice::Light,
            "Switch to dark theme",
            ThemeChoice::Dark,
        ),
        (
            ThemeChoice::Dark,
            "Switch to system theme",
            ThemeChoice::System,
        ),
    ] {
        tabs.read_with(cx, |tabs, cx| {
            assert_eq!(tabs.theme_choice, now);
            let actions = tabs.palette_section("Actions", cx);
            assert!(actions.iter().any(|label| label == row), "{actions:?}");
            assert!(
                !actions
                    .iter()
                    .any(|label| label.starts_with("Switch theme"))
            );
        });
        cx.simulate_keystrokes("secondary-k");
        cx.simulate_input(row);
        draw(cx);
        cx.simulate_keystrokes("enter");
        draw(cx);
        cx.update(|_, cx| assert_eq!(tabs.read(cx).theme_choice, next));
    }
}

#[gpui::test]
fn command_palette_routes_every_settings_category(cx: &mut TestAppContext) {
    cx.update(crate::workspace::bind_keys);
    let (tabs, cx) = boot_home(cx, Session::default());
    let panel = cx.update(|_, cx| tabs.read(cx).settings.clone());
    let labels = tabs.read_with(cx, |tabs, cx| tabs.palette_section("Go to", cx));
    for category in crate::settings::Category::ALL {
        assert!(labels.contains(&format!("Settings › {}", category.label())));
        cx.simulate_keystrokes("secondary-k");
        cx.simulate_input(&format!("settings {}", category.label()));
        draw(cx);
        cx.simulate_keystrokes("enter");
        draw(cx);
        cx.update(|_, cx| {
            assert!(tabs.read(cx).shell.palette.is_none());
            assert!(panel.read(cx).open);
            assert_eq!(panel.read(cx).category(), category);
        });
        panel.update_in(cx, |panel, window, cx| panel.close(window, cx));
        draw(cx);
    }
}

/// The palette lists the document's headings; choosing one moves the caret
/// to its line and scrolls it into view, leaving Undo as it was.
#[gpui::test]
fn palette_goes_to_a_heading(cx: &mut TestAppContext) {
    cx.update(crate::workspace::bind_keys);
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(600.)));
    let view = active(&tabs, cx);
    let filler = "Paragraph text.\n\n".repeat(60);
    let text = format!("# Overview\n\n{filler}## Termination clause\n\n{filler}## Fees\n");
    view.update_in(cx, |view, _, cx| {
        view.editor.update(cx, |e, cx| e.set_text(&text, cx));
        view.editor.update(cx, |e, cx| e.set_cursor(text.len(), cx));
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.focus_active(window, cx));
    draw(cx);
    // One typed character is the Undo step to keep.
    cx.simulate_input("x");
    draw(cx);
    let rows = cx.update(|_, cx| tabs.read(cx).palette_section("Go to heading", cx));
    assert_eq!(rows, ["Overview", "Termination clause", "Fees"]);
    // A narrow window showing Source switches back to the Markdown pane.
    view.update(cx, |view, _| view.original_selected = true);
    cx.simulate_keystrokes("secondary-k");
    draw(cx);
    cx.simulate_input("termination");
    draw(cx);
    cx.simulate_keystrokes("enter");
    draw(cx);
    draw(cx);
    let target = text.find("## Termination").unwrap();
    assert!(!cx.update(|_, cx| view.read(cx).original_selected));
    assert!(editor_focused(&tabs, cx));
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(view.editor.read(cx).cursor(), target);
        let caret = view.editor.read(cx).caret_bounds().unwrap();
        let viewport = view.scroll.bounds();
        assert!(
            caret.top() >= viewport.top() && caret.bottom() <= viewport.bottom(),
            "heading {caret:?} outside {viewport:?}"
        );
    });
    // Undo still removes the typed character and nothing else.
    cx.dispatch_action(mdoc_editor::Undo);
    draw(cx);
    cx.update(|_, cx| assert_eq!(view.read(cx).editor.read(cx).text(), text));
}
