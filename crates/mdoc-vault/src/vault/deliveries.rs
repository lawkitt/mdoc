//! Deliveries from the AI connector (#1164 Decisions 8, 26 and 27; ADR
//! 0126; Milestone 2 item 6): an AI app's answer, pseudonymized, saved as
//! an AI Reply of the AI project, and as a new Document promoted from it
//! when the app asks for one. Restoring stays inside mdoc.
use super::{
    Vault,
    activity::{AiActivity, AiEvent, app_name},
};
use crate::{
    ObjectKind, VaultError, new_id,
    records::{Document, Project, Reply, ReplyOwner, now},
};
use mdoc_pii::restore::RestoreMap;

/// The most characters of a delivery's title kept.
const TITLE_CHARS: usize = 120;

/// A delivery, as the connector hands it over once it checked that each
/// document it names was read under the current share.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewDelivery {
    /// The documents the app read for it, by id: at least one, each in the
    /// Project now.
    pub read: Vec<String>,
    /// As the app gave it, aliases and all; `None` or blank: "Reply to"
    /// the first document read.
    pub title: Option<String>,
    /// The AI app, as it named itself.
    pub app: String,
    /// As received: aliases, never restored text.
    pub markdown: String,
    /// Land as a new Document too, promoted from the reply.
    pub as_document: bool,
}

/// What a delivery saved: ids and counts, nothing restored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Delivered {
    /// The AI Reply it was saved as.
    pub reply: String,
    /// The Document it landed as, when it asked for one.
    pub document: Option<String>,
    /// Aliases the reply restores now.
    pub restored: usize,
    /// The alias-shaped tokens in its text that no identity answers, each
    /// once, in order: the app's own text, never a name.
    pub unresolved: Vec<String>,
    /// Its line in the Project's AI activity.
    pub activity: AiActivity,
}

impl Vault {
    /// Save `delivery` for Project `project`, all under the write lock: an
    /// AI Reply of the Project, its aliases bound to the identities they
    /// mean in the Project's mapping now (so it restores by identity, as a
    /// pasted reply does), its app as the reply's `source_app`; then, when
    /// it asks, a Document promoted from it ([`Self::promote_reply`]); then
    /// its line in the Project's AI activity. Refused, with nothing
    /// written, when the Project is gone or a document it names is gone,
    /// in another Project or Unfiled ([`VaultError::NotFound`]) or excluded
    /// from AI ([`VaultError::ExcludedFromAi`]).
    ///
    /// The title, given, is restored with the Project's names now and kept
    /// to one line, as the titles of documents and pasted replies hold real
    /// names; without one it is "Reply to" the first document's title.
    pub fn deliver_in(
        &self,
        project: &str,
        delivery: NewDelivery,
    ) -> Result<Delivered, VaultError> {
        let lock = self.write_lock()?;
        let mapping = self.load_record::<Project>(project)?.mapping;
        let mut titles = Vec::with_capacity(delivery.read.len());
        for id in &delivery.read {
            let document: Document = self.load_record(id)?;
            if document.project.as_deref() != Some(project) {
                return Err(VaultError::NotFound {
                    kind: ObjectKind::Document,
                    id: id.clone(),
                });
            }
            if document.exclude_from_ai {
                return Err(VaultError::ExcludedFromAi);
            }
            titles.push(document.title);
        }
        let given = (delivery.title.as_deref())
            .and_then(|title| title.lines().map(str::trim).find(|line| !line.is_empty()));
        let title = match given {
            Some(title) => {
                RestoreMap::new(mapping.exported())
                    .restore_in_place(title)
                    .text
            }
            None => reply_to(&titles),
        };
        let app = app_name(&delivery.app);
        let reply = Reply {
            id: new_id()?,
            owner: ReplyOwner::Project(project.to_string()),
            title: title.chars().take(TITLE_CHARS).collect(),
            source_app: (!app.is_empty()).then(|| app.clone()),
            bindings: mapping.bindings(&delivery.markdown),
            markdown: delivery.markdown,
            created: now(),
        };
        let restored = (mapping.reply_map(&reply.bindings)).restore_in_place(&reply.markdown);
        let mut unresolved: Vec<String> = Vec::new();
        for range in restored.unresolved {
            let token = &restored.text[range];
            if !unresolved.iter().any(|seen| seen == token) {
                unresolved.push(token.to_string());
            }
        }
        self.write_reply(&lock, &reply)?;
        let document = match delivery.as_document {
            true => Some(self.promote_locked(&lock, &reply)?.id),
            false => None,
        };
        let activity = AiActivity {
            at: reply.created,
            app,
            event: AiEvent::Delivered {
                reply: reply.id.clone(),
                document: document.clone(),
                read: delivery.read,
            },
        };
        self.record_locked(&lock, project, &activity)?;
        Ok(Delivered {
            reply: reply.id,
            document,
            restored: restored.restored.len(),
            unresolved,
            activity,
        })
    }
}

/// "Reply to Lease.pdf", or "Reply to Lease.pdf and 2 more".
fn reply_to(titles: &[String]) -> String {
    match titles {
        [] => "Reply".to_string(),
        [one] => format!("Reply to {one}"),
        [first, rest @ ..] => format!("Reply to {first} and {} more", rest.len()),
    }
}
