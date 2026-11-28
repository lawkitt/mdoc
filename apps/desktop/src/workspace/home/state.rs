//! Home's state: the Continue strip's cards and the palette's search over
//! the Vault's summaries, worked out from the manifest in memory, with no
//! GPUI.

use super::text;
use mdoc_vault::{DocumentSummary, ProjectSummary, ReplySummary};
use std::{cmp::Reverse, collections::HashMap};

/// How many cards the Continue strip shows.
pub(in crate::workspace) const STRIP_CARDS: usize = 3;

/// How many documents a palette search lists.
pub(in crate::workspace) const SEARCH_LIMIT: usize = 50;

/// One card of the Continue strip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::workspace) enum Card {
    /// A document that needs review; `proposed` is a live view's count
    /// (none for a closed document, whose count the manifest doesn't keep).
    Document {
        summary: DocumentSummary,
        proposed: Option<usize>,
    },
    Reply(ReplySummary),
}

impl Card {
    /// Its document's or reply's id.
    pub(in crate::workspace) fn id(&self) -> &str {
        match self {
            Self::Document { summary, .. } => &summary.id,
            Self::Reply(reply) => &reply.id,
        }
    }

    /// When it last changed, in seconds since the epoch.
    pub(in crate::workspace) fn at(&self) -> u64 {
        match self {
            Self::Document { summary, .. } => summary.updated,
            Self::Reply(reply) => reply.created,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::workspace) struct Strip {
    /// Shown at all: the Vault holds a document or a reply.
    pub(in crate::workspace) shown: bool,
    pub(in crate::workspace) cards: Vec<Card>,
    /// Every document that needs review, shown or not.
    pub(in crate::workspace) need_review: usize,
}

/// Whether `summary` needs review: it was never saved Ready (desktop marks
/// a document scanned only by a Ready save), or it has proposals left
/// ([`proposals`]). A document kept from AI never does. The Project page's
/// Documents segment uses the same rule.
pub(in crate::workspace) fn needs_review(
    summary: &DocumentSummary,
    proposed: Option<usize>,
) -> bool {
    !summary.exclude_from_ai && (proposed.is_some_and(|n| n > 0) || !summary.scanned)
}

/// Proposals left in `summary`'s document: what its live view has now,
/// `live`, or else what its last save counted.
pub(in crate::workspace) fn proposals(
    summary: &DocumentSummary,
    live: Option<usize>,
) -> Option<usize> {
    live.or_else(|| (summary.counts).map(|counts| counts.proposed as usize))
        .filter(|&n| n > 0)
}

/// The Continue strip: documents that need review and saved replies,
/// newest first, at most [`STRIP_CARDS`]. `live` holds each live view's
/// proposals left, by document id.
pub(in crate::workspace) fn strip(
    documents: &[DocumentSummary],
    replies: &[ReplySummary],
    live: &HashMap<String, usize>,
) -> Strip {
    let mut cards: Vec<Card> = documents
        .iter()
        .filter_map(|summary| {
            let proposed = proposals(summary, live.get(&summary.id).copied());
            needs_review(summary, proposed).then(|| Card::Document {
                summary: summary.clone(),
                proposed,
            })
        })
        .collect();
    let need_review = cards.len();
    cards.extend(replies.iter().cloned().map(Card::Reply));
    // Stable: equal times keep documents before replies, each in the
    // Vault's order.
    cards.sort_by_key(|card| Reverse(card.at()));
    cards.truncate(STRIP_CARDS);
    Strip {
        shown: !documents.is_empty() || !replies.is_empty(),
        cards,
        need_review,
    }
}

/// Seconds since the epoch now, as the Vault stamps its records.
pub(in crate::workspace) fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// How long ago `at` was, at `now` (both seconds since the epoch): "just
/// now", "5 min ago", "3 h ago", "yesterday", "4 days ago", then the date.
pub(in crate::workspace) fn age(at: u64, now: u64) -> String {
    let seconds = now.saturating_sub(at);
    match seconds {
        0..60 => text::JUST_NOW.into(),
        60..3_600 => text::minutes_ago(seconds / 60),
        3_600..86_400 => text::hours_ago(seconds / 3_600),
        86_400..172_800 => text::YESTERDAY.into(),
        172_800..604_800 => text::days_ago(seconds / 86_400),
        _ => chrono::DateTime::from_timestamp(at as i64, 0).map_or_else(String::new, |date| {
            let date = date.with_timezone(&chrono::Local);
            let year = |d: &chrono::DateTime<chrono::Local>| chrono::Datelike::year(d);
            let this_year = chrono::DateTime::from_timestamp(now as i64, 0)
                .is_some_and(|today| year(&today.with_timezone(&chrono::Local)) == year(&date));
            date.format(if this_year {
                text::DATE_THIS_YEAR
            } else {
                text::DATE_OTHER_YEAR
            })
            .to_string()
        }),
    }
}

/// What a palette query finds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::workspace) struct Found {
    pub(in crate::workspace) documents: Vec<DocumentSummary>,
    pub(in crate::workspace) projects: Vec<ProjectSummary>,
}

/// The query's words, lowercased; none for an empty query.
pub(super) fn words(query: &str) -> Vec<String> {
    query.split_whitespace().map(str::to_lowercase).collect()
}

/// How well `words` match `text` (lowercased), best first: `text` starts
/// with the query, every word starts a word of `text`, `text` contains every
/// word; `None` when a word is missing.
pub(super) fn rank(words: &[String], text: &str) -> Option<u8> {
    if !words.iter().all(|word| text.contains(word.as_str())) {
        return None;
    }
    if text.starts_with(&words.join(" ")) {
        return Some(0);
    }
    let starts = |word: &String| {
        text.split(|c: char| !c.is_alphanumeric())
            .any(|part| part.starts_with(word.as_str()))
    };
    Some(if words.iter().all(starts) { 1 } else { 2 })
}

/// Search the Vault's titles and Project names for `query`. A document
/// matches when each word is in its title or its Project's name; those
/// matched by title come first, best match then most recently updated.
/// Projects match by name, best match first, then in `projects`' order.
/// An empty query finds nothing.
pub(in crate::workspace) fn search(
    query: &str,
    projects: &[ProjectSummary],
    documents: &[DocumentSummary],
) -> Found {
    let words = words(query);
    if words.is_empty() {
        return Found::default();
    }
    let names: HashMap<&str, String> = projects
        .iter()
        .map(|project| (project.id.as_str(), project.name.to_lowercase()))
        .collect();
    let mut found: Vec<(u8, &DocumentSummary)> = documents
        .iter()
        .filter_map(|summary| {
            let title = summary.title.to_lowercase();
            if let Some(rank) = rank(&words, &title) {
                return Some((rank, summary));
            }
            let project = (summary.project.as_deref())
                .and_then(|id| names.get(id))
                .map_or("", String::as_str);
            let each = words
                .iter()
                .all(|word| title.contains(word.as_str()) || project.contains(word.as_str()));
            each.then_some((3, summary))
        })
        .collect();
    found.sort_by_key(|(rank, summary)| (*rank, Reverse(summary.updated)));
    let mut ranked: Vec<(u8, &ProjectSummary)> = projects
        .iter()
        .filter_map(|project| Some((rank(&words, &project.name.to_lowercase())?, project)))
        .collect();
    ranked.sort_by_key(|(rank, _)| *rank);
    Found {
        documents: (found.into_iter().take(SEARCH_LIMIT))
            .map(|(_, summary)| summary.clone())
            .collect(),
        projects: ranked.into_iter().map(|(_, p)| p.clone()).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdoc_vault::{DocumentOrigin, ReplyOwner, SourceKind};

    fn document(id: &str, title: &str, project: Option<&str>, updated: u64) -> DocumentSummary {
        DocumentSummary {
            id: id.into(),
            project: project.map(Into::into),
            title: title.into(),
            source_kind: SourceKind::Pdf,
            origin: DocumentOrigin::Ordinary,
            exclude_from_ai: false,
            scanned: false,
            created: updated,
            updated,
            source_sha256: None,
            counts: None,
        }
    }

    fn project(id: &str, name: &str) -> ProjectSummary {
        ProjectSummary {
            id: id.into(),
            name: name.into(),
            colour: None,
            pinned: false,
            created: 0,
            updated: 0,
        }
    }

    fn reply(id: &str, created: u64) -> ReplySummary {
        ReplySummary {
            id: id.into(),
            owner: ReplyOwner::Document("d".into()),
            title: format!("Reply {id}"),
            source_app: None,
            created,
        }
    }

    fn ids(strip: &Strip) -> Vec<String> {
        (strip.cards.iter())
            .map(|card| match card {
                Card::Document { summary, .. } => summary.id.clone(),
                Card::Reply(reply) => reply.id.clone(),
            })
            .collect()
    }

    #[test]
    fn proposals_come_from_the_live_view_or_else_the_last_save() {
        let mut lease = document("a", "Lease", None, 1);
        assert_eq!(proposals(&lease, None), None);
        lease.counts = Some(mdoc_vault::ReviewCounts {
            proposed: 2,
            ..Default::default()
        });
        assert_eq!(proposals(&lease, None), Some(2));
        assert_eq!(
            proposals(&lease, Some(0)),
            None,
            "the live view says none now"
        );
        assert_eq!(proposals(&lease, Some(3)), Some(3));
    }

    #[test]
    fn a_document_needs_review_until_saved_ready_and_while_its_view_has_proposals() {
        let mut lease = document("a", "Lease", None, 1);
        assert!(needs_review(&lease, None));
        assert!(needs_review(&lease, Some(0)));
        lease.scanned = true;
        assert!(!needs_review(&lease, None));
        assert!(!needs_review(&lease, Some(0)));
        assert!(needs_review(&lease, Some(2)));
        lease.exclude_from_ai = true;
        assert!(!needs_review(&lease, Some(2)));
    }

    #[test]
    fn the_strip_merges_documents_and_replies_newest_first_and_counts_the_rest() {
        let mut done = document("done", "Done", None, 50);
        done.scanned = true;
        let mut kept = document("kept", "Kept from AI", None, 60);
        kept.exclude_from_ai = true;
        let documents = [
            document("old", "Old", None, 10),
            document("new", "New", Some("p"), 40),
            document("live", "Live", None, 20),
            done,
            kept,
        ];
        let live = HashMap::from([("live".to_string(), 3), ("done".to_string(), 0)]);
        let strip = strip(&documents, &[reply("r", 30)], &live);
        assert!(strip.shown);
        assert_eq!(ids(&strip), ["new", "r", "live"]);
        assert_eq!(strip.need_review, 3);
        assert_eq!(
            strip.cards[2],
            Card::Document {
                summary: documents[2].clone(),
                proposed: Some(3)
            }
        );
    }

    #[test]
    fn the_strip_hides_with_an_empty_vault_and_is_empty_with_nothing_to_review() {
        assert_eq!(strip(&[], &[], &HashMap::new()), Strip::default());
        let mut done = document("done", "Done", None, 5);
        done.scanned = true;
        let strip = strip(&[done], &[], &HashMap::new());
        assert!(strip.shown && strip.cards.is_empty() && strip.need_review == 0);
    }

    #[test]
    fn search_matches_each_word_in_the_title_or_the_project_name() {
        let projects = [project("p", "Smith v Jones"), project("q", "Lease review")];
        let documents = [
            document("a", "Lease.pdf", Some("p"), 1),
            document("b", "Schedule.docx", Some("p"), 2),
            document("c", "Notes", None, 3),
        ];
        let found = search("smith lease", &projects, &documents);
        assert_eq!(found.documents, [documents[0].clone()]);
        assert!(found.projects.is_empty());
        let found = search("JONES", &projects, &documents);
        assert_eq!(
            found.documents,
            [documents[1].clone(), documents[0].clone()]
        );
        assert_eq!(found.projects, [projects[0].clone()]);
        assert_eq!(search("  ", &projects, &documents), Found::default());
        assert_eq!(search("nothing", &projects, &documents), Found::default());
    }

    #[test]
    fn search_ranks_prefix_then_word_starts_then_contains_then_project_name() {
        let projects = [project("p", "Lease matters"), project("q", "Old leases")];
        let documents = [
            document("project", "Notes", Some("p"), 9),
            document("contains", "Release", None, 8),
            document("word", "Signed lease", None, 1),
            document("word-newer", "Draft lease", None, 2),
            document("prefix", "Lease agreement", None, 0),
        ];
        let found = search("lease", &projects, &documents);
        let order: Vec<_> = found.documents.iter().map(|d| d.id.as_str()).collect();
        assert_eq!(
            order,
            ["prefix", "word-newer", "word", "contains", "project"]
        );
        let order: Vec<_> = found.projects.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(order, ["p", "q"]);
    }

    #[test]
    fn an_age_reads_in_minutes_hours_days_then_a_date() {
        let now = 1_800_000_000;
        assert_eq!(age(now - 5, now), "just now");
        assert_eq!(age(now - 300, now), "5 min ago");
        assert_eq!(age(now - 3 * 3_600, now), "3 h ago");
        assert_eq!(age(now - 30 * 3_600, now), "yesterday");
        assert_eq!(age(now - 4 * 86_400, now), "4 days ago");
        assert!(age(now - 400 * 86_400, now).ends_with("2025"));
        // A clock set back never shows a future time.
        assert_eq!(age(now + 50, now), "just now");
    }

    #[test]
    fn search_lists_at_most_the_limit() {
        let documents: Vec<_> = (0..SEARCH_LIMIT as u64 + 5)
            .map(|i| document(&i.to_string(), "Lease", None, i))
            .collect();
        let found = search("lease", &[], &documents);
        assert_eq!(found.documents.len(), SEARCH_LIMIT);
        assert_eq!(found.documents[0].updated, SEARCH_LIMIT as u64 + 4);
    }
}
