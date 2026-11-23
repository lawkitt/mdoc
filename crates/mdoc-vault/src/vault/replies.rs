//! AI Replies (#1091 Decisions 4, 16, 18 and 20): saved as received, with
//! each alias bound to the identity it meant, and restored from those
//! identities' originals as they are now.
use super::{FaultHook, ManifestCheck, Vault, WriteLock};
use crate::{
    ObjectKind, VaultError, is_id,
    manifest::{Entry, Record, reply_path},
    mapping::{Identity, Mapping},
    new_id,
    records::{
        Document, DocumentSummary, NewReply, Project, ProjectSummary, Reply, ReplyOwner,
        ReplySummary, now,
    },
};
use mdoc_pii::restore::{RestoreMap, RestoredText};
use std::{io, path::PathBuf};

/// The Restore extension's handle (#1091 Decision 20): it reads the
/// manifest, Projects and Documents, and writes nothing but new AI Reply
/// files. The manifest is checked against the object files at open and on
/// [`Self::refresh`] as [`Vault`]'s is, but only in memory: it is never
/// written, not even when found stale or missing, so the app's cache is
/// left alone and the app picks the reply up through its own
/// [`Vault::refresh`]. The folder must exist already, and no temp file is
/// swept.
pub struct ReplyHandle {
    vault: Vault,
}

impl ReplyHandle {
    /// Open the Vault in `root`, an existing folder, for replies only.
    /// [`VaultError::WrongKey`] as [`Vault::open`] refuses it.
    pub fn open(root: impl Into<PathBuf>, key: &[u8; 32]) -> Result<Self, VaultError> {
        let vault = Vault::open_inner(root.into(), key, None, false)?;
        Ok(Self { vault })
    }

    /// [`Self::open`] with a hook run at every write step (tests).
    pub fn open_with_hook(
        root: impl Into<PathBuf>,
        key: &[u8; 32],
        hook: FaultHook,
    ) -> Result<Self, VaultError> {
        let vault = Vault::open_inner(root.into(), key, Some(hook), false)?;
        Ok(Self { vault })
    }

    /// How the manifest was found when the handle opened; it was not
    /// rewritten either way.
    pub fn opened(&self) -> ManifestCheck {
        self.vault.opened()
    }

    /// Pick up what other handles wrote; `true` when anything changed.
    pub fn refresh(&self) -> Result<bool, VaultError> {
        self.vault.refresh()
    }

    /// [`Vault::projects`].
    pub fn projects(&self) -> Vec<ProjectSummary> {
        self.vault.projects()
    }

    /// [`Vault::documents`].
    pub fn documents(&self) -> Vec<DocumentSummary> {
        self.vault.documents()
    }

    /// [`Vault::documents_in`].
    pub fn documents_in(&self, project: Option<&str>) -> Vec<DocumentSummary> {
        self.vault.documents_in(project)
    }

    pub fn project(&self, id: &str) -> Result<Project, VaultError> {
        self.vault.project(id)
    }

    pub fn document(&self, id: &str) -> Result<Document, VaultError> {
        self.vault.document(id)
    }

    /// [`Vault::save_reply`]: only the reply's file is written.
    pub fn save_reply(&self, new: NewReply) -> Result<Reply, VaultError> {
        self.vault.save_reply(new)
    }

    pub fn reply(&self, id: &str) -> Result<Reply, VaultError> {
        self.vault.reply(id)
    }

    /// [`Vault::reply_map`].
    pub fn reply_map(&self, reply: &Reply) -> Result<RestoreMap, VaultError> {
        self.vault.reply_map(reply)
    }

    /// [`Vault::reply_map_for`].
    pub fn reply_map_for(
        &self,
        owner: &ReplyOwner,
        markdown: &str,
    ) -> Result<RestoreMap, VaultError> {
        self.vault.reply_map_for(owner, markdown)
    }

    /// [`Vault::restore_reply`].
    pub fn restore_reply(&self, id: &str) -> Result<RestoredText, VaultError> {
        self.vault.restore_reply(id)
    }

    /// [`Vault::project_replies`].
    pub fn project_replies(&self, project: &str) -> Vec<ReplySummary> {
        self.vault.project_replies(project)
    }

    /// [`Vault::document_replies`].
    pub fn document_replies(&self, document: &str) -> Vec<ReplySummary> {
        self.vault.document_replies(document)
    }
}

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

    /// What `markdown` would restore with if it were saved now as a reply
    /// of `owner`: the identities its aliases name in the owner's mapping
    /// now, as [`Self::save_reply`] would bind them. For showing a reply
    /// restored before it is saved.
    pub fn reply_map_for(
        &self,
        owner: &ReplyOwner,
        markdown: &str,
    ) -> Result<RestoreMap, VaultError> {
        let mapping = self.owner_mapping(owner)?;
        Ok(mapping.reply_map(&mapping.bindings(markdown)))
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
        sort_newest_first(&mut replies);
        replies
    }

    /// A Project's AI Replies, newest first: its own and those of the
    /// Documents in it now, which followed them there.
    pub fn project_replies(&self, project: &str) -> Vec<ReplySummary> {
        // One look at the cache, so a move meanwhile cannot split the two.
        let mut replies: Vec<ReplySummary> = {
            let state = self.state();
            let in_project = |document: &str| {
                (state.manifest.documents.ok.get(document))
                    .is_some_and(|entry| entry.summary.project.as_deref() == Some(project))
            };
            (state.manifest.replies.ok.values())
                .filter(|entry| match &entry.summary.owner {
                    ReplyOwner::Project(id) => id == project,
                    ReplyOwner::Document(id) => in_project(id),
                })
                .map(|entry| entry.summary.clone())
                .collect()
        };
        sort_newest_first(&mut replies);
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
    /// handle saved are found first. A reply that cannot be read for now
    /// refuses the delete, as it may be `owner`'s; one that never will
    /// (another key, damaged) is passed over.
    pub(super) fn delete_replies_of(
        &self,
        _lock: &WriteLock<'_>,
        owner: &ReplyOwner,
    ) -> Result<(), VaultError> {
        let (replies, _) = self.current_replies()?;
        for reply in replies.into_iter().filter(|r| r.owner == *owner) {
            self.check_alive()?;
            super::remove_sealed(&reply_path(&self.root, &reply.id))?;
            self.forget_deleted::<Reply>(&reply.id)?;
        }
        Ok(())
    }

    /// Every reply as the folder holds it now, other handles' included,
    /// and whether any does not open. An I/O error reading one is passed
    /// on: it may open later.
    pub(super) fn current_replies(&self) -> Result<(Vec<ReplySummary>, bool), VaultError> {
        self.refresh()?;
        let unopened: Vec<String> =
            (self.state().manifest.replies.damaged.keys().cloned()).collect();
        for id in &unopened {
            if let Err(VaultError::Io(error)) = self.load_record::<Reply>(id) {
                return Err(VaultError::Io(error));
            }
        }
        Ok((self.replies(), !unopened.is_empty()))
    }

    /// The identities in `from` that `document`'s replies are bound to and
    /// that the move did not carry with its text (`carried`): they go along
    /// too, so the replies still restore where the document goes.
    pub(super) fn reply_identities(
        &self,
        document: &str,
        from: &Mapping,
        carried: &[Vec<String>],
    ) -> Result<Vec<Identity>, VaultError> {
        let (replies, _) = self.current_replies()?;
        let mut identities: Vec<Identity> = Vec::new();
        for summary in replies {
            if !matches!(&summary.owner, ReplyOwner::Document(id) if id == document) {
                continue;
            }
            let reply = match self.load_record::<Reply>(&summary.id) {
                Err(VaultError::Io(error)) => return Err(VaultError::Io(error)),
                Err(_) => continue,
                Ok(reply) => reply,
            };
            for binding in &reply.bindings {
                let Some(identity) = from.bound(&binding.identity) else {
                    continue;
                };
                let known = |id: &String| {
                    carried.iter().flatten().any(|c| c == id)
                        || identities.iter().any(|i| i.id == *id)
                };
                if identity.exported().is_some() && !known(&identity.id) {
                    identities.push(identity.clone());
                }
            }
        }
        Ok(identities)
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

fn sort_newest_first(replies: &mut [ReplySummary]) {
    replies.sort_by(|a, b| {
        b.created
            .cmp(&a.created)
            .then_with(|| a.title.cmp(&b.title))
            .then_with(|| a.id.cmp(&b.id))
    });
}
