//! Restore in place (ADR 0078): the Share panel's "AI reply" box restores a
//! pasted reply at once, with Restore's matching rules (ADR 0041), and the
//! view switcher's read-only Reply segment shows the result, following the
//! mapping live. One reply per document: kept in the case file when the tab
//! has a case, otherwise only while the tab is open.
use super::{compare::CompareMode, restore::UNRESOLVED_ID};
use crate::{document_view::DocumentView, workspace};
use gpui::{Context, Entity, Focusable, Window, prelude::*};
use mdoc_pii::restore::{RestoreMap, RestoredText};
use std::ops::Range;

/// What the document view carries out for a reply-box transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReplyEffect {
    /// Keep the reply in the tab's case, or remove it there.
    Store,
    /// The box was filled: show the Reply view.
    ShowReply,
    /// The box was cleared: leave the Reply view.
    LeaveReply,
}

/// The tab's AI reply as pasted, and its restoration against the tab's
/// current mapping.
#[derive(Default)]
pub(crate) struct ReplyBox {
    text: String,
    revision: u64,
    /// The reply and review revisions `restored` was made from.
    made_from: Option<(u64, u64)>,
    restored: RestoredText,
    /// The unresolved token Next unrecognised moves to after the last.
    next: usize,
}

impl ReplyBox {
    /// The reply as pasted, aliases and all.
    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    /// Whether there is a reply: the Reply segment shows only then.
    pub(crate) fn exists(&self) -> bool {
        !self.text.trim().is_empty()
    }

    /// Bumped on every change of the reply text.
    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    /// What the case keeps for this member: the reply, or nothing.
    pub(crate) fn stored(&self) -> Option<String> {
        self.exists().then(|| self.text.clone())
    }

    /// The user typed, pasted or cleared the box.
    pub(crate) fn edited(&mut self, text: &str) -> Vec<ReplyEffect> {
        if text == self.text {
            return Vec::new();
        }
        let had = self.exists();
        self.set(text.to_owned());
        let mut effects = vec![ReplyEffect::Store];
        match (had, self.exists()) {
            (false, true) => effects.push(ReplyEffect::ShowReply),
            (true, false) => effects.push(ReplyEffect::LeaveReply),
            _ => {}
        }
        effects
    }

    /// A reopened case member gets its kept reply back: nothing to store,
    /// and the view stays as it is.
    pub(crate) fn loaded(&mut self, text: String) {
        if text != self.text {
            self.set(text);
        }
    }

    fn set(&mut self, text: String) {
        self.text = text;
        self.revision += 1;
        self.next = 0;
    }

    /// Restore against the mapping at `review` (its revision), unless the
    /// reply and mapping are what the last restoration used. Returns whether
    /// the restoration changed.
    pub(crate) fn refresh(&mut self, review: u64, map: impl FnOnce() -> RestoreMap) -> bool {
        let key = Some((self.revision, review));
        if self.made_from == key {
            return false;
        }
        self.made_from = key;
        let restored = if self.exists() {
            map().restore_in_place(&self.text)
        } else {
            RestoredText::default()
        };
        if restored == self.restored {
            return false;
        }
        self.restored = restored;
        self.next = self.next.min(self.restored.unresolved.len());
        true
    }

    /// The last restoration (refresh first).
    pub(crate) fn restored(&self) -> &RestoredText {
        &self.restored
    }

    /// "N values restored · M unrecognised".
    pub(crate) fn summary(&self) -> String {
        let restored = self.restored.restored.len();
        let unresolved = self.restored.unresolved.len();
        format!(
            "{restored} {} restored · {unresolved} unrecognised",
            if restored == 1 { "value" } else { "values" }
        )
    }

    /// The unresolved token Next unrecognised last moved to.
    #[cfg(test)]
    pub(crate) fn current_unresolved(&self) -> Option<Range<usize>> {
        let at = self.next.checked_sub(1)?;
        self.restored.unresolved.get(at).cloned()
    }

    /// The next unresolved token, wrapping after the last.
    pub(crate) fn next_unresolved(&mut self) -> Option<Range<usize>> {
        let tokens = &self.restored.unresolved;
        if tokens.is_empty() {
            return None;
        }
        let at = self.next % tokens.len();
        self.next = at + 1;
        Some(tokens[at].clone())
    }
}

impl DocumentView {
    /// The box is an editor so a reply keeps its lines; it is made when the
    /// Share tab first draws.
    pub(crate) fn ensure_reply_box(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.reply_box.is_some() {
            return;
        }
        let theme = self.theme.get();
        let text = self.reply.text().to_owned();
        let editor = cx.new(|cx| {
            let mut editor = mdoc_editor::EditorState::new(window, cx)
                .with_placeholder("Paste the AI's reply here");
            editor.set_markdown_style(crate::style::markdown_style(theme), cx);
            editor.set_text(text, cx);
            // Unlike the document editor, this field participates in the
            // Share panel's Tab order. Clones share the focus metadata.
            editor.focus_handle(cx).tab_index(0).tab_stop(true);
            editor
        });
        let subscription = cx.subscribe_in(&editor, window, |this, editor, event, window, cx| {
            if let mdoc_editor::EditorEvent::Changed = event {
                let text = editor.read(cx).text().to_owned();
                let effects = this.reply.edited(&text);
                this.carry_out_reply(effects, window, cx);
            }
        });
        self.reply_box = Some((editor, subscription));
    }

    pub(crate) fn reply_box(&self) -> Option<&Entity<mdoc_editor::EditorState>> {
        self.reply_box.as_ref().map(|(editor, _)| editor)
    }

    /// Fill the box, as a paste does (tests and headless renders).
    #[cfg(any(
        test,
        all(
            debug_assertions,
            feature = "headless-renders",
            any(target_os = "macos", target_os = "windows")
        )
    ))]
    pub(crate) fn fill_reply(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.ensure_reply_box(window, cx);
        if let Some(editor) = self.reply_box().cloned() {
            editor.update(cx, |editor, cx| editor.set_text(text.to_owned(), cx));
        }
        let effects = self.reply.edited(text);
        self.carry_out_reply(effects, window, cx);
    }

    /// The case gave this reopened member its reply (ADR 0078).
    pub(crate) fn load_reply(&mut self, text: String, cx: &mut Context<Self>) {
        self.reply.loaded(text.clone());
        if let Some(editor) = self.reply_box().cloned()
            && editor.read(cx).text() != text
        {
            editor.update(cx, |editor, cx| editor.set_text(text, cx));
        }
        cx.notify();
    }

    fn carry_out_reply(
        &mut self,
        effects: Vec<ReplyEffect>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for effect in effects {
            match effect {
                ReplyEffect::Store => {
                    if self.case.link().is_some() {
                        cx.emit(workspace::TabEvent::Case(workspace::CaseEvent::Reply));
                    }
                }
                ReplyEffect::ShowReply => {
                    // Typing goes on in the box while the view opens beside it.
                    let focused = window.focused(cx);
                    self.set_view_mode(Some(CompareMode::Reply), window, cx);
                    if let Some(focus) = focused {
                        window.focus(&focus, cx);
                    }
                }
                ReplyEffect::LeaveReply => {
                    if self.compare.as_ref().map(|c| c.mode) == Some(CompareMode::Reply) {
                        let focused = window.focused(cx);
                        self.close_compare(window, cx);
                        if let Some(focus) = focused {
                            window.focus(&focus, cx);
                        }
                    }
                }
            }
        }
        cx.notify();
    }

    /// Bring the reply's restoration up to date with the reply and mapping.
    pub(crate) fn refresh_reply(&mut self) -> bool {
        let review = &self.pii.review;
        self.reply.refresh(review.revision(), || {
            RestoreMap::new(review.export_identities())
        })
    }

    /// Copy reply: the restored text.
    pub(crate) fn copy_reply(&mut self, cx: &mut Context<Self>) {
        self.refresh_reply();
        if !self.reply.exists() {
            return;
        }
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(
            self.reply.restored().text.clone(),
        ));
        self.copied_for_ai = false;
        self.copied_reply = true;
        self.show_copy_feedback(cx);
    }

    /// Open as tab: the restore tab Paste AI Response makes, from this reply.
    pub(crate) fn open_reply_as_tab(&mut self, cx: &mut Context<Self>) {
        if self.reply.exists() {
            let text = self.reply.text().to_owned();
            self.open_restore_tab(text, cx);
        }
    }

    /// Next unrecognised: show the Reply view at the next unresolved token.
    pub(crate) fn next_unrecognised(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.refresh_reply();
        let Some(range) = self.reply.next_unresolved() else {
            return;
        };
        if self.compare.as_ref().map(|c| c.mode) != Some(CompareMode::Reply) {
            self.set_view_mode(Some(CompareMode::Reply), window, cx);
        }
        let id = UNRESOLVED_ID | range.start as u64;
        if let Some(editor) = self.compare.as_ref().map(|c| c.reply_editor().clone()) {
            editor.update(cx, |editor, cx| editor.set_active_annotation(Some(id), cx));
        }
        // The view lays out first when it has just opened.
        let weak = cx.entity().downgrade();
        window.on_next_frame(move |_, cx| {
            let _ = weak.update(cx, |this, cx| {
                this.reveal_in_reply(range.start, cx);
                cx.notify();
            });
        });
        cx.notify();
    }

    /// The reply editor's annotations: restored values in the original's
    /// gold, unresolved tokens red as in the restore tab.
    pub(super) fn reply_annotations(&self) -> Vec<mdoc_editor::SourceAnnotation> {
        let theme = self.theme.get();
        let gold = theme.proposed();
        let red = crate::style::markdown_style(theme).alert_caution;
        let mark = |id, range, accent: gpui::Hsla| mdoc_editor::SourceAnnotation {
            id,
            range,
            color: gpui::Hsla { a: 0.16, ..accent },
            active_color: gpui::Hsla { a: 0.34, ..accent },
            border: gpui::Hsla { a: 0.5, ..accent },
            text_color: Some(accent),
            underline: None,
            dotted: false,
        };
        let restored = self.reply.restored();
        restored
            .restored
            .iter()
            .enumerate()
            .map(|(i, (range, _))| mark(i as u64, range.clone(), gold))
            .chain(
                restored
                    .unresolved
                    .iter()
                    .map(|range| mark(UNRESOLVED_ID | range.start as u64, range.clone(), red)),
            )
            .collect()
    }

    /// Focus the box (a click on its padding).
    pub(crate) fn focus_reply_box(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = self.reply_box() {
            window.focus(&editor.read(cx).focus_handle(cx), cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdoc_pii::{Category, restore::ExportedIdentity};

    fn map() -> RestoreMap {
        RestoreMap::new(vec![ExportedIdentity {
            alias: "PERSON_1".into(),
            previous: vec![],
            category: Category::Person,
            custom: false,
            originals: vec!["Anna Petrova".into()],
        }])
    }

    #[test]
    fn filling_shows_the_reply_and_clearing_leaves_it() {
        let mut reply = ReplyBox::default();
        assert!(!reply.exists() && reply.stored().is_none());
        assert_eq!(
            reply.edited("PERSON_1"),
            [ReplyEffect::Store, ReplyEffect::ShowReply]
        );
        assert_eq!(reply.edited("PERSON_1 left"), [ReplyEffect::Store]);
        assert_eq!(reply.edited("PERSON_1 left"), []);
        assert_eq!(reply.stored().as_deref(), Some("PERSON_1 left"));
        assert_eq!(
            reply.edited("  "),
            [ReplyEffect::Store, ReplyEffect::LeaveReply]
        );
        assert!(reply.stored().is_none(), "a blank box keeps nothing");
        reply.loaded("PERSON_1".into());
        assert!(reply.exists(), "a kept reply comes back without effects");
    }

    #[test]
    fn the_summary_counts_and_next_unrecognised_wraps() {
        let mut reply = ReplyBox::default();
        reply.edited("PERSON_1 and PERSON_1 met PERSON_8 and PERSON_9.");
        assert!(reply.refresh(1, map));
        assert!(!reply.refresh(1, || unreachable!("cached")));
        assert_eq!(reply.summary(), "2 values restored · 2 unrecognised");
        let text = reply.restored().text.clone();
        let mut seen = Vec::new();
        for _ in 0..3 {
            let range = reply.next_unresolved().unwrap();
            seen.push(text[range].to_string());
        }
        assert_eq!(seen, ["PERSON_8", "PERSON_9", "PERSON_8"]);
        reply.edited("PERSON_1");
        reply.refresh(1, map);
        assert_eq!(reply.summary(), "1 value restored · 0 unrecognised");
        assert_eq!(reply.next_unresolved(), None);
    }
}
