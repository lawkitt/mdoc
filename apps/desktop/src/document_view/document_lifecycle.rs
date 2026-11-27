//! A document view's lifecycle: loading its
//! Markdown and whether that failed, whether it shows only a source still to
//! convert, whether it is a generated conversion nobody edited yet or a blank
//! Home placeholder, and whether a dialog of its own is open. Documents save
//! themselves (the workspace's autosave); this owner only tracks the view.

/// What the document view carries out for a lifecycle transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LifecycleEffect {
    /// Go on with what was asked.
    Proceed,
    /// Ask for files to open.
    AskForFiles,
    /// Ask where to export the Markdown.
    AskForPath,
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
    /// An automatic conversion nobody has edited: the session keeps its
    /// source instead of a Markdown file.
    generated_unedited: bool,
    /// A blank placeholder Home may replace.
    blank_disposable: bool,
    /// One of this document's dialogs is open.
    prompting: bool,
    /// When the user last edited the text in this session (the shelf's
    /// order).
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
    /// or placeholder.
    pub(crate) fn edited(&mut self) {
        self.generated_unedited = false;
        self.blank_disposable = false;
        self.edited_at = Some(std::time::SystemTime::now());
    }

    /// When the user last edited the text in this session.
    pub(crate) fn edited_at(&self) -> Option<std::time::SystemTime> {
        self.edited_at
    }

    /// The user acted on the tab, so Home no longer replaces it.
    pub(crate) fn keep_blank(&mut self) {
        self.blank_disposable = false;
    }

    /// Something asked to replace or close the document. Nothing while one
    /// of its dialogs is open.
    pub(crate) fn request(&mut self) -> Option<LifecycleEffect> {
        (!self.prompting).then_some(LifecycleEffect::Proceed)
    }

    /// Open pressed. Nothing while one of the document's dialogs is open.
    pub(crate) fn open_files(&mut self) -> Option<LifecycleEffect> {
        if self.prompting {
            return None;
        }
        self.prompting = true;
        Some(LifecycleEffect::AskForFiles)
    }

    /// Export Markdown. A document that is loading, failed to load, shows
    /// only a source, or has a dialog open has nothing to export.
    pub(crate) fn export(&mut self) -> Option<LifecycleEffect> {
        if self.prompting || self.loading || self.unavailable || self.source_only {
            return None;
        }
        self.prompting = true;
        Some(LifecycleEffect::AskForPath)
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
    fn a_conversion_is_generated_until_edited() {
        let mut lifecycle = lifecycle(false, true, false);
        assert!(lifecycle.source_only());
        lifecycle.converted(true);
        assert!(!lifecycle.source_only() && lifecycle.generated_unedited());
        lifecycle.edited();
        assert!(!lifecycle.generated_unedited());
        lifecycle.converted(false);
        assert!(!lifecycle.generated_unedited());
        lifecycle.show_source();
        assert!(lifecycle.source_only());
    }

    #[test]
    fn a_blank_placeholder_stops_being_one_once_used() {
        let mut placeholder = lifecycle(false, false, true);
        assert!(placeholder.blank_disposable());
        placeholder.edited();
        assert!(!placeholder.blank_disposable());
        let mut placeholder = lifecycle(false, false, true);
        placeholder.keep_blank();
        assert!(!placeholder.blank_disposable());
    }

    #[test]
    fn nothing_else_asks_while_a_dialog_is_open() {
        let mut lifecycle = DocumentLifecycle::default();
        assert_eq!(lifecycle.request(), Some(LifecycleEffect::Proceed));
        assert_eq!(lifecycle.open_files(), Some(LifecycleEffect::AskForFiles));
        assert!(lifecycle.prompting());
        assert_eq!(lifecycle.request(), None);
        assert_eq!(lifecycle.open_files(), None);
        assert_eq!(lifecycle.export(), None);
        lifecycle.end_prompt();
        assert_eq!(lifecycle.export(), Some(LifecycleEffect::AskForPath));
        lifecycle.end_prompt();
        assert!(!lifecycle.prompting());
    }

    #[test]
    fn only_a_shown_text_exports() {
        for (loading, unavailable, source_only) in [
            (true, false, false),
            (false, true, false),
            (false, false, true),
        ] {
            let mut lifecycle = DocumentLifecycle::default();
            lifecycle.force(loading, unavailable, source_only);
            assert_eq!(lifecycle.export(), None);
            assert!(!lifecycle.prompting());
        }
    }
}
