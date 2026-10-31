//! Restore in place (ADR 0078): the Share panel's "AI reply" box restores a
//! pasted reply at once, with Restore's matching rules (ADR 0041), and the
//! view switcher's read-only Reply segment shows the result, following the
//! mapping live. One reply per document: kept in the case file when the tab
//! has a case, otherwise only while the tab is open.
use super::{compare::CompareMode, restore::UNRESOLVED_ID};
use crate::{document_view::DocumentView, workspace};
use gpui::{Context, Entity, Focusable, Window, prelude::*};
use mdoc_pii::restore::{ExportedIdentity, RestoreMap, RestoredText};
use std::ops::Range;

/// What the document view carries out for a reply-box transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReplyEffect {
    /// Keep the reply in the tab's case, or remove it there.
    Store,
    /// The box holds a changed reply: show the Reply view.
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
    /// Whether the mapping of the last restoration had identities to restore.
    restorable: bool,
    /// The mapping the reply's alias tokens were last read against, by
    /// identity id, and its review revision: when an identity's alias
    /// changes, the reply's tokens for it follow (ADR 0078).
    bound: Vec<(u64, ExportedIdentity)>,
    bound_at: Option<u64>,
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

    /// The user typed, pasted or cleared the box. Any change that leaves a
    /// reply shows the Reply view unless it is already showing, a paste over
    /// an earlier reply included; clearing leaves it.
    pub(crate) fn edited(&mut self, text: &str, showing_reply: bool) -> Vec<ReplyEffect> {
        if text == self.text {
            return Vec::new();
        }
        let had = self.exists();
        self.set(text.to_owned());
        let mut effects = vec![ReplyEffect::Store];
        if self.exists() {
            if !showing_reply {
                effects.push(ReplyEffect::ShowReply);
            }
        } else if had {
            effects.push(ReplyEffect::LeaveReply);
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

    /// Read the reply's alias tokens against the mapping at `review` (its
    /// revision) from now on, as when it is pasted or loaded.
    pub(crate) fn bind(&mut self, review: u64, mapping: Vec<(u64, ExportedIdentity)>) {
        self.bound = mapping;
        self.bound_at = Some(review);
    }

    /// The mapping moved on to `review`: rewrite the reply's alias tokens
    /// whose identity now has another alias, so the reply keeps naming the
    /// same people whatever renumbered them (a join, a case-wide rename, an
    /// undo of either). The reply stays in alias form; nothing restored is
    /// kept. Returns whether the reply changed.
    pub(crate) fn follow(
        &mut self,
        review: u64,
        mapping: impl FnOnce() -> Vec<(u64, ExportedIdentity)>,
    ) -> bool {
        if !self.exists() {
            self.bound.clear();
            self.bound_at = None;
            return false;
        }
        if self.bound_at == Some(review) {
            return false;
        }
        let before = std::mem::take(&mut self.bound);
        self.bind(review, mapping());
        let aliases: Vec<_> = before
            .iter()
            .map(|(id, old)| {
                self.bound
                    .iter()
                    .find(|(now, _)| now == id)
                    .map(|(_, now)| &now.alias)
                    .filter(|alias| **alias != old.alias)
                    .cloned()
            })
            .collect();
        if aliases.iter().all(Option::is_none) {
            return false;
        }
        let map = RestoreMap::new(before.into_iter().map(|(_, e)| e).collect());
        match map.realias(&self.text, &aliases) {
            Some(text) => {
                self.set(text);
                true
            }
            None => false,
        }
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
            let map = map();
            self.restorable = !map.is_empty();
            map.restore_in_place(&self.text)
        } else {
            self.restorable = false;
            RestoredText::default()
        };
        if restored == self.restored {
            return false;
        }
        self.restored = restored;
        self.next = self.next.min(self.restored.unresolved.len());
        true
    }

    /// Whether Copy reply and Open as tab have anything to restore: a reply,
    /// and a mapping with identities (refresh first).
    pub(crate) fn restorable(&self) -> bool {
        self.exists() && self.restorable
    }

    /// The last restoration (refresh first).
    pub(crate) fn restored(&self) -> &RestoredText {
        &self.restored
    }

    /// "N values restored · M unrecognized": distinct identities restored
    /// and distinct unknown tokens, as Anoni counts (ADR 0078). Next
    /// unrecognised still visits every occurrence.
    pub(crate) fn summary(&self) -> String {
        let restored = self.restored.values;
        let unresolved = self.unrecognised();
        format!(
            "{restored} {} restored · {unresolved} unrecognized",
            if restored == 1 { "value" } else { "values" }
        )
    }

    /// Distinct unresolved tokens in the restored text.
    pub(crate) fn unrecognised(&self) -> usize {
        let text = &self.restored.text;
        let tokens: std::collections::HashSet<_> = self
            .restored
            .unresolved
            .iter()
            .filter_map(|range| text.get(range.clone()))
            .collect();
        tokens.len()
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
    /// Share tab first draws. It is plain text, so a paste is kept exactly
    /// as the AI wrote it: no Markdown paste or Enter rules. The caret stays
    /// in view in its scrolling box.
    pub(crate) fn ensure_reply_box(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.reply_box.is_some() {
            return;
        }
        let text = self.reply.text().to_owned();
        let scroll = self.reply_scroll.clone();
        let editor = cx.new(|cx| {
            let mut editor = mdoc_editor::EditorState::new(window, cx)
                .with_placeholder("Paste the AI's reply here");
            editor.set_plain_text(true);
            editor.set_autoscroll(scroll);
            editor.set_text(text, cx);
            // Unlike the document editor, this field participates in the
            // Share panel's Tab order. Clones share the focus metadata.
            editor.focus_handle(cx).tab_index(0).tab_stop(true);
            editor
        });
        let subscription = cx.subscribe_in(&editor, window, |this, editor, event, window, cx| {
            if let mdoc_editor::EditorEvent::Changed = event {
                let text = editor.read(cx).text().to_owned();
                let effects = this.reply.edited(&text, this.showing_reply());
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
        let effects = self.reply.edited(text, self.showing_reply());
        self.carry_out_reply(effects, window, cx);
    }

    /// Whether the Reply view is the one open.
    fn showing_reply(&self) -> bool {
        self.compare.as_ref().map(|c| c.mode) == Some(CompareMode::Reply)
    }

    /// The case gave this reopened member its reply (ADR 0078).
    pub(crate) fn load_reply(&mut self, text: String, cx: &mut Context<Self>) {
        self.reply.loaded(text);
        self.bind_reply();
        self.show_reply_text(cx);
        cx.notify();
    }

    /// The box shows the reply as it is now.
    fn show_reply_text(&mut self, cx: &mut Context<Self>) {
        let text = self.reply.text().to_owned();
        if let Some(editor) = self.reply_box().cloned()
            && editor.read(cx).text() != text
        {
            editor.update(cx, |editor, cx| editor.set_text(text, cx));
        }
    }

    fn store_reply(&self, cx: &mut Context<Self>) {
        if self.case.link().is_some() {
            cx.emit(workspace::TabEvent::Case(workspace::CaseEvent::Reply));
        }
    }

    fn bind_reply(&mut self) {
        let review = &self.pii.review;
        self.reply
            .bind(review.revision(), review.export_identities_with_ids());
    }

    /// The mapping may have changed: once this update settles (a join or an
    /// undo commits its identities last), the reply's alias tokens follow
    /// any identity whose alias changed, and the case keeps the result.
    pub(crate) fn follow_reply_aliases_soon(&mut self, cx: &mut Context<Self>) {
        if !self.reply.exists() {
            return;
        }
        let weak = cx.entity().downgrade();
        cx.defer(move |cx| {
            let _ = weak.update(cx, |this, cx| this.follow_reply_aliases(cx));
        });
    }

    fn follow_reply_aliases(&mut self, cx: &mut Context<Self>) {
        let review = &self.pii.review;
        if !self
            .reply
            .follow(review.revision(), || review.export_identities_with_ids())
        {
            return;
        }
        self.show_reply_text(cx);
        self.store_reply(cx);
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
                    // A new reply's tokens name today's identities.
                    self.bind_reply();
                    self.store_reply(cx);
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
                    if self.showing_reply() {
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
        if !self.reply.restorable() {
            return;
        }
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(
            self.reply.restored().text.clone(),
        ));
        self.copied_for_ai = false;
        self.copied_reply = true;
        self.show_copy_feedback(cx);
    }

    /// Open as tab: the restore tab Paste AI Response makes, from this reply,
    /// with its guard when no identities are left to restore.
    pub(crate) fn open_reply_as_tab(&mut self, cx: &mut Context<Self>) {
        if !self.reply.exists() || !self.has_identities_to_restore("Open as tab", cx) {
            return;
        }
        let text = self.reply.text().to_owned();
        self.open_restore_tab(text, cx);
    }

    /// Next unrecognised: show the Reply view at the next unresolved token.
    pub(crate) fn next_unrecognised(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.refresh_reply();
        let Some(range) = self.reply.next_unresolved() else {
            return;
        };
        if !self.showing_reply() {
            self.set_view_mode(Some(CompareMode::Reply), window, cx);
        }
        let id = UNRESOLVED_ID | range.start as u64;
        if let Some(editor) = self.compare.as_ref().map(|c| c.reply_editor().clone()) {
            editor.update(cx, |editor, cx| editor.set_active_annotation(Some(id), cx));
        }
        self.reveal_in_reply(range.start, cx);
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
            reply.edited("PERSON_1", false),
            [ReplyEffect::Store, ReplyEffect::ShowReply]
        );
        // Typing on while the Reply view shows only stores.
        assert_eq!(reply.edited("PERSON_1 left", true), [ReplyEffect::Store]);
        assert_eq!(reply.edited("PERSON_1 left", false), []);
        // A different reply pasted over this one while on Edit shows it.
        assert_eq!(
            reply.edited("PERSON_1 came back", false),
            [ReplyEffect::Store, ReplyEffect::ShowReply]
        );
        assert_eq!(reply.stored().as_deref(), Some("PERSON_1 came back"));
        assert_eq!(
            reply.edited("  ", true),
            [ReplyEffect::Store, ReplyEffect::LeaveReply]
        );
        assert!(reply.stored().is_none(), "a blank box keeps nothing");
        assert_eq!(reply.edited("", false), [ReplyEffect::Store]);
        reply.loaded("PERSON_1".into());
        assert!(reply.exists(), "a kept reply comes back without effects");
    }

    #[test]
    fn the_reply_follows_its_identities_through_a_swap_and_back() {
        let person = |alias: &str, name: &str| ExportedIdentity {
            alias: alias.into(),
            previous: vec![],
            category: Category::Person,
            custom: false,
            originals: vec![name.into()],
        };
        let mapping =
            |anna: &str, boris: &str| vec![(1, person(anna, "Anna")), (2, person(boris, "Boris"))];
        let mut reply = ReplyBox::default();
        reply.edited("PERSON_1 owes PERSON_2.", false);
        reply.bind(1, mapping("PERSON_1", "PERSON_2"));
        assert!(!reply.follow(1, || unreachable!("same revision")));
        assert!(!reply.follow(2, || mapping("PERSON_1", "PERSON_2")));
        assert!(reply.follow(3, || mapping("PERSON_2", "PERSON_1")));
        assert_eq!(reply.text(), "PERSON_2 owes PERSON_1.");
        assert!(reply.follow(4, || mapping("PERSON_1", "PERSON_2")));
        assert_eq!(reply.text(), "PERSON_1 owes PERSON_2.");
        // A forgotten identity's tokens stay as they are.
        assert!(!reply.follow(5, || vec![(2, person("PERSON_2", "Boris"))]));
        assert_eq!(reply.text(), "PERSON_1 owes PERSON_2.");
    }

    #[test]
    fn the_summary_counts_distinct_values_and_next_unrecognised_wraps() {
        let mut reply = ReplyBox::default();
        reply.edited(
            "PERSON_1 and PERSON_1 met PERSON_8, PERSON_9 and PERSON_8.",
            false,
        );
        assert!(reply.refresh(1, map));
        assert!(!reply.refresh(1, || unreachable!("cached")));
        // One identity twice is one value; PERSON_8 twice is one unknown.
        assert_eq!(reply.summary(), "1 value restored · 2 unrecognized");
        let text = reply.restored().text.clone();
        let mut seen = Vec::new();
        for _ in 0..4 {
            let range = reply.next_unresolved().unwrap();
            seen.push(text[range].to_string());
        }
        // Next unrecognised visits every occurrence, then wraps.
        assert_eq!(seen, ["PERSON_8", "PERSON_9", "PERSON_8", "PERSON_8"]);
        reply.edited("PERSON_1", false);
        reply.refresh(1, map);
        assert_eq!(reply.summary(), "1 value restored · 0 unrecognized");
        assert_eq!(reply.next_unresolved(), None);
    }
}
