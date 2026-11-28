//! The shortcuts sheet's actions: opening and closing it, with focus going
//! back where it was, and scrolling its list with the keys.

use super::{FILTER, LINE, ShortcutsSheet};
use crate::document::markdown_search::{SearchInput, SearchInputEvent};
use crate::workspace::Workspace;
use gpui::{Context, Focusable, KeyDownEvent, ScrollHandle, Window, point, prelude::*, px};

impl Workspace {
    /// ⌘/ (Ctrl+/), `?` outside text fields, the palette's and the Help
    /// menu's Keyboard Shortcuts: open the sheet, or close it when open.
    pub(in crate::workspace) fn toggle_shortcuts(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.shell.shortcuts.is_some() {
            self.close_shortcuts(window, cx);
            return;
        }
        let filter = cx.new(|cx| {
            SearchInput::new(cx)
                .with_theme(self.theme.clone())
                .with_key_context(FILTER)
                .with_placeholder(super::super::text::FILTER_SHORTCUTS)
        });
        let changes = cx.subscribe(&filter, |this, _, _: &SearchInputEvent, cx| {
            if let Some(sheet) = &this.shell.shortcuts {
                sheet.scroll.set_offset(point(px(0.), px(0.)));
            }
            cx.notify();
        });
        let return_focus = window.focused(cx);
        window.focus(&filter.read(cx).focus_handle(cx), cx);
        let focus = cx.focus_handle();
        // A command that moves focus (⌘N, Ctrl+Tab, Find…) closes the sheet
        // rather than acting behind it.
        // Switching to another app is not one of them.
        let focus_out = cx.on_focus_out(&focus, window, |this, _, window, cx| {
            if window.is_window_active() && this.shell.shortcuts.take().is_some() {
                cx.notify();
            }
        });
        self.shell.shortcuts = Some(ShortcutsSheet {
            filter,
            focus,
            list: cx.focus_handle().tab_stop(true),
            scroll: ScrollHandle::new(),
            return_focus,
            _changes: changes,
            _focus_out: focus_out,
        });
        cx.notify();
    }

    pub(in crate::workspace) fn close_shortcuts(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(sheet) = self.shell.shortcuts.take() {
            match sheet.return_focus {
                Some(focus) => window.focus(&focus, cx),
                None => self.focus_active(window, cx),
            }
        }
        cx.notify();
    }

    /// The list scrolls with ↑ ↓, Page Up / Down, Home and End.
    pub(super) fn scroll_shortcuts(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let Some(sheet) = &self.shell.shortcuts else {
            return;
        };
        if event.keystroke.modifiers.modified() {
            return;
        }
        let page = (sheet.scroll.bounds().size.height - px(LINE)).max(px(LINE));
        let max = sheet.scroll.max_offset().y;
        let y = -sheet.scroll.offset().y;
        let y = match event.keystroke.key.as_str() {
            "up" => y - px(LINE),
            "down" => y + px(LINE),
            "pageup" => y - page,
            "pagedown" => y + page,
            "home" => px(0.),
            "end" => max,
            _ => return,
        };
        sheet
            .scroll
            .set_offset(point(px(0.), -y.clamp(px(0.), max.max(px(0.)))));
        cx.stop_propagation();
        cx.notify();
    }
}
