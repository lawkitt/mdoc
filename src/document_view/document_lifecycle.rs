//! A document view's lifecycle (ADR 0049 document lifecycle): loading its
//! Markdown and whether that failed, whether it shows only a source still to
//! convert, whether its text has unsaved changes, whether it is a generated
//! conversion nobody edited yet or a blank Home placeholder, and whether a
//! dialog of its own is open. File identity and saves are the document
//! session's (`mdoc_store::document_session`); this owner only tracks the view.

/// What the document view carries out for a lifecycle transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LifecycleEffect {
    /// Nothing is unsaved: go on with what was asked.
    Proceed,
    /// Ask whether to save the unsaved changes first (ADR 0064).
    AskToSave,
    /// Ask for files to open.
    AskForFiles,
    /// Save to the document's own file.
    Write,
    /// Ask where to save.
    AskForPath,
    /// The document cannot be saved now; a close waiting on it is cancelled.
    CancelClose,
}

#[derive(Default)]
pub(crate) struct DocumentLifecycle {
    loading: bool,
    /// Bumped by every load; a load's result applies only to the latest.
    load_generation: u64,
    /// The last load failed; Retry loads again.
    unavailable: bool,
    /// A source shown before its conversion (or a non-Markdown file).
    source_only: bool,
    /// Unsaved changes, as of the last edit or save.
    dirty: bool,
    /// An automatic conversion nobody has edited: the session keeps its
    /// source instead of a Markdown file.
    generated_unedited: bool,
    /// A blank placeholder Home may replace (ADR 0048).
    blank_disposable: bool,
    /// One of this document's dialogs is open.
    prompting: bool,
    /// When the user last edited the text in this session (the shelf's
    /// order, ADR 0084).
    edited_at: Option<std::time::SystemTime>,
}

impl DocumentLifecycle {
    /// A new tab's view as its session record describes it: a Markdown
    /// file to load, a source to convert, or a blank Home placeholder.
    pub(crate) fn restore(&mut self, loading: bool, source_only: bool, blank_disposable: bool) {
        self.loading = loading;
        self.source_only = source_only;
        self.blank_disposable = blank_disposable;
    }

    pub(crate) fn loading(&self) -> bool {
        self.loading
    }

    pub(crate) fn unavailable(&self) -> bool {
        self.unavailable
    }

    /// Neither loading nor failed to load.
    pub(crate) fn ready(&self) -> bool {
        !self.loading && !self.unavailable
    }

    pub(crate) fn source_only(&self) -> bool {
        self.source_only
    }

    pub(crate) fn dirty(&self) -> bool {
        self.dirty
    }

    pub(crate) fn generated_unedited(&self) -> bool {
        self.generated_unedited
    }

    pub(crate) fn blank_disposable(&self) -> bool {
        self.blank_disposable
    }

    pub(crate) fn prompting(&self) -> bool {
        self.prompting
    }

    /// Start loading the tab's Markdown; returns the load's generation.
    pub(crate) fn begin_load(&mut self) -> u64 {
        self.load_generation = self.load_generation.wrapping_add(1);
        self.loading = true;
        self.unavailable = false;
        self.load_generation
    }

    /// The load for `generation` ended, `ok` or not. False when a newer
    /// load started meanwhile: its result is ignored.
    pub(crate) fn finish_load(&mut self, generation: u64, ok: bool) -> bool {
        if generation != self.load_generation {
            return false;
        }
        self.loading = false;
        if !ok {
            self.unavailable = true;
        }
        true
    }

    /// The tab shows a source still to convert.
    pub(crate) fn show_source(&mut self) {
        self.source_only = true;
    }

    /// The conversion replaced the document; an automatic one counts as
    /// generated and unedited.
    pub(crate) fn converted(&mut self, automatic: bool) {
        self.source_only = false;
        self.generated_unedited = automatic;
    }

    /// The user edited the text: it is no longer an untouched conversion
    /// or placeholder. The caller then rechecks `set_dirty`.
    pub(crate) fn edited(&mut self) {
        self.generated_unedited = false;
        self.blank_disposable = false;
        self.edited_at = Some(std::time::SystemTime::now());
    }

    /// When the user last edited the text in this session.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn edited_at(&self) -> Option<std::time::SystemTime> {
        self.edited_at
    }

    /// Whether the text differs from the saved text, rechecked.
    pub(crate) fn set_dirty(&mut self, dirty: bool) {
        self.dirty = dirty;
    }

    pub(crate) fn saved(&mut self) {
        self.generated_unedited = false;
        self.blank_disposable = false;
        self.dirty = false;
    }

    /// The user acted on the tab, so Home no longer replaces it.
    pub(crate) fn keep_blank(&mut self) {
        self.blank_disposable = false;
    }

    /// Something asked to replace or close the document; `dirty` is whether
    /// it has unsaved changes. Nothing while one of its dialogs is open.
    pub(crate) fn request(&mut self, dirty: bool) -> Option<LifecycleEffect> {
        if self.prompting {
            return None;
        }
        if !dirty {
            return Some(LifecycleEffect::Proceed);
        }
        self.prompting = true;
        Some(LifecycleEffect::AskToSave)
    }

    /// Open pressed. Nothing while one of the document's dialogs is open.
    pub(crate) fn open_files(&mut self) -> Option<LifecycleEffect> {
        if self.prompting {
            return None;
        }
        self.prompting = true;
        Some(LifecycleEffect::AskForFiles)
    }

    /// Save (`save_as` false) or Save As; `has_path` when the document has a
    /// file of its own. A document that is loading, failed to load, shows only
    /// a source, or has a dialog open cannot be saved.
    pub(crate) fn save(&mut self, save_as: bool, has_path: bool) -> LifecycleEffect {
        if self.prompting || self.loading || self.unavailable || self.source_only {
            return LifecycleEffect::CancelClose;
        }
        if !save_as && has_path {
            return LifecycleEffect::Write;
        }
        self.prompting = true;
        LifecycleEffect::AskForPath
    }

    /// The document's dialog was answered or dismissed.
    pub(crate) fn end_prompt(&mut self) {
        self.prompting = false;
    }

    #[cfg(test)]
    pub(crate) fn set_source_only(&mut self, source_only: bool) {
        self.source_only = source_only;
    }

    /// Put the view in a state directly, for tests of how it renders.
    #[cfg(test)]
    pub(crate) fn force(&mut self, loading: bool, unavailable: bool, source_only: bool) {
        self.loading = loading;
        self.unavailable = unavailable;
        self.source_only = source_only;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lifecycle(loading: bool, source_only: bool, blank: bool) -> DocumentLifecycle {
        let mut lifecycle = DocumentLifecycle::default();
        lifecycle.restore(loading, source_only, blank);
        lifecycle
    }

    #[test]
    fn only_the_latest_load_applies() {
        let mut lifecycle = lifecycle(true, false, false);
        assert!(lifecycle.loading() && !lifecycle.ready());
        let first = lifecycle.begin_load();
        let second = lifecycle.begin_load();
        assert!(!lifecycle.finish_load(first, true));
        assert!(lifecycle.loading());
        assert!(lifecycle.finish_load(second, false));
        assert!(lifecycle.unavailable() && !lifecycle.loading());
        // Retry clears the failure while it loads.
        let retry = lifecycle.begin_load();
        assert!(!lifecycle.unavailable());
        assert!(lifecycle.finish_load(retry, true));
        assert!(lifecycle.ready());
    }

    #[test]
    fn a_conversion_is_generated_until_edited_or_saved() {
        let mut lifecycle = lifecycle(false, true, false);
        assert!(lifecycle.source_only());
        lifecycle.converted(true);
        assert!(!lifecycle.source_only() && lifecycle.generated_unedited());
        lifecycle.edited();
        lifecycle.set_dirty(true);
        assert!(!lifecycle.generated_unedited() && lifecycle.dirty());
        lifecycle.converted(true);
        lifecycle.saved();
        assert!(!lifecycle.generated_unedited() && !lifecycle.dirty());
        lifecycle.converted(false);
        assert!(!lifecycle.generated_unedited());
        lifecycle.show_source();
        assert!(lifecycle.source_only());
    }

    #[test]
    fn a_blank_placeholder_stops_being_one_once_used() {
        let mut placeholder = lifecycle(false, false, true);
        assert!(placeholder.blank_disposable());
        placeholder.set_dirty(true);
        assert!(placeholder.blank_disposable());
        placeholder.edited();
        assert!(!placeholder.blank_disposable());
        let mut placeholder = lifecycle(false, false, true);
        placeholder.keep_blank();
        assert!(!placeholder.blank_disposable());
        let mut placeholder = lifecycle(false, false, true);
        placeholder.saved();
        assert!(!placeholder.blank_disposable());
    }

    #[test]
    fn unsaved_changes_are_asked_about_once() {
        let mut lifecycle = DocumentLifecycle::default();
        assert_eq!(lifecycle.request(false), Some(LifecycleEffect::Proceed));
        assert!(!lifecycle.prompting());
        assert_eq!(lifecycle.request(true), Some(LifecycleEffect::AskToSave));
        assert!(lifecycle.prompting());
        // While the dialog is open nothing else may ask.
        assert_eq!(lifecycle.request(true), None);
        assert_eq!(lifecycle.open_files(), None);
        assert_eq!(lifecycle.save(false, true), LifecycleEffect::CancelClose);
        lifecycle.end_prompt();
        assert_eq!(lifecycle.open_files(), Some(LifecycleEffect::AskForFiles));
        lifecycle.end_prompt();
        assert!(!lifecycle.prompting());
    }

    #[test]
    fn save_writes_its_file_or_asks_where_unless_it_cannot_save() {
        let mut lifecycle = DocumentLifecycle::default();
        assert_eq!(lifecycle.save(false, true), LifecycleEffect::Write);
        assert!(!lifecycle.prompting());
        assert_eq!(lifecycle.save(true, true), LifecycleEffect::AskForPath);
        assert!(lifecycle.prompting());
        lifecycle.end_prompt();
        assert_eq!(lifecycle.save(false, false), LifecycleEffect::AskForPath);
        lifecycle.end_prompt();
        for (loading, unavailable, source_only) in [
            (true, false, false),
            (false, true, false),
            (false, false, true),
        ] {
            let mut lifecycle = DocumentLifecycle::default();
            lifecycle.force(loading, unavailable, source_only);
            assert_eq!(lifecycle.save(false, true), LifecycleEffect::CancelClose);
            assert!(!lifecycle.prompting());
        }
    }
}
