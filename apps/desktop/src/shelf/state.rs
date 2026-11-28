//! The menu bar shelf's model, GPUI-free: which documents it
//! lists and whether each is ready, the pseudonymized row it shows, the
//! text Copy for AI and a drag carry, the temporary `.md` exports a drag
//! drops under a neutral name, and Restore Clipboard. The window collects
//! what it lists, from its open tabs and the Vault; the shelf's view shows
//! it.
//!
//! Invariants: nothing leaves through the shelf that is not ready, and a
//! row is ready only once the Vault's outbound check passes
//! (`Vault::text_for_ai`), whose text is all a row hands out; rows and
//! exports carry pseudonymized text only; an export's name holds nothing
//! of the title (#1208); no document text goes to app data, and exports
//! live in a private temporary folder until quit.

use super::preview::{Preview, preview_lines};
use super::text::neutral_name;
use crate::workspace::ProjectLink;
use mdoc_pii::{
    Category,
    restore::{ExportedIdentity, RestoreMap},
};
use mdoc_store::document_id::VaultId;
use std::time::SystemTime;

/// The shelf lists at most this many documents.
pub(crate) const MAX_DOCUMENTS: usize = 8;

/// Whether a listed document may leave through the shelf.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Readiness {
    /// At least one applied alias and nothing pending: copy and drag.
    Ready,
    /// An open tab with this many proposals still to review.
    LeftToReview(usize),
    /// An open tab just edited: its proposals are being found again, or
    /// its text, or a document's Project names, are not written to the
    /// Vault yet.
    Checking,
    /// A document whose text still holds this many known names by an
    /// original wording: a kept mention, or a name another document added
    /// to its Project since.
    NamesNotReplaced(usize),
    /// A document whose stored text is not the text it saved while ready.
    ChangedSinceReview,
    /// A document never saved while ready: never scanned, or saved with
    /// proposals left.
    NotReviewed,
}

impl Readiness {
    pub(crate) fn is_ready(self) -> bool {
        self == Self::Ready
    }
}

/// What a row opens: an open tab, or a Vault document no tab shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ShelfKey {
    Tab(u64),
    Document(VaultId),
}

/// What the Vault's outbound check (`Vault::text_for_ai`) gave for a
/// document as stored: the text Copy for AI sends, or why it may not leave.
pub(crate) type Outbound = Result<String, mdoc_vault::Readiness>;

/// A stored document's state as the shelf shows it, when the outbound
/// check refused it.
fn refused(readiness: mdoc_vault::Readiness) -> Readiness {
    match readiness {
        mdoc_vault::Readiness::NamesNotReplaced(n) => Readiness::NamesNotReplaced(n),
        mdoc_vault::Readiness::ChangedSinceReview => Readiness::ChangedSinceReview,
        // The check refuses only what is not Ready; a refusal it can't name
        // (an unreadable document) has no review to show.
        mdoc_vault::Readiness::Ready | mdoc_vault::Readiness::NotReviewed => Readiness::NotReviewed,
    }
}

/// One row of the shelf.
#[derive(Clone, Debug)]
pub(crate) struct ShelfDocument {
    pub key: ShelfKey,
    /// The title with known originals pseudonymized.
    pub title: String,
    pub project: Option<ProjectLink>,
    /// The Project's name as the row shows it, pseudonymized with the
    /// names the document knows: a Project is often named after a person.
    pub project_label: Option<String>,
    pub readiness: Readiness,
    /// Names were looked for: a document never saved ready has a review to
    /// finish, not names to find.
    pub scanned: bool,
    /// Moved since it was saved Ready ([`StoredDocument::moved`]).
    pub moved: bool,
    pub edited: SystemTime,
    pub words: usize,
    /// Only a ready document with prose has one.
    pub preview: Option<Preview>,
    /// Distinct names its text holds by an alias, per category, in
    /// [`Category::ALL`] order: the breakdown hovering "24 replaced" shows.
    pub replaced: Vec<(Category, usize)>,
    /// Mentions its text holds by an alias: "24 replaced", the count the
    /// editor's Share tab shows.
    pub mentions: usize,
    /// Copy for AI's text; only a ready document has it.
    for_ai: Option<String>,
    /// Its document's `Vault::share_name`, asked only as a drag leaves
    /// ([`Self::set_share_name`]): see [`Self::file_name`].
    share_name: Option<String>,
}

/// An open tab as the shelf needs it. The window builds this only for tabs
/// with applied aliases.
pub(crate) struct OpenTab<'a> {
    pub id: u64,
    /// The tab's title, as the sidebar shows it (may hold an original).
    pub title: &'a str,
    pub text: &'a str,
    /// Applied aliases in source order, as Copy for AI names them.
    pub aliases: Vec<&'a str>,
    /// Proposals still to review.
    pub pending: usize,
    /// An edit's proposals are still being found again, so `pending` may
    /// undercount.
    pub checking: bool,
    /// Every name the tab knows, pending and kept ones included: they mask
    /// the title and preview, and any left in the text keeps it from
    /// leaving.
    pub names: Vec<ExportedIdentity>,
    pub project: Option<ProjectLink>,
    pub edited: SystemTime,
    /// The outbound check of its Vault document, asked only once its review
    /// is ready; `None` while its text or review is not written yet.
    pub stored: Option<Outbound>,
}

impl OpenTab<'_> {
    /// Its review's readiness, as a write records it: an applied alias
    /// assumed, nothing pending or still being found, and no known name
    /// left as written. The Vault is not asked.
    pub(crate) fn review_readiness(&self) -> Readiness {
        self.review_readiness_with(&RestoreMap::new(self.names.clone()))
    }

    /// As if what it shows was written and the outbound check passed with
    /// the editor's Copy for AI text, for tests and renders.
    #[cfg(any(
        test,
        all(
            debug_assertions,
            feature = "headless-renders",
            any(target_os = "macos", target_os = "windows")
        )
    ))]
    pub(crate) fn checked(mut self) -> Self {
        let text = mdoc_pii::handoff::for_ai(self.text, self.aliases.iter().copied());
        self.stored = Some(Ok(text));
        self
    }

    fn review_readiness_with(&self, names: &RestoreMap) -> Readiness {
        if self.pending > 0 {
            Readiness::LeftToReview(self.pending)
        } else if self.checking {
            Readiness::Checking
        } else {
            match names.known_names(self.text) {
                0 => Readiness::Ready,
                missed => Readiness::NamesNotReplaced(missed),
            }
        }
    }
}

/// A Vault document no open tab shows, as stored.
pub(crate) struct StoredDocument<'a> {
    pub id: VaultId,
    pub title: &'a str,
    /// Its Markdown as stored.
    pub text: &'a str,
    /// The names it lives with: its Project's as stored, or its own while
    /// Unfiled.
    pub names: Vec<ExportedIdentity>,
    pub project: Option<ProjectLink>,
    /// When it was last written.
    pub edited: SystemTime,
    /// Names were looked for in it: scanned, or saved with a review.
    pub scanned: bool,
    /// Scanned, but moved since it was saved Ready: no review and no Ready
    /// fingerprint until its next save, which opening it and Copy for AI
    /// make (#1292).
    pub moved: bool,
    /// `None` while a move of it, or a change to its Project's names, is
    /// not written yet.
    pub outbound: Option<Outbound>,
}

impl ShelfDocument {
    /// An open tab's row; `None` without an applied alias (not listed).
    /// Ready only when its review is and the outbound check of what it
    /// wrote passes; Copy for AI's text is that check's.
    pub(crate) fn of_tab(tab: OpenTab<'_>) -> Option<Self> {
        if tab.aliases.is_empty() {
            return None;
        }
        Some(Self::of_any_tab(tab))
    }

    /// An open tab's row even with no alias, which the shelf does not list:
    /// the editor's Copy for AI of a document scanned with nothing found.
    pub(crate) fn of_any_tab(tab: OpenTab<'_>) -> Self {
        let names = RestoreMap::new(tab.names.clone());
        let (readiness, for_ai) = match (tab.review_readiness_with(&names), tab.stored) {
            (Readiness::Ready, Some(Ok(text))) => (Readiness::Ready, Some(text)),
            (Readiness::Ready, Some(Err(readiness))) => (refused(readiness), None),
            // Its last edit is still on its way to the Vault.
            (Readiness::Ready, None) => (Readiness::Checking, None),
            (readiness, _) => (readiness, None),
        };
        Self::new(
            ShelfKey::Tab(tab.id),
            tab.title,
            tab.text,
            &names,
            tab.project,
            // An open tab with an applied alias has a review to open.
            (readiness, true, false),
            tab.edited,
            for_ai,
        )
    }

    /// A stored document's row: ready only when the outbound check passes,
    /// as it leaves with that check's text. `None` when its text holds no
    /// alias its names know (not listed), as a tab without an applied
    /// alias is not: reopened, it would have none.
    pub(crate) fn of_stored(document: StoredDocument<'_>) -> Option<Self> {
        let names = RestoreMap::new(document.names);
        if names.exact_aliases(document.text).is_empty() {
            return None;
        }
        let (readiness, for_ai) = match document.outbound {
            Some(Ok(text)) => (Readiness::Ready, Some(text)),
            Some(Err(readiness)) => (refused(readiness), None),
            None => (Readiness::Checking, None),
        };
        // Moved, with a name its new place knows still written out: opening
        // it leaves that name to review, so it does not say "open it to
        // share" (#1292).
        let moved = document.moved && names.known_names(document.text) == 0;
        Some(Self::new(
            ShelfKey::Document(document.id),
            document.title,
            document.text,
            &names,
            document.project,
            (readiness, document.scanned, moved),
            document.edited,
            for_ai,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn new(
        key: ShelfKey,
        title: &str,
        text: &str,
        names: &RestoreMap,
        project: Option<ProjectLink>,
        (readiness, scanned, moved): (Readiness, bool, bool),
        edited: SystemTime,
        for_ai: Option<String>,
    ) -> Self {
        let preview = readiness
            .is_ready()
            .then(|| preview_lines(text, names))
            .flatten();
        let (mentions, replaced) = replaced(text, names);
        Self {
            key,
            title: names.pseudonymize(title).0,
            project_label: (project.as_ref()).map(|project| names.pseudonymize(&project.name).0),
            project,
            readiness,
            scanned,
            moved,
            edited,
            words: text
                .split_whitespace()
                .filter(|word| word.chars().any(char::is_alphanumeric))
                .count(),
            preview,
            replaced,
            mentions,
            for_ai,
            share_name: None,
        }
    }

    /// The title as the row shows it: without `.md`.
    pub(crate) fn display_title(&self) -> &str {
        let lower = self.title.to_ascii_lowercase();
        [".md", ".markdown"]
            .iter()
            .find(|ext| lower.ends_with(*ext) && lower.len() > ext.len())
            .map_or(&self.title, |ext| {
                &self.title[..self.title.len() - ext.len()]
            })
    }

    /// What resolves the row's refusal: "Replace 2 and copy", "Review" or
    /// "Find names"; `None` while ready or still checking.
    pub(crate) fn step(&self) -> Option<crate::review::refusal::Step> {
        crate::review::refusal::Step::of(self.readiness, self.scanned)
    }

    /// The card a drag shows under the pointer: the title as the
    /// row shows it and the preview, both pseudonymized; `None` unless
    /// ready.
    pub(crate) fn drag_card(&self) -> Option<(String, String)> {
        self.readiness.is_ready().then(|| {
            (
                self.display_title().to_string(),
                self.preview
                    .as_ref()
                    .map(|preview| preview.text.clone())
                    .unwrap_or_default(),
            )
        })
    }

    /// How many mentions its text replaced: "24 replaced".
    pub(crate) fn replaced_count(&self) -> usize {
        self.mentions
    }

    /// How many distinct names those mentions are: "14 names".
    #[cfg(any(
        test,
        all(
            debug_assertions,
            feature = "headless-renders",
            any(target_os = "macos", target_os = "windows")
        )
    ))]
    pub(crate) fn names_count(&self) -> usize {
        self.replaced.iter().map(|(_, n)| n).sum()
    }

    /// The text Copy for AI and a drag carry; `None` unless ready.
    pub(crate) fn text_for_ai(&self) -> Option<&str> {
        self.for_ai.as_deref().filter(|_| self.readiness.is_ready())
    }

    /// The `.md` file name a drag drops: the document's
    /// `Vault::share_name`, "Document 3 – 2026-11-25.md", or "Document –
    /// 2026-11-25.md" when the Vault gave none. Nothing of the title, which
    /// may hold a real name nobody reviewed (#1208).
    pub(crate) fn file_name(&self) -> String {
        #[cfg(test)]
        if crate::exits::taken_out(crate::exits::Gate::ShelfFileName) {
            // As before #1208: the title, masked with the names it knows.
            return format!("{}.md", self.display_title().replace(['/', ':'], "-"));
        }
        match &self.share_name {
            Some(name) => format!("{name}.md"),
            None => format!("{}.md", neutral_name(SystemTime::now())),
        }
    }

    /// The name its drag's file leaves under: `Vault::share_name`, asked
    /// as the drag leaves, not for every row listed.
    pub(crate) fn set_share_name(&mut self, name: Option<String>) {
        self.share_name = name;
    }
}

/// The mentions `text` holds by an alias, and the distinct names they are,
/// per category.
fn replaced(text: &str, names: &RestoreMap) -> (usize, Vec<(Category, usize)>) {
    let identities = names.identities();
    let mut mentions = 0;
    let mut seen: Vec<&str> = Vec::new();
    let mut counts = [0usize; Category::ALL.len()];
    for token in names.tokens(text) {
        let Some(identity) = token.identity.and_then(|i| identities.get(i)) else {
            continue;
        };
        mentions += 1;
        // The tab's names list one person in its mapping and its Project.
        if seen.contains(&identity.alias.as_str()) {
            continue;
        }
        seen.push(&identity.alias);
        if let Some(at) = Category::ALL.iter().position(|c| *c == identity.category) {
            counts[at] += 1;
        }
    }
    let replaced = Category::ALL
        .into_iter()
        .zip(counts)
        .filter(|(_, n)| *n > 0)
        .collect();
    (mentions, replaced)
}

/// The shelf's list: open tabs, most recently edited first, then stored
/// documents no tab shows, most recently written first; at most
/// [`MAX_DOCUMENTS`].
pub(crate) fn list(
    mut tabs: Vec<ShelfDocument>,
    mut stored: Vec<ShelfDocument>,
) -> Vec<ShelfDocument> {
    tabs.sort_by_key(|row| std::cmp::Reverse(row.edited));
    stored.sort_by_key(|row| std::cmp::Reverse(row.edited));
    tabs.into_iter().chain(stored).take(MAX_DOCUMENTS).collect()
}

/// What Restore Clipboard did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ClipboardRestore {
    /// The clipboard text with the Project's aliases restored.
    pub text: String,
    /// Distinct names restored, as the reply box counts them.
    pub restored: usize,
    /// Alias-shaped tokens naming none of the Project's identities.
    pub unrecognised: usize,
}

/// Restore Clipboard: replace one Project's aliases in `text` with their
/// primary wordings, matching as Restore does. Projects are never mixed.
pub(crate) fn restore_clipboard(text: &str, names: Vec<ExportedIdentity>) -> ClipboardRestore {
    let restored = RestoreMap::new(names).restore_in_place(text);
    ClipboardRestore {
        restored: restored.values,
        unrecognised: restored.unresolved.len(),
        text: restored.text,
    }
}
