//! Document-local identities. Policy history shares immutable values and follows
//! editor history; typing never clones the mapping index. Nothing is serialized.
use super::*;

#[derive(Clone, Debug)]
pub struct Identity {
    pub id: u64,
    pub alias: String,
    pub category: Category,
    pub owner: Option<u64>,
    pub custom_alias: bool,
}
#[derive(Clone, Default)]
pub struct IdentityPolicy {
    edited: HashMap<u64, Arc<Identity>>,
    /// Variant id → identity, overriding the identity the variant was seeded with.
    variants: HashMap<u64, u64>,
    kept: HashMap<u64, bool>,
}
pub type IdentitySnapshot = Arc<IdentityPolicy>;
#[derive(Default)]
pub(super) struct IdentityStore {
    definitions: HashMap<u64, Arc<Identity>>,
    policy: IdentitySnapshot,
    normalized: HashMap<(Category, String), u64>,
    history: HashMap<u64, IdentitySnapshot>,
}
impl IdentityStore {
    pub fn normalized_identity(&self, value: &str, category: Category) -> Option<u64> {
        self.normalized
            .get(&(category, canonical(value)))
            .copied()
            .filter(|id| self.get(*id).is_some_and(|i| i.category == category))
    }
    pub fn remember_normalized(&mut self, value: &str, category: Category, id: u64) {
        self.normalized.insert((category, canonical(value)), id);
    }
    pub fn add(&mut self, identity: Identity) {
        self.definitions.insert(identity.id, Arc::new(identity));
    }
    fn get(&self, id: u64) -> Option<&Identity> {
        self.policy
            .edited
            .get(&id)
            .or_else(|| self.definitions.get(&id))
            .map(AsRef::as_ref)
    }
    pub fn on_transaction(&mut self, tx: &mdoc_history::EditorTransaction) {
        for change in &tx.changes {
            self.history
                .entry(change.before)
                .or_insert_with(|| self.policy.clone());
            if change.edits.is_empty() {
                if let Some(policy) = self.history.get(&change.after) {
                    self.policy = policy.clone();
                }
            } else {
                self.history.insert(change.after, self.policy.clone());
            }
        }
        self.history.retain(|id, _| tx.retained.contains(id));
    }
}

fn canonical(value: &str) -> String {
    value
        .replace(['«', '»', '“', '”'], "\"")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}
fn name_parts(value: &str) -> Vec<String> {
    value
        .split(|c: char| !c.is_alphabetic())
        .filter(|v| !v.is_empty())
        .map(str::to_lowercase)
        .filter(|v| !matches!(v.as_str(), "ип" | "mr" | "ms" | "mrs" | "dr"))
        .collect()
}
fn surname_related(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    // Suggestions only: Russian case endings with two matching initials. A
    // shared surname or character similarity never establishes identity.
    let stem = |s: &str| {
        for suffix in [
            "овой", "евым", "овым", "еву", "ову", "ева", "ова", "евой", "ину", "ина", "иной", "ым",
            "ой", "ую", "у", "а",
        ] {
            if let Some(stem) = s.strip_suffix(suffix)
                && stem.chars().count() >= 5
            {
                return stem.to_owned();
            }
        }
        s.to_owned()
    };
    stem(a) == stem(b)
}
fn related_name(a: &str, b: &str) -> bool {
    let a = name_parts(a);
    let b = name_parts(b);
    if a.len() != 3 || b.len() != 3 {
        return false;
    }
    if b[1].chars().count() == 1 && b[2].chars().count() == 1 {
        let initials = |x: &str, y: &str| x.chars().next() == y.chars().next();
        return (surname_related(&a[0], &b[0]) && initials(&a[1], &b[1]) && initials(&a[2], &b[2]))
            || (surname_related(&a[2], &b[0]) && initials(&a[0], &b[1]) && initials(&a[1], &b[2]));
    }
    a.iter().zip(&b).all(|(x, y)| surname_related(x, y))
}
impl Review {
    pub fn identity(&self, id: u64) -> Option<&Identity> {
        self.identities.get(id)
    }
    pub fn variant_identity(&self, group: u64) -> Option<u64> {
        self.identities
            .policy
            .variants
            .get(&group)
            .copied()
            .or_else(|| self.variant(group).map(|g| g.identity))
    }
    pub fn occurrence_identity(&self, group: u64, range: &Range<usize>) -> Option<u64> {
        self.tracking
            .assignment(range)
            .map(|a| a.identity)
            .or_else(|| self.variant_identity(group))
    }
    pub fn identity_snapshot(&self) -> IdentitySnapshot {
        self.identities.policy.clone()
    }
    pub fn restore_identity_snapshot(&mut self, snapshot: IdentitySnapshot) {
        self.identities.policy = snapshot;
        self.sync_identity_variants();
    }
    pub fn commit_identity_snapshot(&mut self, before: u64, after: u64, old: IdentitySnapshot) {
        self.identities.history.insert(before, old);
        self.identities
            .history
            .insert(after, self.identities.policy.clone());
    }
    pub(super) fn sync_identity_variants(&mut self) {
        for group in &mut self.candidates.variants {
            group.kept = self
                .identities
                .policy
                .kept
                .get(&group.id)
                .copied()
                .unwrap_or(group.keep_default);
            let id = self
                .identities
                .policy
                .variants
                .get(&group.id)
                .copied()
                .unwrap_or(group.identity);
            if let Some(identity) = self.identities.get(id) {
                group.category = identity.category;
                group.replacement.clone_from(&identity.alias);
            }
        }
    }
    pub fn active_identities(&self) -> Vec<u64> {
        let mut ids: HashSet<_> = self
            .variants()
            .iter()
            .filter(|g| !g.kept)
            .filter_map(|g| self.variant_identity(g.id))
            .collect();
        ids.extend(
            self.applied()
                .iter()
                .map(|a| a.step.identity)
                .filter(|id| *id != 0),
        );
        ids.extend(self.assignments().iter().map(|a| a.identity));
        let mut ids: Vec<_> = ids.into_iter().collect();
        ids.sort_unstable();
        ids
    }
    pub fn identity_count(&self, id: u64) -> usize {
        self.candidates()
            .iter()
            .filter(|o| self.occurrence_identity(o.variant, &o.range) == Some(id))
            .count()
            + self
                .applied()
                .iter()
                .filter(|o| o.step.identity == id)
                .count()
    }
    /// Keep current pending mentions of this identity, respecting occurrence
    /// overrides for homonyms. Refresh once for the entire metadata operation.
    pub fn keep_identity(&mut self, id: u64) {
        let mentions: Vec<_> = self
            .candidates()
            .iter()
            .filter(|c| self.occurrence_identity(c.variant, &c.range) == Some(id))
            .filter_map(|c| {
                self.variant(c.variant)
                    .map(|g| (c.range.clone(), g.original.clone()))
            })
            .collect();
        self.tracking.keep_many(mentions);
        let source = self.source.clone();
        self.refresh(&source);
    }
    pub(super) fn set_kept(&mut self, group: u64, kept: bool) {
        Arc::make_mut(&mut self.identities.policy)
            .kept
            .insert(group, kept);
        self.sync_identity_variants();
    }
    pub fn rename_identity(&mut self, id: u64, alias: &str) -> Result<(), String> {
        if !valid_replacement(alias) {
            return Err("Use an ASCII alias with letters, digits, underscores or hyphens (up to 128 characters).".into());
        }
        if self
            .active_identities()
            .into_iter()
            .any(|other| other != id && self.identity(other).is_some_and(|i| i.alias == alias))
        {
            return Err(
                "That alias belongs to another identity. Use Assign or Merge instead.".into(),
            );
        }
        let mut identity = self
            .identity(id)
            .ok_or("Identity is no longer available.")?
            .clone();
        if identity.alias != alias && self.occupied_tokens.contains(alias) {
            return Err("That alias is already reserved or present in the document.".into());
        }
        if identity.alias != alias
            && self.source.match_indices(alias).any(|(start, _)| {
                let token = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '-';
                !self.source[..start].chars().next_back().is_some_and(token)
                    && !self.source[start + alias.len()..]
                        .chars()
                        .next()
                        .is_some_and(token)
            })
        {
            return Err(
                "That alias is already present in the document. Choose a different alias.".into(),
            );
        }
        identity.alias = alias.into();
        identity.custom_alias = true;
        self.occupied_tokens.insert(alias.into());
        Arc::make_mut(&mut self.identities.policy)
            .edited
            .insert(id, Arc::new(identity));
        self.sync_identity_variants();
        Ok(())
    }
    pub fn recategorize_identity(&mut self, id: u64, category: Category) -> Result<(), String> {
        let mut identity = self
            .identity(id)
            .ok_or("Identity is no longer available.")?
            .clone();
        if identity.category != category {
            if !identity.custom_alias {
                identity.alias = self.allocate_alias(category);
            }
            identity.category = category;
            identity.owner = None;
            Arc::make_mut(&mut self.identities.policy)
                .edited
                .insert(id, Arc::new(identity));
            self.sync_identity_variants();
            if !matches!(category, Category::Person | Category::Organization) {
                let dependents: Vec<_> = self
                    .active_identities()
                    .into_iter()
                    .filter(|target| self.identity(*target).is_some_and(|i| i.owner == Some(id)))
                    .collect();
                for dependent in dependents {
                    self.set_owner(dependent, None)?;
                }
            }
        }
        Ok(())
    }
    /// Preview without reserving a token or mutating policy on every render.
    pub fn next_alias(&self, category: Category) -> String {
        let mut count = self.counters.get(&category).copied().unwrap_or(0);
        loop {
            count += 1;
            let alias = format!("{}_{count}", category.token());
            if !self.occupied_tokens.contains(&alias) {
                return alias;
            }
        }
    }
    pub(super) fn allocate_alias(&mut self, category: Category) -> String {
        let count = self.counters.entry(category).or_default();
        loop {
            *count += 1;
            let alias = format!("{}_{count}", category.token());
            if self.occupied_tokens.insert(alias.clone()) {
                return alias;
            }
        }
    }
    pub fn new_identity(&mut self, category: Category) -> u64 {
        let alias = self.allocate_alias(category);
        self.next_id += 1;
        let id = self.next_id;
        self.identities.add(Identity {
            id,
            alias,
            category,
            owner: None,
            custom_alias: false,
        });
        id
    }
    pub fn assign_variant(&mut self, group: u64, target: u64) -> Result<(), String> {
        self.variant(group)
            .ok_or("Variant is no longer available.")?;
        self.identity(target)
            .ok_or("Identity is no longer available.")?;
        Arc::make_mut(&mut self.identities.policy)
            .variants
            .insert(group, target);
        self.sync_identity_variants();
        Ok(())
    }
    pub fn merge_identity(&mut self, from: u64, target: u64) -> Result<(), String> {
        if from == target {
            return Ok(());
        }
        self.identity(target)
            .ok_or("Identity is no longer available.")?;
        let groups: Vec<_> = self
            .variants()
            .iter()
            .filter(|g| self.variant_identity(g.id) == Some(from))
            .map(|g| g.id)
            .collect();
        for group in groups {
            self.assign_variant(group, target)?;
        }
        let owners: Vec<_> = self
            .active_identities()
            .into_iter()
            .filter(|id| self.identity(*id).is_some_and(|i| i.owner == Some(from)))
            .collect();
        for id in owners {
            self.set_owner(id, Some(target))?;
        }
        Ok(())
    }
    pub fn set_owner(&mut self, id: u64, owner: Option<u64>) -> Result<(), String> {
        let mut identity = self
            .identity(id)
            .ok_or("Identity is no longer available.")?
            .clone();
        if matches!(identity.category, Category::Person | Category::Organization) {
            return Err("Assign ownership to a contact, address or identifier.".into());
        }
        if let Some(owner) = owner
            && !self
                .identity(owner)
                .is_some_and(|i| matches!(i.category, Category::Person | Category::Organization))
        {
            return Err("Choose a person or organization as owner.".into());
        }
        identity.owner = owner;
        Arc::make_mut(&mut self.identities.policy)
            .edited
            .insert(id, Arc::new(identity));
        Ok(())
    }
    pub fn suggestions(&self, group: u64) -> Vec<u64> {
        let Some(variant) = self.variant(group) else {
            return Vec::new();
        };
        let current = self.variant_identity(group);
        let mut ids: Vec<_> = self
            .variants()
            .iter()
            .filter(|g| {
                g.category == Category::Person
                    && g.id != group
                    && related_name(&g.original, &variant.original)
            })
            .filter_map(|g| self.variant_identity(g.id))
            .filter(|id| Some(*id) != current)
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }
    pub(super) fn discover_initial_variants(&mut self, source: &str) {
        static INITIALS: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
        let regex = INITIALS.get_or_init(|| {
            regex::Regex::new(r"[\p{L}][\p{L}'-]{2,}[ \t]+\p{L}\.[ \t]*\p{L}\.")
                .expect("static initials regex")
        });
        // One source pass; surname/signature indexing keeps large documents from
        // being rescanned once per identity. Ambiguous signatures stay separate.
        let mut signatures: HashSet<(String, char, char)> = HashSet::new();
        for group in self.variants() {
            if group.category != Category::Person {
                continue;
            }
            let p = name_parts(&group.original);
            if p.len() == 3 && p.iter().all(|p| p.chars().count() > 1) {
                signatures.insert((
                    p[0].clone(),
                    p[1].chars().next().unwrap(),
                    p[2].chars().next().unwrap(),
                ));
                signatures.insert((
                    p[2].clone(),
                    p[0].chars().next().unwrap(),
                    p[1].chars().next().unwrap(),
                ));
            }
        }
        if signatures.is_empty() {
            return;
        }
        let protected = protected_syntax(source);
        for m in regex.find_iter(source) {
            let parts = name_parts(m.as_str());
            if parts.len() == 3
                && signatures.contains(&(
                    parts[0].clone(),
                    parts[1].chars().next().unwrap(),
                    parts[2].chars().next().unwrap(),
                ))
                && safe_span(m.as_str())
                && !intersects(&protected, &(m.start()..m.end()))
                && exact_boundary(source, &(m.start()..m.end()), m.as_str())
            {
                self.add_seed(m.as_str(), Category::Person);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "20,000 identity policy history storage; excludes editor layout and source matching"]
    fn identity_policy_history_shares_indices_and_prunes_states() {
        let mut review = Review::default();
        let mut source = String::new();
        let mut detections = Vec::new();
        for n in 0..20_000 {
            let start = source.len();
            source.push_str(&format!("Name{n:05}"));
            detections.push(Detection {
                range: start..source.len(),
                category: Category::Person,
                score: 0.9,
                recognizer: Recognizer::Model,
            });
            source.push('\n');
        }
        review.ingest(&source, detections).unwrap();
        assert_eq!(review.identities.definitions.len(), 20_000);
        let policy = review.identity_snapshot();
        for n in 1..=300u64 {
            review
                .identities
                .on_transaction(&mdoc_history::EditorTransaction {
                    revision: n,
                    changes: vec![mdoc_history::HistoryChange {
                        before: n - 1,
                        after: n,
                        edits: vec![mdoc_history::SourceEdit {
                            range: source.len()..source.len(),
                            new_len: 1,
                        }],
                    }],
                    retained: (n.saturating_sub(255)..=n).collect(),
                });
        }
        assert_eq!(review.identities.history.len(), 256);
        assert!(
            review
                .identities
                .history
                .values()
                .all(|state| Arc::ptr_eq(state, &policy))
        );
        let id = review.variants()[0].identity;
        review.rename_identity(id, "CLIENT_1").unwrap();
        assert!(!Arc::ptr_eq(&policy, &review.identity_snapshot()));
        assert_eq!(review.identities.policy.edited.len(), 1);
        assert!(
            review
                .identities
                .definitions
                .values()
                .all(|value| Arc::strong_count(value) == 1)
        );
        assert_eq!(review.source.as_ref(), source);
        eprintln!(
            "IDENTITY_STORAGE identities=20000 retained_states=256 shared_policy_during_typing=true edited_overrides=1 immutable_definitions=20000 (excludes editor snapshots/layout, allocator and matcher costs)"
        );
    }
}
