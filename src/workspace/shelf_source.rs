//! What the window gives the menu bar shelf (ADR 0084): its open tabs with
//! applied aliases and its cases' closed members, read only while the shelf
//! asks; and, on save, the fingerprint of a ready member's text, kept in its
//! (encrypted) case.

use super::Workspace;
use crate::{
    document_view::DocumentView,
    shelf::{self, ClosedMember, OpenTab, ShelfDocument},
};
use gpui::{App, Context};
use mdoc_pii::restore::ExportedIdentity;
use mdoc_store::session_store;
use std::{path::PathBuf, time::SystemTime};

impl Workspace {
    /// The shelf's documents: open tabs with applied aliases, then closed
    /// members of unlocked cases, at most `shelf::MAX_DOCUMENTS`. Reads up
    /// to that many member files; nothing about them is kept.
    pub(crate) fn shelf_documents(&self, cx: &App) -> Vec<ShelfDocument> {
        let mut open = Vec::new();
        let mut tabs = Vec::new();
        for tab in &self.tabs {
            let Some(view) = tab.view() else {
                continue;
            };
            let view = view.read(cx);
            if let Some(path) = &view.session.document.path {
                open.push(session_store::identity(path));
            }
            if let Some(document) = shelf_tab(tab.id(), view, cx) {
                tabs.push(document);
            }
        }
        let room = shelf::MAX_DOCUMENTS.saturating_sub(tabs.len());
        // Locked cases are not loaded: their closed members stay hidden.
        let members = shelf::closed_members(self.case_coordinator.cases(), &open, room)
            .into_iter()
            .map(|(case, path, edited)| {
                let text = shelf::read_member(path);
                ShelfDocument::of_member(ClosedMember {
                    case,
                    path,
                    text: text.as_deref(),
                    edited,
                })
            })
            .collect();
        shelf::list(tabs, members)
    }

    /// The tab `id` was saved, or its case created: keep the fingerprint of
    /// its saved text in its case while it is ready, or forget it.
    pub(super) fn record_review(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(view) = self.tabs.get(id).and_then(|tab| tab.view().cloned()) else {
            return;
        };
        let (link, path, fingerprint) = {
            let v = view.read(cx);
            (
                v.case.link().cloned(),
                v.session
                    .document
                    .path
                    .as_deref()
                    .map(session_store::identity),
                if !ready(v) {
                    Some(None)
                } else if v.lifecycle.dirty() {
                    // The saved text is the reviewed one only while nothing
                    // is unsaved: keep the fingerprint of the last save.
                    None
                } else {
                    Some(Some(shelf::fingerprint(&v.session.document.saved)))
                },
            )
        };
        let (Some(link), Some(path), Some(fingerprint)) = (link, path, fingerprint) else {
            return;
        };
        if let Some(((), effect)) = self.case_coordinator.edit(&link.id, |case| {
            case.members
                .contains(&path)
                .then(|| case.set_reviewed(&path, fingerprint))
                .filter(|changed| *changed)
                .map(|_| ())
        }) {
            self.carry_out_cases(effect.into_iter().collect(), cx);
        }
    }
}

/// A tab is ready: at least one applied alias and nothing pending.
fn ready(view: &DocumentView) -> bool {
    let review = view.pii.review();
    !review.applied().is_empty() && review.remaining() == 0
}

/// An open tab's shelf row; `None` without an applied alias.
fn shelf_tab(id: u64, view: &DocumentView, cx: &App) -> Option<ShelfDocument> {
    let review = view.pii.review();
    if review.applied().is_empty() || !view.lifecycle.ready() {
        return None;
    }
    // Every name the tab knows masks the title and preview: its mapping,
    // its case's names and its pending proposals.
    let mut names = review.export_identities();
    names.extend(view.case.names().iter().cloned());
    for candidate in review.candidates() {
        if let Some(variant) = review.variant(candidate.variant) {
            names.push(ExportedIdentity {
                alias: variant.replacement.clone(),
                previous: Vec::new(),
                category: variant.category,
                custom: false,
                originals: vec![variant.original.clone()],
            });
        }
    }
    let edited = view
        .lifecycle
        .edited_at()
        .or_else(|| modified(view.session.document.path.as_ref()))
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let title = view.display_name();
    ShelfDocument::of_tab(OpenTab {
        id,
        title: &title,
        text: view.editor.read(cx).text(),
        aliases: review
            .applied()
            .iter()
            .map(|applied| applied.step.after.as_ref())
            .collect(),
        pending: review.remaining(),
        names,
        case: view.case.link().cloned(),
        edited,
    })
}

fn modified(path: Option<&PathBuf>) -> Option<SystemTime> {
    std::fs::metadata(path?).ok()?.modified().ok()
}
