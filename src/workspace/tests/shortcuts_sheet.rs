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
