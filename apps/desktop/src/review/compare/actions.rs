//! What Side by side, the Original, the Pseudonymized and the Reply views
//! do: opening, switching and closing them, following every change to
//! the replacements, and the view switcher waking and resting.
use super::{
    APPLIED_ID, CompareMode, CompareState, CopyFrom, SWITCHER_IDLE, SWITCHER_REACH, compare_texts,
    to_after, to_before,
};
use crate::document::DocumentView;
use gpui::{AppContext, Context, Focusable, Hsla, Pixels, SharedString, Window, px};
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
};

impl DocumentView {
    /// ⌘\ and View › Side by Side: Side by side, from any view; from Side
    /// by side, back to Edit.
    pub(crate) fn toggle_compare(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let side = self
            .compare
            .as_ref()
            .is_some_and(|state| state.mode == CompareMode::Side);
        if side || !self.has_applied_aliases() {
            self.close_compare(window, cx);
        } else if self.compare.is_some() {
            self.set_view_mode(Some(CompareMode::Side), window, cx);
        } else {
            self.open_compare(CompareMode::Side, window, cx);
        }
    }
    fn open_compare(&mut self, mode: CompareMode, window: &mut Window, cx: &mut Context<Self>) {
        let theme = self.theme.get();
        let pane = |scroll: &gpui::ScrollHandle,
                    from: CopyFrom,
                    window: &mut Window,
                    cx: &mut Context<Self>| {
            let view = cx.weak_entity();
            let editor = cx.new(|cx| {
                let mut editor = mdoc_editor::EditorState::new(window, cx);
                editor.set_markdown_style(crate::ui::style::markdown_style(theme), cx);
                editor.set_read_only(true, cx);
                editor.set_autoscroll(scroll.clone());
                super::check_hand_copies(&mut editor, view, from);
                editor
            });
            // A name clicked in either column selects its Replacements row.
            let subscription =
                cx.subscribe_in(&editor, window, move |this, editor, event, window, cx| {
                    let id = match event {
                        // "Replace N and Copy", offered in the Pseudonymized
                        // column only: its text is the editor's.
                        mdoc_editor::EditorEvent::CopyAction { range, label } => {
                            this.copy_action(from, editor, range.clone(), label, cx);
                            return;
                        }
                        mdoc_editor::EditorEvent::ActivateAnnotation(id) => *id,
                        mdoc_editor::EditorEvent::ActivateAnnotations(ids) => match ids.first() {
                            Some(id) => *id,
                            None => return,
                        },
                        _ => return,
                    };
                    if !this.pii.mapping.open {
                        this.pii.mapping.open_panel(window.focused(cx));
                    }
                    this.panel_select_mention(id, cx);
                });
            (editor, subscription)
        };
        let (left_scroll, right_scroll) = (gpui::ScrollHandle::new(), gpui::ScrollHandle::new());
        let (original, left) = pane(&left_scroll, CopyFrom::Original, window, cx);
        let (current, right) = pane(&right_scroll, CopyFrom::Pseudonymized, window, cx);
        let reply_scroll = gpui::ScrollHandle::new();
        let reply = cx.new(|cx| {
            let mut editor = mdoc_editor::EditorState::new(window, cx);
            editor.set_markdown_style(crate::ui::style::markdown_style(theme), cx);
            editor.set_read_only(true, cx);
            editor.set_autoscroll(reply_scroll.clone());
            // The restored reply holds originals.
            editor.set_copy_metadata(Some(crate::review::restored_clipboard::TAG.into()));
            editor
        });
        self.pii.dismiss_popup();
        let hid_preview = self.preview.hide();
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        self.compare = Some(CompareState {
            original,
            current,
            reply,
            left: left_scroll,
            right: right_scroll,
            reply_scroll,
            reply_key: None,
            last: Default::default(),
            focus,
            key: None,
            shown: HashMap::new(),
            before: String::new(),
            marks: Vec::new(),
            hid_preview,
            original_only: false,
            mode,
            _subscriptions: [left, right],
        });
        self.refresh_compare(window, cx);
        cx.notify();
    }
    /// The view switcher's choice: `None` is Edit.
    pub(crate) fn set_view_mode(
        &mut self,
        mode: Option<CompareMode>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.wake_switcher(cx);
        match mode {
            None => {
                if self.compare.is_some() {
                    self.close_compare(window, cx);
                }
            }
            Some(CompareMode::Reply) if !self.reply.exists() => {}
            Some(mode) if mode != CompareMode::Reply && !self.has_applied_aliases() => {}
            Some(mode) => {
                if self.compare.is_none() {
                    self.open_compare(mode, window, cx);
                }
                if let Some(state) = self.compare.as_mut() {
                    state.mode = mode;
                }
                self.refresh_compare(window, cx);
                cx.notify();
            }
        }
    }
    /// Show the view pill, then let it fade after [`SWITCHER_IDLE`] unless
    /// the pointer stays on it. Pointer, scroll, zoom and view changes call
    /// this; repeated calls only restart the idle timer.
    pub(crate) fn wake_switcher(&mut self, cx: &mut Context<Self>) {
        let shown = self.switcher.shown();
        self.switcher.awake = true;
        self.switcher.settle(shown, cx);
        self.switcher.idle = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SWITCHER_IDLE).await;
            let _ = this.update(cx, |this, cx| {
                let shown = this.switcher.shown();
                this.switcher.awake = false;
                this.switcher.idle = None;
                this.switcher.settle(shown, cx);
            });
        }));
    }
    /// The pointer near the bottom of the text wakes the pill.
    pub(crate) fn switcher_pointer(
        &mut self,
        position: gpui::Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let column = self.switcher.column.get();
        if column.contains(&position) && position.y > column.bottom() - px(SWITCHER_REACH) {
            self.wake_switcher(cx);
        }
    }

    pub(crate) fn close_compare(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(state) = self.compare.take()
            && state.hid_preview
        {
            self.preview.show();
        }
        // A popup opened by panel decisions meanwhile belongs to no visible text.
        self.pii.dismiss_popup();
        window.focus(&self.editor.read(cx).focus_handle(cx), cx);
        cx.notify();
    }
    /// Bring an open view up to date with the editor and its applied
    /// replacements; cheap when nothing changed. New replacements settle in and
    /// roll as in the editor, reverted ones fade out.
    pub(crate) fn refresh_compare(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.compare.as_ref() else {
            return;
        };
        // Focus handed back to the hidden editor (e.g. by closing the panel)
        // moves to the view, so Esc still reaches it.
        if self.editor.read(cx).focus_handle(cx).is_focused(window) {
            window.focus(&state.focus, cx);
        }
        // The Reply view without a reply goes back to the editor. Only the
        // showing view is refreshed: switching to Reply refreshes it then.
        if state.mode == CompareMode::Reply {
            if self.reply.exists() {
                self.refresh_reply_view(cx);
            } else {
                self.close_compare(window, cx);
            }
            return;
        }
        let editor = self.editor.read(cx);
        let key = Some((
            editor.revision(),
            self.pii.review.revision(),
            self.theme.get(),
        ));
        if key == state.key {
            return;
        }
        let text = editor.text();
        let applied: Vec<_> = self
            .pii
            .review
            .applied()
            .iter()
            .filter(|a| text.get(a.range.clone()) == Some(a.step.after.as_ref()))
            .collect();
        if applied.is_empty() {
            self.close_compare(window, cx);
            return;
        }
        let text = text.to_owned();
        let pairs: Vec<_> = applied
            .iter()
            .map(|a| (a.range.clone(), a.step.original()))
            .collect();
        let (before, marks) = compare_texts(&text, &pairs);
        let theme = self.theme.get();
        let blue = theme.applied();
        let gold = theme.proposed();
        // Original wording is gold, aliases blue, as in the editor's review.
        let annotation = |id, range, accent: Hsla| mdoc_editor::SourceAnnotation {
            id,
            range,
            color: Hsla { a: 0.16, ..accent },
            active_color: Hsla { a: 0.34, ..accent },
            border: Hsla { a: 0.5, ..accent },
            text_color: Some(accent),
            underline: None,
            dotted: false,
        };
        let mut shown = HashMap::with_capacity(marks.len());
        let (mut left_marks, mut right_marks) = (Vec::new(), Vec::new());
        let mut added = HashSet::new();
        let mut rolls: Vec<(Range<usize>, SharedString)> = Vec::new();
        for (i, before_range, after_range) in &marks {
            let a = applied[*i];
            let id = APPLIED_ID | a.id;
            match state.shown.get(&id) {
                None => {
                    added.insert(id);
                    rolls.push((after_range.clone(), a.step.original().to_owned().into()));
                }
                Some((alias, _)) if alias.as_str() != a.step.after.as_ref() => {
                    rolls.push((after_range.clone(), alias.clone().into()));
                }
                Some(_) => {}
            }
            shown.insert(id, (a.step.after.to_string(), before_range.clone()));
            left_marks.push(annotation(id, before_range.clone(), gold));
            right_marks.push(annotation(id, after_range.clone(), blue));
        }
        let spans: Vec<_> = marks.into_iter().map(|(_, b, a)| (b, a)).collect();
        // Reverted names go back to their original wording: a ghost of the
        // chip fades from both columns and the alias rolls away. The original
        // text stays put, so its old spans still hold.
        let mut ghosts = Vec::new();
        if state.before == before {
            for (id, (alias, range)) in &state.shown {
                if shown.contains_key(id) {
                    continue;
                }
                let start = to_after(range.start, &spans);
                let right = start..start + range.len();
                ghosts.push((range.clone(), right.clone()));
                rolls.push((right, alias.clone().into()));
            }
        }
        let animate = state.key.is_some() && !cx.reduce_motion();
        let surface = theme.pdf_style().bg;
        let fill =
            |range: Range<usize>, c: Hsla| (range, Hsla { a: 0.22, ..c }, Hsla { a: 0.7, ..c });
        // The alias settles from gold into blue; its original glows in gold.
        let settle = |ids: HashSet<u64>, from: Hsla, to: Hsla| mdoc_editor::AnnotationFlash {
            ids,
            from: (Hsla { a: 0.22, ..from }, Hsla { a: 0.6, ..from }),
            peak: (Hsla { a: 0.3, ..to }, Hsla { a: 0.85, ..to }),
            duration: std::time::Duration::from_millis(700),
            mark: None,
        };
        let left_ghosts: Vec<_> = ghosts.iter().map(|(l, _)| fill(l.clone(), gold)).collect();
        let right_ghosts: Vec<_> = ghosts.into_iter().map(|(_, r)| fill(r, blue)).collect();
        let (original, current) = (state.original.clone(), state.current.clone());
        original.update(cx, |editor, cx| {
            editor.set_markdown_style(crate::ui::style::markdown_style(theme), cx);
            if editor.text() != before {
                editor.set_text(before.clone(), cx);
            }
            let revision = editor.revision();
            editor.set_annotations(revision, left_marks, cx);
            if animate {
                editor.flash_annotations(settle(added.clone(), gold, gold), cx);
                editor.flash_ranges(
                    mdoc_editor::RangeFlash {
                        revision,
                        ranges: left_ghosts,
                        duration: std::time::Duration::from_millis(700),
                    },
                    cx,
                );
            }
        });
        current.update(cx, |editor, cx| {
            editor.set_markdown_style(crate::ui::style::markdown_style(theme), cx);
            if editor.text() != text {
                editor.set_text(text, cx);
            }
            let revision = editor.revision();
            editor.set_annotations(revision, right_marks, cx);
            if animate {
                editor.flash_annotations(settle(added, gold, blue), cx);
                editor.flash_ranges(
                    mdoc_editor::RangeFlash {
                        revision,
                        ranges: right_ghosts,
                        duration: std::time::Duration::from_millis(700),
                    },
                    cx,
                );
                editor.roll_text(
                    mdoc_editor::TextRoll {
                        revision,
                        items: rolls,
                        surface,
                        duration: std::time::Duration::from_millis(600),
                    },
                    cx,
                );
            }
        });
        let state = self.compare.as_mut().expect("checked above");
        state.key = key;
        state.shown = shown;
        state.before = before;
        state.marks = spans;
    }
    /// The Reply view follows the reply and the mapping; cheap
    /// when neither changed.
    fn refresh_reply_view(&mut self, cx: &mut Context<Self>) {
        self.refresh_reply();
        let Some(state) = self.compare.as_ref() else {
            return;
        };
        let key = Some((
            self.reply.revision(),
            self.pii.review.revision(),
            self.theme.get(),
        ));
        if key == state.reply_key {
            return;
        }
        let theme = self.theme.get();
        let text = self.reply.restored().text.clone();
        let marks = self.reply_annotations();
        state.reply.clone().update(cx, |editor, cx| {
            editor.set_markdown_style(crate::ui::style::markdown_style(theme), cx);
            if editor.text() != text {
                editor.set_text(text, cx);
            }
            let revision = editor.revision();
            editor.set_annotations(revision, marks, cx);
        });
        if let Some(state) = self.compare.as_mut() {
            state.reply_key = key;
        }
    }
    /// Scroll the Reply view to `offset` in the restored text and flash the
    /// token there. The editor scrolls once it has painted the text, so a
    /// view opened by this same click, not laid out yet, still reveals it.
    pub(in crate::review) fn reveal_in_reply(&mut self, offset: usize, cx: &mut Context<Self>) {
        let Some(state) = self.compare.as_ref() else {
            return;
        };
        state
            .reply
            .clone()
            .update(cx, |editor, cx| editor.reveal_offset(offset, cx));
        if !cx.reduce_motion() {
            let red = crate::ui::style::markdown_style(self.theme.get()).alert_caution;
            let id = super::restore::UNRESOLVED_ID | offset as u64;
            state.reply.clone().update(cx, |editor, cx| {
                let strong = (Hsla { a: 0.45, ..red }, Hsla { a: 0.95, ..red });
                editor.flash_annotations(
                    mdoc_editor::AnnotationFlash {
                        ids: HashSet::from([id]),
                        from: strong,
                        peak: strong,
                        duration: std::time::Duration::from_millis(900),
                        mark: None,
                    },
                    cx,
                );
            });
        }
        cx.notify();
    }
    /// Show the replacement at `offset` (in the editor text) in both columns:
    /// scroll it into view when it is outside, then mark and flash it.
    pub(in crate::review) fn reveal_in_compare(&mut self, offset: usize, cx: &mut Context<Self>) {
        let Some(state) = self.compare.as_ref() else {
            return;
        };
        let review = &self.pii.review;
        let id = review
            .applied()
            .iter()
            .find(|a| a.range.start == offset)
            .map(|a| APPLIED_ID | a.id);
        // A pending mention is plain text in both columns: a gold ghost
        // marks it instead.
        let pending = review
            .candidates()
            .iter()
            .find(|c| id.is_none() && c.range.start == offset)
            .map(|c| c.range.len());
        let left_offset = to_before(offset, &state.marks);
        let theme = self.theme.get();
        for (editor, scroll, at, accent) in [
            (&state.original, &state.left, left_offset, theme.proposed()),
            (&state.current, &state.right, offset, theme.applied()),
        ] {
            let viewport = scroll.bounds();
            if let Some(top) = editor.read(cx).offset_screen_top(at)
                && (top < viewport.top() || top + px(24.) > viewport.bottom())
            {
                let delta = top - viewport.top() - px(48.);
                let mut position = scroll.offset();
                position.y = (position.y - delta).clamp(-scroll.max_offset().y, px(0.));
                scroll.set_offset(position);
            }
            editor.update(cx, |editor, cx| {
                editor.set_active_annotation(id, cx);
                if let Some(len) = pending.filter(|_| !cx.reduce_motion()) {
                    let gold = theme.proposed();
                    let revision = editor.revision();
                    editor.flash_ranges(
                        mdoc_editor::RangeFlash {
                            revision,
                            ranges: vec![(
                                at..at + len,
                                Hsla { a: 0.4, ..gold },
                                Hsla { a: 0.95, ..gold },
                            )],
                            duration: std::time::Duration::from_millis(1200),
                        },
                        cx,
                    );
                }
                if let Some(id) = id.filter(|_| !cx.reduce_motion()) {
                    let strong = (Hsla { a: 0.45, ..accent }, Hsla { a: 0.95, ..accent });
                    editor.flash_annotations(
                        mdoc_editor::AnnotationFlash {
                            ids: HashSet::from([id]),
                            from: strong,
                            peak: strong,
                            duration: std::time::Duration::from_millis(900),
                            mark: None,
                        },
                        cx,
                    );
                }
            });
        }
        // Both columns moved on purpose; neither leads the next frame.
        state
            .last
            .set((state.left.offset().y, state.right.offset().y));
        cx.notify();
    }
}
