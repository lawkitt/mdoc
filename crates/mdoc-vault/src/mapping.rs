//! A replacement mapping: the identities a Project's Documents share, or an
//! Unfiled Document keeps for itself. The case store's rules
//! (`mdoc_store::cases::Case`), with stable identity ids and Document ids
//! where a case kept member paths.
use mdoc_pii::{Category, restore::ExportedIdentity};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// One person, organization or other named thing and the aliases it has
/// had. Its id never changes, so a saved reply can name it whatever its
/// alias becomes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    /// 32 lowercase hex digits, random.
    pub id: String,
    pub alias: String,
    /// Earlier aliases, still restored in text sent before a rename.
    #[serde(default)]
    pub previous: Vec<String>,
    /// The category token, e.g. `PERSON`.
    pub category: String,
    #[serde(default)]
    pub custom: bool,
    /// The primary wording first. Empty once forgotten: the aliases stay,
    /// reserved, and are never used again.
    pub originals: Vec<String>,
    /// The Document whose Not a name took wordings out of this identity:
    /// only its undo gives them back, a reserved alias included.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not_a_name_by: Option<String>,
}

impl Identity {
    fn from_exported(id: String, e: &ExportedIdentity) -> Self {
        Self {
            id,
            alias: e.alias.clone(),
            previous: e.previous.clone(),
            category: e.category.token().into(),
            custom: e.custom,
            originals: e.originals.iter().map(|o| o.to_string()).collect(),
            not_a_name_by: None,
        }
    }

    /// As a review takes it; `None` once forgotten.
    pub fn exported(&self) -> Option<ExportedIdentity> {
        let category = *Category::ALL.iter().find(|c| c.token() == self.category)?;
        (!self.originals.is_empty()).then(|| ExportedIdentity {
            alias: self.alias.clone(),
            previous: self.previous.clone(),
            category,
            custom: self.custom,
            originals: self
                .originals
                .iter()
                .map(|o| Arc::from(o.as_str()))
                .collect(),
        })
    }

    /// Whether `alias` is its alias now or was before.
    pub fn names(&self, alias: &str) -> bool {
        self.alias == alias || self.previous.iter().any(|p| p == alias)
    }
}

/// A wording a Document took off as Not a name that the mapping keeps for
/// another Document's sake: that Document leaves it off when it opens.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotName {
    pub document: String,
    pub alias: String,
    pub wording: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mapping {
    #[serde(default)]
    pub identities: Vec<Identity>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub not_names: Vec<NotName>,
}

/// A new random identity id.
pub(crate) fn identity_id() -> String {
    use aes_gcm::aead::Generate;
    <[u8; 16]>::generate()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

impl Mapping {
    pub fn is_empty(&self) -> bool {
        self.identities.is_empty() && self.not_names.is_empty()
    }

    pub fn identity(&self, id: &str) -> Option<&Identity> {
        self.identities.iter().find(|i| i.id == id)
    }

    /// The identity `alias` names now or named before.
    pub fn identity_named(&self, alias: &str) -> Option<&Identity> {
        self.identities.iter().find(|i| i.names(alias))
    }

    /// The identities that still restore, as a review takes them.
    pub fn exported(&self) -> Vec<ExportedIdentity> {
        self.identities
            .iter()
            .filter_map(Identity::exported)
            .collect()
    }

    /// The next generated alias of `category` that no identity has ever used.
    pub fn next_alias(&self, category: Category) -> String {
        let used = |alias: &str| self.identities.iter().any(|i| i.names(alias));
        (1..)
            .map(|n| format!("{}_{n}", category.token()))
            .find(|alias| !used(alias))
            .unwrap_or_default()
    }

    /// Forgotten identities' aliases. Documents reserve these even though
    /// their originals are gone and cannot be seeded as live identities.
    pub fn reserved_aliases(&self) -> Vec<String> {
        self.reserved(|_| true)
    }

    /// [`Self::reserved_aliases`] but those `document`'s own Not a name
    /// reserved, which its undo may give back.
    pub fn reserved_aliases_for(&self, document: &str) -> Vec<String> {
        self.reserved(|identity| identity.not_a_name_by.as_deref() != Some(document))
    }

    fn reserved(&self, keep: impl Fn(&Identity) -> bool) -> Vec<String> {
        self.identities
            .iter()
            .filter(|identity| identity.originals.is_empty() && keep(identity))
            .flat_map(|identity| {
                std::iter::once(identity.alias.clone()).chain(identity.previous.iter().cloned())
            })
            .collect()
    }

    /// Match an identity by its current alias, then earlier aliases, then a
    /// shared original. Alias matches keep homonyms distinct.
    fn position(&self, incoming: &Identity) -> Option<usize> {
        let position =
            |matches: &dyn Fn(&Identity) -> bool| self.identities.iter().position(matches);
        position(&|s| s.alias == incoming.alias)
            .or_else(|| {
                position(&|s| {
                    s.names(&incoming.alias) || incoming.previous.iter().any(|p| s.names(p))
                })
            })
            .or_else(|| position(&|s| incoming.originals.iter().any(|o| s.originals.contains(o))))
    }

    /// The entry of `exported` that is identity `index` when a document,
    /// which did not merge, still exports several entries matching it: the
    /// one with its alias, then one renamed from it, then the one holding
    /// its earliest wording (restored Undo history may not remember the
    /// newer alias). The others are absorbed or split entries, not its
    /// rename.
    fn counterpart<'a>(
        &self,
        index: usize,
        exported: &'a [ExportedIdentity],
    ) -> Option<&'a ExportedIdentity> {
        let stored = &self.identities[index];
        exported
            .iter()
            .find(|e| e.alias == stored.alias)
            .or_else(|| exported.iter().find(|e| e.previous.contains(&stored.alias)))
            .or_else(|| {
                let matching: Vec<_> = exported
                    .iter()
                    .filter(|e| {
                        self.position(&Identity::from_exported(String::new(), e)) == Some(index)
                    })
                    .collect();
                stored.originals.iter().find_map(|o| {
                    matching
                        .iter()
                        .find(|e| e.originals.iter().any(|m| m.as_ref() == o))
                        .copied()
                })
            })
    }

    /// Renames and category changes of surviving identities since
    /// `before`, as (old alias, identity now). Start from each current entry
    /// so a merge's absorbed entries cannot become renames in other
    /// documents; merges and splits are not propagated.
    pub fn renames_since(&self, before: &Self) -> Vec<(String, ExportedIdentity)> {
        self.identities
            .iter()
            .filter_map(|new| {
                let old = &before.identities[before.position(new)?];
                // A split can share originals with an entry that still
                // exists; that new entry is not a rename of the surviving one.
                if old.alias != new.alias && self.identities.iter().any(|s| s.alias == old.alias) {
                    return None;
                }
                if old.alias != new.alias || old.category != new.category {
                    Some((old.alias.clone(), new.exported()?))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Fold a document's current identities into the mapping. Identities
    /// the document no longer uses stay; a renamed alias is kept as an
    /// earlier alias, so old files and replies still restore. A new identity
    /// gets a new id.
    pub fn merge(&mut self, exported: &[ExportedIdentity]) {
        self.merge_from(exported, None);
    }

    /// [`Self::merge`] from `document`, whose undo of a saved Not a name
    /// may give the identity it emptied its wordings again.
    pub fn merge_from(&mut self, exported: &[ExportedIdentity], document: Option<&str>) {
        let ids = vec![None; exported.len()];
        self.merge_with_ids(exported, &ids, document);
    }

    /// [`Self::merge_from`] where each new identity takes the id beside it
    /// in `ids`, when no identity here has it: a moved document's names
    /// keep their ids.
    pub(crate) fn merge_with_ids(
        &mut self,
        exported: &[ExportedIdentity],
        ids: &[Option<String>],
        document: Option<&str>,
    ) {
        for (e, id) in exported.iter().zip(ids) {
            // A forgotten identity retains only its aliases, reserving them
            // and preventing a stale document's save from restoring its
            // originals.
            if self.identities.iter().any(|identity| {
                identity.originals.is_empty()
                    && (document.is_none() || identity.not_a_name_by.as_deref() != document)
                    && (identity.names(&e.alias)
                        || e.previous.iter().any(|alias| identity.names(alias)))
            }) {
                continue;
            }
            let mut incoming = Identity::from_exported(String::new(), e);
            // The current alias first, so homonyms stay apart and a merge
            // finds its target rather than the identity it absorbed; then an
            // earlier alias; then a shared original, whatever the category (a
            // category change renames the alias).
            let found = self.position(&incoming);
            if let Some(index) = found
                && self.identities[index].alias != incoming.alias
                && self
                    .counterpart(index, exported)
                    .is_some_and(|counterpart| !std::ptr::eq(counterpart, e))
            {
                // This document still has the current identity alongside an
                // absorbed or split one. Keep the current identity rather
                // than interpreting the other entry as its rename.
                let mut retained = self.identities[index].clone();
                for alias in std::iter::once(&incoming.alias).chain(&incoming.previous) {
                    if !retained.names(alias) {
                        retained.previous.push(alias.clone());
                    }
                }
                for original in incoming.originals {
                    if !retained.originals.contains(&original) {
                        retained.originals.push(original);
                    }
                }
                incoming = retained;
            }
            // Identities the document merged into this one are now named
            // only by its earlier aliases: fold them in, so no two entries
            // share an alias.
            let mut absorbed = Vec::new();
            let found = found.map(|index| {
                let mut kept = Vec::with_capacity(self.identities.len());
                let mut shifted = index;
                for (i, s) in std::mem::take(&mut self.identities).into_iter().enumerate() {
                    if i != index && incoming.previous.contains(&s.alias) {
                        shifted -= usize::from(i < index);
                        absorbed.push(s);
                    } else {
                        kept.push(s);
                    }
                }
                self.identities = kept;
                shifted
            });
            // An alias moves to one identity only.
            for (i, other) in self.identities.iter_mut().enumerate() {
                if Some(i) != found && other.alias != incoming.alias {
                    other
                        .previous
                        .retain(|p| *p != incoming.alias && !incoming.previous.contains(p));
                }
            }
            match found {
                Some(index) => {
                    let stored = &mut self.identities[index];
                    if stored.alias != incoming.alias && !stored.previous.contains(&stored.alias) {
                        let old = std::mem::take(&mut stored.alias);
                        stored.previous.push(old);
                    }
                    stored.alias = incoming.alias.clone();
                    stored.category = incoming.category.clone();
                    let earlier = incoming.previous.into_iter().chain(
                        absorbed
                            .iter()
                            .flat_map(|a| std::iter::once(&a.alias).chain(&a.previous).cloned()),
                    );
                    for p in earlier {
                        if !stored.previous.contains(&p) {
                            stored.previous.push(p);
                        }
                    }
                    stored.previous.retain(|p| *p != stored.alias);
                    stored.custom = incoming.custom;
                    let mut originals = incoming.originals;
                    let known = std::mem::take(&mut stored.originals);
                    for o in known
                        .into_iter()
                        .chain(absorbed.into_iter().flat_map(|a| a.originals))
                    {
                        if !originals.contains(&o) {
                            originals.push(o);
                        }
                    }
                    stored.originals = originals;
                }
                None => {
                    incoming.id = id
                        .clone()
                        .filter(|id| self.identity(id).is_none())
                        .unwrap_or_else(identity_id);
                    self.identities.push(incoming);
                }
            }
        }
    }

    /// Not a name: each (alias, wording) leaves the identity the alias
    /// names, except where `in_use` finds that identity restored in another
    /// document's saved text. An identity left with no wording keeps its
    /// aliases, reserved. Each identity changed remembers `document`; the
    /// wordings kept for another document's sake are noted for `document`
    /// (`not_names`), replacing what it noted before.
    pub fn drop_wordings(
        &mut self,
        pairs: &[(String, String)],
        document: &str,
        in_use: impl Fn(&Identity) -> bool,
    ) {
        let mut kept = Vec::new();
        for (alias, wording) in pairs {
            let Some(identity) = self.identities.iter_mut().find(|i| i.names(alias)) else {
                continue;
            };
            if !identity.originals.contains(wording) {
                continue;
            }
            if in_use(identity) {
                kept.push((identity.alias.clone(), wording.clone()));
                continue;
            }
            identity.originals.retain(|o| o != wording);
            identity.not_a_name_by = Some(document.to_string());
        }
        self.not_names.retain(|n| n.document != document);
        self.not_names
            .extend(kept.into_iter().map(|(alias, wording)| NotName {
                document: document.to_string(),
                alias,
                wording,
            }));
    }

    /// Undo of a saved Not a name: each (alias, wording) the document shows
    /// again goes back into the identity `document` took it out of.
    pub fn refill(&mut self, pairs: &[(String, String)], document: &str) {
        // The mark stays: another of its wordings may come back by a later
        // undo step.
        for (alias, wording) in pairs {
            if let Some(identity) = self
                .identities
                .iter_mut()
                .find(|i| i.names(alias) && i.not_a_name_by.as_deref() == Some(document))
                && !identity.originals.contains(wording)
            {
                identity.originals.push(wording.clone());
            }
        }
    }

    /// The (alias, wording) pairs `document` took off as Not a name that
    /// the mapping keeps, under each identity's alias now: another document
    /// may have renamed it since.
    pub fn not_names_of(&self, document: &str) -> Vec<(String, String)> {
        self.not_names
            .iter()
            .filter(|n| n.document == document)
            .map(|n| {
                let current = self.identity_named(&n.alias).map_or(&n.alias, |i| &i.alias);
                (current.clone(), n.wording.clone())
            })
            .collect()
    }

    /// Irreversibly discard the originals of the identity `alias` names,
    /// keeping its aliases reserved: the wordings it had, or `None` when
    /// there was nothing to forget.
    pub(crate) fn forget(&mut self, alias: &str) -> Option<Vec<String>> {
        let identity = self
            .identities
            .iter_mut()
            .find(|identity| identity.names(alias))?;
        if identity.originals.is_empty() {
            return None;
        }
        // No Document's undo of a Not a name may give its wordings back.
        identity.not_a_name_by = None;
        Some(std::mem::take(&mut identity.originals))
    }
}
