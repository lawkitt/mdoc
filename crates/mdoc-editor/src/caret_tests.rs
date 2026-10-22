//! macOS caret moves and deletions (ADR 0061), run on any OS through the
//! macOS keymap.
use super::*;

/// An editor on the macOS keymap, focused on `text` with the caret at `at`,
/// laid out once so wrapped rows are known.
fn mac_editor<'a>(
    cx: &'a mut gpui::TestAppContext,
    text: &str,
    at: usize,
) -> (Entity<EditorState>, &'a mut gpui::VisualTestContext) {
    cx.update(|cx| cx.bind_keys(key_bindings(true)));
    let (editor, cx) = cx.add_window_view(EditorState::new);
    editor.update_in(cx, |e, window, cx| {
        e.set_text(text, cx);
        e.set_cursor(at, cx);
        e.focus(window, cx);
    });
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    (editor, cx)
}

fn press(keys: &str, cx: &mut gpui::VisualTestContext) {
    cx.simulate_keystrokes(keys);
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
}

fn selection(editor: &Entity<EditorState>, cx: &mut gpui::VisualTestContext) -> Range<usize> {
    editor.read_with(cx, |e, _| e.selected_range.clone())
}

fn text(editor: &Entity<EditorState>, cx: &mut gpui::VisualTestContext) -> String {
    editor.read_with(cx, |e, _| e.text().to_string())
}

#[gpui::test]
fn cmd_arrows_move_to_line_and_document_ends(cx: &mut gpui::TestAppContext) {
    let source = "alpha beta\ngamma delta";
    let (editor, cx) = mac_editor(cx, source, 3);
    press("cmd-right", cx);
    assert_eq!(selection(&editor, cx), 10..10);
    press("cmd-left", cx);
    assert_eq!(selection(&editor, cx), 0..0);
    press("cmd-down", cx);
    assert_eq!(selection(&editor, cx), source.len()..source.len());
    press("cmd-up", cx);
    assert_eq!(selection(&editor, cx), 0..0);
}

#[gpui::test]
fn shift_cmd_arrows_select_to_line_and_document_ends(cx: &mut gpui::TestAppContext) {
    let source = "alpha beta\ngamma delta";
    let (editor, cx) = mac_editor(cx, source, 14);
    press("cmd-shift-left", cx);
    assert_eq!(selection(&editor, cx), 11..14);
    press("cmd-right", cx);
    press("cmd-shift-up", cx);
    assert_eq!(selection(&editor, cx), 0..source.len());
    press("cmd-up", cx);
    press("cmd-shift-right", cx);
    assert_eq!(selection(&editor, cx), 0..10);
    press("cmd-shift-down", cx);
    assert_eq!(selection(&editor, cx), 0..source.len());
}

#[gpui::test]
fn cmd_arrows_follow_the_wrapped_row(cx: &mut gpui::TestAppContext) {
    let source = "word ".repeat(400);
    let source = source.trim_end().to_string();
    let (editor, cx) = mac_editor(cx, &source, 0);
    // ⌘→ stops before the space the first row broke at, not the line's end.
    press("cmd-right", cx);
    let end = selection(&editor, cx).start;
    assert!(end > 0 && end < source.len(), "{end}");
    assert_eq!(&source[end..end + 1], " ");
    // Again: still on that row.
    press("cmd-right", cx);
    assert_eq!(selection(&editor, cx).start, end);
    // From the last row, ⌘← goes to that row's start, after a break space.
    press("cmd-down", cx);
    press("cmd-left", cx);
    let start = selection(&editor, cx).start;
    assert!(start > end && start < source.len(), "{start}");
    assert_eq!(&source[start - 1..start], " ");
    // ⇧⌘→ selects the rest of that row, which is the line's end.
    press("cmd-shift-right", cx);
    assert_eq!(selection(&editor, cx), start..source.len());
}

#[gpui::test]
fn cmd_left_lands_after_a_list_marker_then_at_the_line_start(cx: &mut gpui::TestAppContext) {
    let source = "- item text";
    let (editor, cx) = mac_editor(cx, source, source.len());
    editor.update(cx, |e, cx| {
        e.set_markdown_style(markdown_syntax::search_style(), cx)
    });
    press("cmd-left", cx);
    assert_eq!(selection(&editor, cx), 2..2);
    press("cmd-left", cx);
    assert_eq!(selection(&editor, cx), 0..0);
}

#[gpui::test]
fn alt_and_cmd_deletions(cx: &mut gpui::TestAppContext) {
    let source = "keep\none two three";
    let (editor, cx) = mac_editor(cx, source, source.len());
    press("alt-backspace", cx);
    assert_eq!(text(&editor, cx), "keep\none two ");
    press("cmd-backspace", cx);
    assert_eq!(text(&editor, cx), "keep\n");
    // At the line start ⌘⌫ joins with the line above, like ⌫.
    press("cmd-backspace", cx);
    assert_eq!(text(&editor, cx), "keep");
    editor.update(cx, |e, cx| {
        e.set_text("one two", cx);
        e.set_cursor(0, cx);
    });
    press("alt-delete", cx);
    assert_eq!(text(&editor, cx), " two");
}

#[gpui::test]
fn the_other_keymap_keeps_ctrl_moves_and_no_cmd_moves(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| cx.bind_keys(key_bindings(false)));
    let (editor, cx) = cx.add_window_view(EditorState::new);
    let source = "alpha beta
gamma";
    editor.update_in(cx, |e, window, cx| {
        e.set_text(source, cx);
        e.set_cursor(3, cx);
        e.focus(window, cx);
    });
    for key in ["cmd-left", "cmd-right", "cmd-up", "cmd-down"] {
        press(key, cx);
        assert_eq!(selection(&editor, cx), 3..3, "{key}");
    }
    press("ctrl-end", cx);
    assert_eq!(selection(&editor, cx), source.len()..source.len());
    press("ctrl-home", cx);
    assert_eq!(selection(&editor, cx), 0..0);
}

#[gpui::test]
fn cmd_moves_and_cmd_backspace_stay_inside_a_table_cell(cx: &mut gpui::TestAppContext) {
    let source = "| one | two |\n| --- | --- |\n| red | blue |";
    let at = source.find("blue").unwrap() + 2;
    let (editor, cx) = mac_editor(cx, source, at);
    editor.update(cx, |e, cx| {
        e.set_markdown_style(markdown_syntax::search_style(), cx)
    });
    press("cmd-left", cx);
    let cell = source.find("blue").unwrap();
    assert_eq!(selection(&editor, cx), cell..cell);
    press("cmd-right", cx);
    assert_eq!(selection(&editor, cx), cell + 4..cell + 4);
    // ⌘⌫ deletes back to the cell's start only, keeping the pipes.
    press("cmd-backspace", cx);
    assert_eq!(
        text(&editor, cx),
        "| one | two |\n| --- | --- |\n| red |  |"
    );
}

#[gpui::test]
fn cmd_right_stays_on_a_row_that_broke_inside_a_word(cx: &mut gpui::TestAppContext) {
    let source = "x".repeat(3000);
    let (editor, cx) = mac_editor(cx, &source, 0);
    press("cmd-right", cx);
    let end = selection(&editor, cx).start;
    assert!(end > 0 && end < source.len(), "{end}");
    // Again: the caret does not walk into the next row.
    press("cmd-right", cx);
    assert_eq!(selection(&editor, cx).start, end);
    press("cmd-shift-left", cx);
    assert_eq!(selection(&editor, cx), 0..end);
}

#[gpui::test]
fn cmd_arrows_go_to_the_visual_edges_of_a_right_to_left_line(cx: &mut gpui::TestAppContext) {
    let source = "שלום עולם";
    let (editor, cx) = mac_editor(cx, source, 4);
    // Each lands on the caret stop painted furthest left / right, whatever
    // the shaping order; on a real RTL line, ⌘→ is the logical start.
    let painted_x = |editor: &Entity<EditorState>, cx: &mut gpui::VisualTestContext| {
        editor.read_with(cx, |e, _| {
            let at = e.cursor_offset();
            let r = e.bidi_map(0).unwrap();
            r.rows[0].map.x_for_index(e.display_col(0, at))
        })
    };
    press("cmd-right", cx);
    let right = painted_x(&editor, cx);
    press("cmd-left", cx);
    let left = painted_x(&editor, cx);
    assert!(left < right, "{left} {right}");
    let stops: Vec<f32> = editor.read_with(cx, |e, _| {
        let r = e.bidi_map(0).unwrap();
        source
            .char_indices()
            .map(|(i, _)| i)
            .chain([source.len()])
            .map(|i| r.rows[0].map.x_for_index(e.display_col(0, i)))
            .collect()
    });
    assert!(stops.iter().all(|x| *x >= left && *x <= right), "{stops:?}");
}

#[gpui::test]
fn cmd_backspace_on_a_right_to_left_line_deletes_to_its_reading_start(
    cx: &mut gpui::TestAppContext,
) {
    let source = "keep
שלום עולם";
    let (editor, cx) = mac_editor(cx, source, source.len());
    press("cmd-backspace", cx);
    assert_eq!(
        text(&editor, cx),
        "keep
"
    );
}
