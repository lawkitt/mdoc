//! The keyboard shortcuts sheet (ADR 0073): opened by `?` outside text
//! fields, ⌘/ (Ctrl+/) everywhere and the palette; every table row drawn.
use super::*;
use crate::{shortcuts, workspace::shortcuts_ui};

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
    // F6 stays inside, like the palette (ADR 0062).
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
    assert!(shown.contains(&("New tab".into(), format!("{mod_}N"))));
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
    let count = tabs.read_with(cx, |tabs, _| tabs.tabs.len());
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
    assert_eq!(tabs.read_with(cx, |tabs, _| tabs.tabs.len()), count + 1);
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
        assert_eq!(panel.category(), settings_ui::Category::General);
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
