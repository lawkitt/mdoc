//! AI Replies through the Vault (#1091 Decisions 4, 18, 20 and 37): saved
//! as received with the identity each alias meant, listed for a Project or
//! a document, restored from those identities as they are now, and
//! deleted.
//!
//! Whose a reply is: one to text shared from a document in a Project is
//! the Project's, and one to text shared from an Unfiled document is that
//! document's, which it follows into a Project later (Decision 4).
//!
//! [`ReplyVault`] is the Restore extension's handle: it reads the manifest,
//! Projects and documents and writes only new reply files, never the
//! manifest (Decision 20). The app picks a reply it saved up through
//! `Vault::refresh`.
use super::{DocumentInfo, ProjectInfo, Vault, document_infos, project_infos, vault_key};
use crate::{CoreError, text::RestoredReply};
use mdoc_pii::restore::RestoreMap;
use mdoc_vault::{Mapping, Reply, ReplySummary, VaultError};
use std::sync::Arc;

/// Where the text a reply answers was shared from.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum SharedFrom {
    /// A Project as a whole: its page.
    Project { id: String },
    /// A document: its Project's when it is in one, else Unfiled.
    Document { id: String },
}

/// Whose a saved reply is: what it restores with.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ReplyOwner {
    /// It restores with the Project's names.
    Project { id: String },
    /// An Unfiled document's when saved: it restores with the document's
    /// own names while Unfiled and its Project's once moved into one.
    Document { id: String },
}

/// A saved reply as lists show it; no text.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ReplyInfo {
    pub id: String,
    pub owner: ReplyOwner,
    pub title: String,
    /// The app it came from, when known.
    pub source_app: Option<String>,
    /// Seconds since the Unix epoch.
    pub created: u64,
}

/// A saved reply with its text as received: aliases, not restored text.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SavedReply {
    pub info: ReplyInfo,
    pub markdown: String,
}

/// The Restore extension's handle on the Vault: it reads the Projects and
/// documents and saves new replies, and writes nothing else. It never
/// writes the manifest, not even one it finds stale or missing; the app
/// takes a reply in through `Vault::refresh`.
#[derive(uniffi::Object)]
pub struct ReplyVault {
    store: mdoc_vault::ReplyHandle,
}

#[uniffi::export]
impl Vault {
    /// Save `markdown`, an AI reply as received, for the place its text was
    /// shared from: a Project's, or an Unfiled document's. Each alias in it
    /// is bound to the person it names there now. A blank reply or title
    /// is refused.
    pub fn save_reply(
        &self,
        shared_from: SharedFrom,
        title: String,
        source_app: Option<String>,
        markdown: String,
    ) -> Result<ReplyInfo, CoreError> {
        let _writes = self.lock_writes();
        save(&self.store, &shared_from, &title, source_app, markdown)
    }

    /// `markdown` restored as `save_reply` would restore it if saved now,
    /// to show it before it is saved.
    pub fn restore_text(
        &self,
        shared_from: SharedFrom,
        markdown: String,
    ) -> Result<RestoredReply, CoreError> {
        restore_text(&self.store, &shared_from, &markdown)
    }

    /// A Project's replies, newest first: its own and those of the
    /// documents in it now.
    pub fn project_replies(&self, project_id: String) -> Vec<ReplyInfo> {
        infos(self.store.project_replies(&project_id))
    }

    /// The replies an Unfiled document's text had, newest first; they stay
    /// its own after it moves into a Project.
    pub fn document_replies(&self, document_id: String) -> Vec<ReplyInfo> {
        infos(self.store.document_replies(&document_id))
    }

    pub fn reply(&self, id: String) -> Result<SavedReply, CoreError> {
        Ok(saved(self.store.reply(&id)?))
    }

    /// Saved reply `id` restored with the people it named when saved, as
    /// they are now: a forgotten one stays an alias.
    pub fn restore_reply(&self, id: String) -> Result<RestoredReply, CoreError> {
        restore(&self.store, &id)
    }

    pub fn delete_reply(&self, id: String) -> Result<(), CoreError> {
        let _writes = self.lock_writes();
        Ok(self.store.delete_reply(&id)?)
    }

    /// Saved reply `id` as lists show it, from the manifest; `None` once it
    /// is deleted: a promoted document's way back reads so.
    pub fn reply_info(&self, id: String) -> Option<ReplyInfo> {
        (self.store.replies().into_iter())
            .find(|reply| reply.id == id)
            .map(info)
    }

    /// Promote to Document (#1091 Decision 3): a new document in saved
    /// reply `id`'s place (its Project, or Unfiled for an Unfiled
    /// document's reply) whose text is the reply restored as it restores
    /// now, real names back. It is not reviewed, has no original, and
    /// names the reply (`DocumentSession::from_reply`); the reply stays.
    /// Nothing is written when the reply or its place is gone.
    pub fn promote_reply(&self, id: String) -> Result<DocumentInfo, CoreError> {
        let _writes = self.lock_writes();
        let document = self.store.promote_reply(&id)?;
        Ok(self.infos(vec![mdoc_vault::DocumentSummary::of(&document)])
            .remove(0))
    }
}

#[uniffi::export]
impl ReplyVault {
    /// The Vault in folder `root`, which the app made, sealed with `key`
    /// (32 bytes), for the Restore extension. A missing folder or another
    /// key is refused, and nothing is written.
    #[uniffi::constructor]
    pub fn open(root: String, key: Vec<u8>) -> Result<Arc<Self>, CoreError> {
        let key = vault_key(&key)?;
        Ok(Arc::new(Self {
            store: mdoc_vault::ReplyHandle::open(root, &key)?,
        }))
    }

    /// Take in what the app wrote since; true when anything changed.
    pub fn refresh(&self) -> Result<bool, CoreError> {
        Ok(self.store.refresh()?)
    }

    /// Every Project, pinned first, then most recently updated.
    pub fn projects(&self) -> Vec<ProjectInfo> {
        project_infos(&self.store)
    }

    /// A Project's documents, most recently updated first.
    pub fn documents_in(&self, project_id: String) -> Vec<DocumentInfo> {
        document_infos(&self.store, self.store.documents_in(Some(&project_id)))
    }

    /// Unfiled documents, most recently updated first.
    pub fn unfiled(&self) -> Vec<DocumentInfo> {
        document_infos(&self.store, self.store.documents_in(None))
    }

    /// `Vault::save_reply`: only the reply's file is written.
    pub fn save_reply(
        &self,
        shared_from: SharedFrom,
        title: String,
        source_app: Option<String>,
        markdown: String,
    ) -> Result<ReplyInfo, CoreError> {
        save(&self.store, &shared_from, &title, source_app, markdown)
    }

    /// `Vault::restore_text`.
    pub fn restore_text(
        &self,
        shared_from: SharedFrom,
        markdown: String,
    ) -> Result<RestoredReply, CoreError> {
        restore_text(&self.store, &shared_from, &markdown)
    }

    /// `Vault::project_replies`.
    pub fn project_replies(&self, project_id: String) -> Vec<ReplyInfo> {
        infos(self.store.project_replies(&project_id))
    }

    /// `Vault::document_replies`.
    pub fn document_replies(&self, document_id: String) -> Vec<ReplyInfo> {
        infos(self.store.document_replies(&document_id))
    }

    pub fn reply(&self, id: String) -> Result<SavedReply, CoreError> {
        Ok(saved(self.store.reply(&id)?))
    }

    /// `Vault::restore_reply`.
    pub fn restore_reply(&self, id: String) -> Result<RestoredReply, CoreError> {
        restore(&self.store, &id)
    }
}

/// What saving and restoring replies needs of a Vault handle.
trait Replies {
    fn save_shared_reply(
        &self,
        from: &mdoc_vault::SharedFrom,
        title: String,
        source_app: Option<String>,
        markdown: String,
    ) -> Result<Reply, VaultError>;
    fn shared_owner(
        &self,
        from: &mdoc_vault::SharedFrom,
    ) -> Result<(mdoc_vault::ReplyOwner, Mapping), VaultError>;
    fn owner_mapping(&self, owner: &mdoc_vault::ReplyOwner) -> Result<Mapping, VaultError>;
    fn reply(&self, id: &str) -> Result<Reply, VaultError>;
}

macro_rules! replies {
    ($handle:ty) => {
        impl Replies for $handle {
            fn save_shared_reply(
                &self,
                from: &mdoc_vault::SharedFrom,
                title: String,
                source_app: Option<String>,
                markdown: String,
            ) -> Result<Reply, VaultError> {
                <$handle>::save_shared_reply(self, from, title, source_app, markdown)
            }
            fn shared_owner(
                &self,
                from: &mdoc_vault::SharedFrom,
            ) -> Result<(mdoc_vault::ReplyOwner, Mapping), VaultError> {
                <$handle>::shared_owner(self, from)
            }
            fn owner_mapping(&self, owner: &mdoc_vault::ReplyOwner) -> Result<Mapping, VaultError> {
                <$handle>::owner_mapping(self, owner)
            }
            fn reply(&self, id: &str) -> Result<Reply, VaultError> {
                <$handle>::reply(self, id)
            }
        }
    };
}
replies!(mdoc_vault::Vault);
replies!(mdoc_vault::ReplyHandle);

impl From<&SharedFrom> for mdoc_vault::SharedFrom {
    fn from(from: &SharedFrom) -> Self {
        match from {
            SharedFrom::Project { id } => Self::Project(id.clone()),
            SharedFrom::Document { id } => Self::Document(id.clone()),
        }
    }
}

/// Saved with the owner Decision 4 gives, found under the Vault's write
/// lock (`mdoc_vault::Vault::save_shared_reply`).
fn save(
    store: &impl Replies,
    from: &SharedFrom,
    title: &str,
    source_app: Option<String>,
    markdown: String,
) -> Result<ReplyInfo, CoreError> {
    let title = title.trim();
    if title.is_empty() {
        return Err(CoreError::invalid("A reply needs a title."));
    }
    if markdown.trim().is_empty() {
        return Err(CoreError::invalid("The reply is empty."));
    }
    let source_app = source_app
        .map(|app| app.trim().to_string())
        .filter(|app| !app.is_empty());
    let reply = store.save_shared_reply(&from.into(), title.into(), source_app, markdown)?;
    Ok(info(ReplySummary::of(&reply)))
}

fn restore_text(
    store: &impl Replies,
    from: &SharedFrom,
    markdown: &str,
) -> Result<RestoredReply, CoreError> {
    let (_, mapping) = store.shared_owner(&from.into())?;
    let map = mapping.reply_map(&mapping.bindings(markdown));
    Ok(restored(&map, &mapping, markdown))
}

fn restore(store: &impl Replies, id: &str) -> Result<RestoredReply, CoreError> {
    let reply = store.reply(id)?;
    let mapping = store.owner_mapping(&reply.owner)?;
    let map = mapping.reply_map(&reply.bindings);
    Ok(restored(&map, &mapping, &reply.markdown))
}

/// `reply` restored with `map`, the identities it binds. Copy and Share
/// follow the owner's names (`mapping`), as the library's do: a reply
/// that names none of them can still be copied.
fn restored(map: &RestoreMap, mapping: &Mapping, reply: &str) -> RestoredReply {
    let restorable = (mapping.identities.iter()).any(|i| i.exported().is_some());
    RestoredReply {
        restorable,
        ..RestoredReply::of(map, reply)
    }
}

fn saved(reply: Reply) -> SavedReply {
    SavedReply {
        info: info(ReplySummary::of(&reply)),
        markdown: reply.markdown,
    }
}

fn infos(replies: Vec<ReplySummary>) -> Vec<ReplyInfo> {
    replies.into_iter().map(info).collect()
}

fn info(reply: ReplySummary) -> ReplyInfo {
    ReplyInfo {
        id: reply.id,
        owner: match reply.owner {
            mdoc_vault::ReplyOwner::Project(id) => ReplyOwner::Project { id },
            mdoc_vault::ReplyOwner::Document(id) => ReplyOwner::Document { id },
        },
        title: reply.title,
        source_app: reply.source_app,
        created: reply.created,
    }
}

#[cfg(test)]
mod tests;
