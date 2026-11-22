//! AI Replies (#1091 Decisions 4, 16, 18 and 20): saved as received, with
//! each alias bound to the identity it meant, and restored from those
//! identities' originals as they are now.
use super::{Vault, WriteLock};
use crate::{
    ObjectKind, VaultError, is_id,
    manifest::{Entry, Record, reply_path},
    mapping::Mapping,
    new_id,
    records::{Document, NewReply, Project, Reply, ReplyOwner, ReplySummary, now},
};
use mdoc_pii::restore::{RestoreMap, RestoredText};
use std::io;

impl Vault {
    /// Save an AI Reply for a Project or a Document, binding each alias in
    /// it to the identity it names in the owner's mapping now: a Document's
    /// own while Unfiled, its Project's once in one. Only the reply's file
    /// is written, never the manifest, so the Restore extension's handle
    /// leaves the app's cache alone; other handles pick it up through
    /// [`Self::refresh`].
    pub fn save_reply(&self, new: NewReply) -> Result<Reply, VaultError> {
        let lock = self.write_lock()?;
        let mapping = self.owner_mapping(&new.owner)?;
        let reply = Reply {
            id: new_id()?,
            bindings: mapping.bindings(&new.markdown),
            owner: new.owner,
            title: new.title,
            source_app: new.source_app,
            markdown: new.markdown,
            created: now(),
        };
        self.write_reply(&lock, &reply)?;
        Ok(reply)
    }

    /// The AI Reply `id`, read from its file.
    pub fn reply(&self, id: &str) -> Result<Reply, VaultError> {
        self.load_record(id)
    }

    /// What `reply` restores with: each bound identity's originals now,
    /// read in the mapping its owner has now. An identity forgotten or
    /// gone restores nothing, and no alias it did not bind is read.
    pub fn reply_map(&self, reply: &Reply) -> Result<RestoreMap, VaultError> {
        Ok(self.owner_mapping(&reply.owner)?.reply_map(&reply.bindings))
    }

    /// The AI Reply `id` restored for reading: [`Self::reply_map`] applied
    /// to its text.
    pub fn restore_reply(&self, id: &str) -> Result<RestoredText, VaultError> {
        let reply = self.reply(id)?;
        Ok(self.reply_map(&reply)?.restore_in_place(&reply.markdown))
    }

    /// Every AI Reply, newest first.
    pub fn replies(&self) -> Vec<ReplySummary> {
        let mut replies: Vec<_> = (self.state().manifest.replies.ok.values())
            .map(|entry| entry.summary.clone())
            .collect();
        replies.sort_by(|a, b| {
            b.created
                .cmp(&a.created)
                .then_with(|| a.title.cmp(&b.title))
                .then_with(|| a.id.cmp(&b.id))
        });
        replies
    }

    /// A Project's AI Replies, newest first: its own and those of the
    /// Documents in it now, which followed them there.
    pub fn project_replies(&self, project: &str) -> Vec<ReplySummary> {
        let documents: Vec<String> = (self.documents_in(Some(project)).into_iter())
            .map(|document| document.id)
            .collect();
        let mut replies = self.replies();
        replies.retain(|reply| match &reply.owner {
            ReplyOwner::Project(id) => id == project,
            ReplyOwner::Document(id) => documents.contains(id),
        });
        replies
    }

    /// The AI Replies saved for Document `document`, newest first.
    pub fn document_replies(&self, document: &str) -> Vec<ReplySummary> {
        let mut replies = self.replies();
        replies.retain(|reply| matches!(&reply.owner, ReplyOwner::Document(id) if id == document));
        replies
    }

    /// Delete the AI Reply `id`.
    pub fn delete_reply(&self, id: &str) -> Result<(), VaultError> {
        if !is_id(id) {
            return Err(VaultError::InvalidId);
        }
        let _lock = self.write_lock()?;
        self.check_alive()?;
        super::remove_sealed(&reply_path(&self.root, id))?;
        self.forget_deleted::<Reply>(id)
    }

    /// Delete the replies `owner` has, as the case store dropped a case's
    /// replies with it and a member's with the member. Replies another
    /// handle saved are found first; one that does not open is passed
    /// over, as its owner is unknown.
    pub(super) fn delete_replies_of(
        &self,
        _lock: &WriteLock<'_>,
        owner: &ReplyOwner,
    ) -> Result<(), VaultError> {
        self.refresh()?;
        let owned: Vec<String> = (self.state().manifest.replies.ok.values())
            .filter(|entry| entry.summary.owner == *owner)
            .map(|entry| entry.summary.id.clone())
            .collect();
        for id in owned {
            self.check_alive()?;
            super::remove_sealed(&reply_path(&self.root, &id))?;
            self.forget_deleted::<Reply>(&id)?;
        }
        Ok(())
    }

    /// The mapping a reply of `owner` binds and restores with.
    fn owner_mapping(&self, owner: &ReplyOwner) -> Result<Mapping, VaultError> {
        match owner {
            ReplyOwner::Project(id) => Ok(self.load_record::<Project>(id)?.mapping),
            ReplyOwner::Document(id) => {
                let document: Document = self.load_record(id)?;
                match &document.project {
                    Some(project) => Ok(self.load_record::<Project>(project)?.mapping),
                    None => Ok(document.mapping),
                }
            }
        }
    }

    /// Seal `reply` to its file and note it in the cached summaries without
    /// marking the manifest for a rewrite.
    fn write_reply(&self, _lock: &WriteLock<'_>, reply: &Reply) -> Result<(), VaultError> {
        let json = serde_json::to_vec(reply).map_err(io::Error::from)?;
        let path = reply_path(&self.root, &reply.id);
        let stamp = self.write_object(ObjectKind::Reply, &reply.id, &path, &json)?;
        let mut state = self.state();
        let table = Reply::table(&mut state.manifest);
        table.damaged.remove(&reply.id);
        let summary = reply.summary();
        table.ok.insert(reply.id.clone(), Entry { summary, stamp });
        Ok(())
    }
}
