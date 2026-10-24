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

#[gpui::test]
fn cmd_backspace_at_a_cell_start_keeps_the_cells(cx: &mut gpui::TestAppContext) {
    let source = "| one | two |\n| --- | --- |\n| red | blue |";
    let at = source.find("blue").unwrap();
    let (editor, cx) = mac_editor(cx, source, at);
    editor.update(cx, |e, cx| {
        e.set_markdown_style(markdown_syntax::search_style(), cx)
    });
    press("cmd-backspace", cx);
    assert_eq!(text(&editor, cx), source);
}

#[gpui::test]
fn cmd_right_before_hidden_markers_at_a_wrap_stays_on_the_row(cx: &mut gpui::TestAppContext) {
    // Every word bold: each row's wrap lands right before a hidden `**`.
    let source = "**word** ".repeat(400);
    let source = source.trim_end().to_string();
    let (editor, cx) = mac_editor(cx, &source, 2);
    editor.update(cx, |e, cx| {
        e.set_markdown_style(markdown_syntax::search_style(), cx)
    });
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    let row_top = |editor: &Entity<EditorState>, cx: &mut gpui::VisualTestContext| {
        editor.read_with(cx, |e, _| {
            e.bounds_for_offset(e.cursor_offset()).map(|b| b.top())
        })
    };
    let top = row_top(&editor, cx);
    press("cmd-right", cx);
    let end = selection(&editor, cx).start;
    assert!(end > 2 && end < source.len(), "{end}");
    assert_eq!(row_top(&editor, cx), top);
    press("cmd-right", cx);
    assert_eq!(selection(&editor, cx).start, end);
}

#[gpui::test]
fn cmd_left_on_a_right_to_left_list_item_reaches_the_line_start(cx: &mut gpui::TestAppContext) {
    let source = "- abc שלום def";
    let (editor, cx) = mac_editor(cx, source, 4);
    editor.update(cx, |e, cx| {
        e.set_markdown_style(markdown_syntax::search_style(), cx)
    });
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    // From the body's start the next press reaches offset 0.
    editor.update(cx, |e, cx| e.set_cursor(2, cx));
    press("cmd-left", cx);
    assert_eq!(selection(&editor, cx), 0..0);
    // Also with hidden formatting right after the marker.
    editor.update(cx, |e, cx| {
        e.set_text("- **abc** שלום def", cx);
        e.set_cursor(12, cx);
    });
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    press("cmd-left", cx);
    press("cmd-left", cx);
    press("cmd-left", cx);
    assert_eq!(selection(&editor, cx), 0..0);
}

#[gpui::test]
fn cmd_left_on_a_wrapped_right_to_left_paragraph_stays_on_its_row(cx: &mut gpui::TestAppContext) {
    let source = "שלום עולם ".repeat(300);
    let source = source.trim_end().to_string();
    let (editor, cx) = mac_editor(cx, &source, 4);
    let row = |editor: &Entity<EditorState>, cx: &mut gpui::VisualTestContext| {
        editor.read_with(cx, |e, _| {
            let r = e.bidi_map(0).unwrap();
            r.row_of(e.display_col(0, e.cursor_offset())).0
        })
    };
    assert!(editor.read_with(cx, |e, _| e.bidi_map(0).unwrap().rows.len() > 1));
    let first = row(&editor, cx);
    for key in ["cmd-left", "cmd-left", "cmd-right", "cmd-right"] {
        press(key, cx);
        assert_eq!(row(&editor, cx), first, "{key}");
    }
}

/// The wrap boundaries of logical line `row` (display columns, which are
/// source offsets on a line without hidden markers).
fn boundaries(
    editor: &Entity<EditorState>,
    cx: &mut gpui::VisualTestContext,
    row: usize,
) -> Vec<usize> {
    editor.read_with(cx, |e, _| wrap_indices(&e.wrapped[row]).collect())
}

/// The caret's drawn top-left, from the same geometry the paint uses.
fn caret_at(editor: &Entity<EditorState>, cx: &mut gpui::VisualTestContext) -> Point<Pixels> {
    editor.read_with(cx, |e, _| {
        e.bounds_for_offset(e.cursor_offset()).unwrap().origin
    })
}

/// One unspaced word of 600 letters, then a short one: rows break inside it.
fn mid_word_source() -> String {
    format!("{} tail", "x".repeat(600))
}

#[gpui::test]
fn cmd_right_reaches_the_end_of_a_row_that_broke_inside_a_word(cx: &mut gpui::TestAppContext) {
    let source = mid_word_source();
    let (editor, cx) = mac_editor(cx, &source, 0);
    let b = boundaries(&editor, cx, 0);
    assert!(b.len() >= 2, "{b:?}");
    let row0 = caret_at(&editor, cx);
    press("cmd-right", cx);
    // After the row's last character, not one short of it, and still drawn
    // on the first row.
    assert_eq!(selection(&editor, cx), b[0]..b[0]);
    let end = caret_at(&editor, cx);
    assert_eq!(end.y, row0.y);
    let before_last = editor.read_with(cx, |e, _| e.bounds_for_offset(b[0] - 1).unwrap().origin);
    assert!(end.x > before_last.x, "{end:?} {before_last:?}");
    // Again: it stays. ⌘← and ⇧⌘← work from there on that row.
    press("cmd-right", cx);
    assert_eq!(selection(&editor, cx), b[0]..b[0]);
    press("cmd-shift-left", cx);
    assert_eq!(selection(&editor, cx), 0..b[0]);
    press("cmd-right", cx);
    press("cmd-left", cx);
    assert_eq!(selection(&editor, cx), 0..0);
    // ⇧⌘→ selects to the row's end with the head on that row.
    press("cmd-shift-right", cx);
    assert_eq!(selection(&editor, cx), 0..b[0]);
    assert!(editor.read_with(cx, |e, _| e.caret_upstream()));
}

#[gpui::test]
fn a_caret_on_a_wrap_boundary_draws_by_its_affinity(cx: &mut gpui::TestAppContext) {
    let source = mid_word_source();
    let (editor, cx) = mac_editor(cx, &source, 0);
    let b = boundaries(&editor, cx, 0);
    let row0 = caret_at(&editor, cx);
    // Placed there (downstream, the default): the start of the next row.
    editor.update(cx, |e, cx| e.set_cursor(b[0], cx));
    let down = caret_at(&editor, cx);
    assert!(down.y > row0.y, "{down:?}");
    assert_eq!(down.x, row0.x);
    // → from the row's end steps into the next row; ← from it back into
    // the first.
    editor.update(cx, |e, cx| e.set_cursor(0, cx));
    press("cmd-right", cx);
    press("right", cx);
    assert_eq!(selection(&editor, cx), b[0] + 1..b[0] + 1);
    assert_eq!(caret_at(&editor, cx).y, down.y);
    press("cmd-right", cx);
    press("left", cx);
    press("cmd-right", cx);
    press("cmd-left", cx);
    assert_eq!(selection(&editor, cx), b[0]..b[0]);
    // ⌘← on the second row lands on its start: the same offset, downstream.
    assert_eq!(caret_at(&editor, cx), down);
}

#[gpui::test]
fn up_and_down_keep_their_row_across_mid_word_wraps(cx: &mut gpui::TestAppContext) {
    let source = mid_word_source();
    let (editor, cx) = mac_editor(cx, &source, 0);
    let b = boundaries(&editor, cx, 0);
    let top = |cx: &mut gpui::VisualTestContext| caret_at(&editor, cx).y;
    let row_h = editor.read_with(cx, |e, _| e.line_h(0));
    let row0 = top(cx);
    // From a row's end, down and up go to the ends of the rows next to it.
    press("cmd-right", cx);
    press("down", cx);
    assert_eq!(selection(&editor, cx), b[1]..b[1]);
    assert_eq!(top(cx), row0 + row_h);
    press("down", cx);
    assert_eq!(selection(&editor, cx), b[2]..b[2]);
    assert_eq!(top(cx), row0 + row_h * 2.);
    press("up", cx);
    press("up", cx);
    assert_eq!(selection(&editor, cx), b[0]..b[0]);
    assert_eq!(top(cx), row0);
    // From a row's start, down goes to the next row's start: the same
    // boundary offsets, drawn below them.
    press("cmd-left", cx);
    press("down", cx);
    assert_eq!(selection(&editor, cx), b[0]..b[0]);
    assert_eq!(top(cx), row0 + row_h);
    press("down", cx);
    assert_eq!(selection(&editor, cx), b[1]..b[1]);
    assert_eq!(top(cx), row0 + row_h * 2.);
    // ⇧↓ from a row's end keeps the head at the next row's end.
    press("up", cx);
    press("up", cx);
    press("cmd-right", cx);
    press("shift-down", cx);
    assert_eq!(selection(&editor, cx), b[0]..b[1]);
    assert_eq!(top(cx), row0 + row_h);
}

#[gpui::test]
fn home_and_end_span_the_paragraph_across_wraps(cx: &mut gpui::TestAppContext) {
    let source = format!("before\n{}\nafter", mid_word_source());
    let start = "before\n".len();
    let end = start + mid_word_source().len();
    for mac in [true, false] {
        cx.update(|cx| {
            cx.clear_key_bindings();
            cx.bind_keys(key_bindings(mac))
        });
        let (editor, vcx) = cx.add_window_view(EditorState::new);
        editor.update_in(vcx, |e, window, cx| {
            e.set_text(&source, cx);
            e.set_cursor(start + 300, cx);
            e.focus(window, cx);
        });
        press("end", vcx);
        assert_eq!(selection(&editor, vcx), end..end, "mac {mac}");
        press("home", vcx);
        assert_eq!(selection(&editor, vcx), start..start, "mac {mac}");
        // From a row's end with upstream affinity too.
        if mac {
            press("cmd-right", vcx);
            assert!(editor.read_with(vcx, |e, _| e.caret_upstream()));
            press("home", vcx);
            assert_eq!(selection(&editor, vcx), start..start);
            press("cmd-right", vcx);
            press("end", vcx);
            assert_eq!(selection(&editor, vcx), end..end);
            assert!(!editor.read_with(vcx, |e, _| e.caret_upstream()));
        }
    }
}

#[gpui::test]
fn a_click_past_a_rows_end_keeps_the_caret_on_that_row(cx: &mut gpui::TestAppContext) {
    let source = mid_word_source();
    let (editor, cx) = mac_editor(cx, &source, source.len());
    let b = boundaries(&editor, cx, 0);
    let (origin, row_h, width) = editor.read_with(cx, |e, _| {
        let bounds = e.last_bounds.unwrap();
        (bounds.origin, e.line_h(0), bounds.size.width)
    });
    // Past the first row's end: its wrap boundary, upstream.
    cx.simulate_click(
        point(origin.x + width - px(1.), origin.y + row_h / 2.),
        gpui::Modifiers::none(),
    );
    assert_eq!(selection(&editor, cx), b[0]..b[0]);
    assert!(editor.read_with(cx, |e, _| e.caret_upstream()));
    assert_eq!(caret_at(&editor, cx).y, origin.y);
    // At the second row's start: the same offset, downstream.
    cx.simulate_click(
        point(origin.x, origin.y + row_h * 1.5),
        gpui::Modifiers::none(),
    );
    assert_eq!(selection(&editor, cx), b[0]..b[0]);
    assert!(!editor.read_with(cx, |e, _| e.caret_upstream()));
    assert_eq!(caret_at(&editor, cx).y, origin.y + row_h);
}

#[gpui::test]
fn up_and_down_walk_a_table_cells_own_rows(cx: &mut gpui::TestAppContext) {
    let long = "Payment is due after the services are provided. ".repeat(12);
    let long = long.trim_end();
    let source =
        format!("| A | B |\n| --- | --- |\n| {long} | x |\n| one<br>two | y |\nclosing line");
    let cell = source.find("Payment").unwrap();
    let (editor, cx) = mac_editor(cx, &source, cell);
    editor.update(cx, |e, cx| {
        e.set_markdown_style(markdown_syntax::search_style(), cx)
    });
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    let in_long = |at: usize| at >= cell && at <= cell + long.len();
    let y = |editor: &Entity<EditorState>, cx: &mut gpui::VisualTestContext| {
        editor.read_with(cx, |e, _| e.caret_table_cell_pos().unwrap().0)
    };
    // Down moves through the wrapped cell's rows before leaving it.
    let mut stops = vec![cell];
    loop {
        press("down", cx);
        let at = selection(&editor, cx).start;
        if !in_long(at) {
            break;
        }
        assert!(at > *stops.last().unwrap(), "{stops:?} then {at}");
        stops.push(at);
    }
    assert!(
        stops.len() >= 3,
        "the cell wraps into several rows: {stops:?}"
    );
    // Then the next table row's cell, through its `<br>` lines.
    let one = source.find("one").unwrap();
    let two = source.find("two").unwrap();
    assert_eq!(selection(&editor, cx).start, one);
    assert_eq!(y(&editor, cx), 3);
    press("down", cx);
    assert_eq!(selection(&editor, cx).start, two);
    press("down", cx);
    let closing = source.find("closing").unwrap();
    assert!(selection(&editor, cx).start >= closing);
    // Up retraces the rows: the `<br>` line above, then the long cell's last
    // row at the same x.
    editor.update(cx, |e, cx| e.set_cursor(two, cx));
    press("up", cx);
    assert_eq!(selection(&editor, cx).start, one);
    press("up", cx);
    assert_eq!(selection(&editor, cx).start, *stops.last().unwrap());
}

/// A table whose first body cell is `cell`, laid out with Markdown styling,
/// the caret at `at` bytes into that cell.
fn table_editor<'a>(
    cx: &'a mut gpui::TestAppContext,
    cell: &str,
    at: usize,
) -> (
    Entity<EditorState>,
    &'a mut gpui::VisualTestContext,
    String,
    usize,
) {
    let source = format!("| A | B |\n| --- | --- |\n| {cell} | y |\nclosing line");
    let start = source.find(cell).unwrap();
    let (editor, cx) = mac_editor(cx, &source, start + at);
    editor.update(cx, |e, cx| {
        e.set_markdown_style(markdown_syntax::search_style(), cx)
    });
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    (editor, cx, source, start)
}

#[gpui::test]
fn shift_down_selects_through_a_cells_rows(cx: &mut gpui::TestAppContext) {
    let (editor, cx, source, start) = table_editor(cx, "one<br>two", 0);
    let two = source.find("two").unwrap();
    let top = |editor: &Entity<EditorState>, cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| {
            editor.update(cx, |e, _| e.table_caret_bounds(window).unwrap().top())
        })
    };
    let first_top = top(&editor, cx);
    press("shift-down", cx);
    assert_eq!(selection(&editor, cx), start..two);
    // Autoscroll follows the selection's head in the cell's second row.
    let line_h = editor.read_with(cx, |e, _| e.font_size * LINE_HEIGHT_RATIO);
    let moved = top(&editor, cx) - first_top;
    assert!((f32::from(moved - line_h)).abs() < 0.5, "{moved:?}");
    press("shift-up", cx);
    assert_eq!(selection(&editor, cx), start..start);
}

#[gpui::test]
fn a_cells_rows_keep_the_goal_column_and_report_their_bounds(cx: &mut gpui::TestAppContext) {
    let (editor, cx, source, start) = table_editor(cx, "abcdefghij<br>x<br>abcdefghij<br>xyz", 8);
    let third = source.rfind("abcdefghij").unwrap();
    let top = |editor: &Entity<EditorState>, cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| {
            editor.update(cx, |e, _| e.table_caret_bounds(window).unwrap().top())
        })
    };
    let first_top = top(&editor, cx);
    press("down", cx);
    assert_eq!(
        selection(&editor, cx).start,
        source.find("x<br>").unwrap() + 1
    );
    // Through the short line and back out at the column it started from.
    press("down", cx);
    assert_eq!(selection(&editor, cx), third + 8..third + 8);
    // The bounds autoscroll follows are the cell row's, two rows down.
    let line_h = editor.read_with(cx, |e, _| e.font_size * LINE_HEIGHT_RATIO);
    let moved = top(&editor, cx) - first_top;
    assert!((f32::from(moved - line_h * 2.)).abs() < 0.5, "{moved:?}");
    assert!(start < third);
}

#[gpui::test]
fn a_click_resets_a_cells_goal_column(cx: &mut gpui::TestAppContext) {
    let (editor, cx, source, _) = table_editor(cx, "abcdefghij<br>x<br>abcdefghij", 8);
    let after_x = source.find("x<br>").unwrap() + 1;
    let third = source.rfind("abcdefghij").unwrap();
    press("down", cx);
    assert_eq!(selection(&editor, cx), after_x..after_x);
    // Clicking where the caret already is makes its column the new goal.
    let at =
        cx.update(|window, cx| editor.update(cx, |e, _| e.table_caret_bounds(window).unwrap()));
    cx.simulate_click(
        point(at.left() + px(1.), at.center().y),
        gpui::Modifiers::none(),
    );
    assert_eq!(selection(&editor, cx), after_x..after_x);
    press("down", cx);
    assert_eq!(selection(&editor, cx), third + 1..third + 1);
}

#[gpui::test]
fn a_cells_right_to_left_br_line_is_a_row_too(cx: &mut gpui::TestAppContext) {
    let (editor, cx, source, _) = table_editor(cx, "one<br>שלום", 0);
    let hebrew = source.find("שלום").unwrap();
    press("down", cx);
    let at = selection(&editor, cx).start;
    assert!(at >= hebrew && at <= hebrew + "שלום".len(), "{at}");
}

#[gpui::test]
fn cmd_right_after_a_click_past_a_space_broken_rows_end_stays(cx: &mut gpui::TestAppContext) {
    let source = "word ".repeat(400);
    let source = source.trim_end().to_string();
    let (editor, cx) = mac_editor(cx, &source, 0);
    let (origin, row_h, width) = editor.read_with(cx, |e, _| {
        let bounds = e.last_bounds.unwrap();
        (bounds.origin, e.line_h(0), bounds.size.width)
    });
    cx.simulate_click(
        point(origin.x + width - px(1.), origin.y + row_h / 2.),
        gpui::Modifiers::none(),
    );
    let clicked = selection(&editor, cx);
    assert_eq!(caret_at(&editor, cx).y, origin.y);
    press("cmd-right", cx);
    assert_eq!(selection(&editor, cx), clicked);
    assert_eq!(caret_at(&editor, cx).y, origin.y);
}

#[gpui::test]
fn ime_bounds_at_the_caret_follow_its_affinity(cx: &mut gpui::TestAppContext) {
    let source = mid_word_source();
    let (editor, cx) = mac_editor(cx, &source, 0);
    let b = boundaries(&editor, cx, 0);
    let ime_top = |editor: &Entity<EditorState>, cx: &mut gpui::VisualTestContext| {
        editor.update_in(cx, |e, window, cx| {
            let bounds = e.last_bounds.unwrap();
            e.bounds_for_range(b[0]..b[0], bounds, window, cx)
                .unwrap()
                .top()
        })
    };
    // Downstream: the lower row, where the caret draws.
    editor.update(cx, |e, cx| e.set_cursor(b[0], cx));
    assert_eq!(ime_top(&editor, cx), caret_at(&editor, cx).y);
    // Upstream: the upper row.
    editor.update(cx, |e, cx| e.set_cursor(0, cx));
    press("cmd-right", cx);
    assert_eq!(ime_top(&editor, cx), caret_at(&editor, cx).y);
}
