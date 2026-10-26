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
    pub(crate) fn normalized_identity(&self, value: &str, category: Category) -> Option<u64> {
        self.normalized
            .get(&(category, canonical(value)))
            .copied()
            .filter(|id| self.get(*id).is_some_and(|i| i.category == category))
    }
    pub(crate) fn remember_normalized(&mut self, value: &str, category: Category, id: u64) {
        self.normalized.insert((category, canonical(value)), id);
    }
    pub(crate) fn add(&mut self, identity: Identity) {
        self.definitions.insert(identity.id, Arc::new(identity));
    }
    pub(crate) fn ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.definitions.keys().copied()
    }
    pub(crate) fn aliases(&self) -> impl Iterator<Item = &str> {
        self.ids()
            .filter_map(|id| self.get(id))
            .map(|i| i.alias.as_str())
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
/// Lowercase courtesy titles that precede a name and never identify anyone.
const TITLES: [&str; 7] = ["mr", "mrs", "ms", "miss", "mx", "dr", "prof"];
fn name_parts(value: &str) -> Vec<String> {
    value
        .split(|c: char| !c.is_alphabetic())
        .filter(|v| !v.is_empty())
        .map(str::to_lowercase)
        .filter(|v| v != "ип" && !TITLES.contains(&v.as_str()))
        .collect()
}
/// Whitespace-separated name words, lowercased, without titles or edge
/// punctuation: "Ms. O'Neil," → `["o'neil"]`.
fn name_words(value: &str) -> Vec<String> {
    value
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphabetic()).to_lowercase())
        .filter(|w| !w.is_empty() && !TITLES.contains(&w.as_str()))
        .collect()
}
fn cyrillic(word: &str) -> bool {
    word.chars().all(|c| matches!(c, 'а'..='я' | 'ё' | '-'))
}
fn patronymic(word: &str) -> bool {
    ["ович", "евич", "овна", "евна", "ична"]
        .iter()
        .any(|s| word.ends_with(s))
}
/// How surely a Russian word is a surname: 2 for endings first names almost
/// never have, 1 for "-ина" (Петрова Ирина, Ирина Пушкина), 0 otherwise.
fn russian_surname_score(word: &str) -> u8 {
    const STRONG: [&str; 10] = [
        "ов", "ев", "ёв", "ова", "ева", "ёва", "ский", "ская", "цкий", "цкая",
    ];
    if STRONG.iter().any(|s| word.ends_with(s)) {
        2
    } else if ["ин", "ын", "ина", "ына"].iter().any(|s| word.ends_with(s)) {
        1
    } else {
        0
    }
}
/// The surname and given names of a full person name, or `None` when the
/// wording is not one (a single word, initials) or the surname is ambiguous.
fn split_full_name(value: &str) -> Option<(String, Vec<String>)> {
    let mut words = name_words(value);
    let full = |w: &String| {
        w.chars().filter(|c| c.is_alphabetic()).count() > 1
            && w.chars()
                .all(|c| c.is_alphabetic() || matches!(c, '-' | '\'' | '’'))
    };
    if !(2..=4).contains(&words.len()) || !words.iter().all(full) {
        return None;
    }
    let last = words.len() - 1;
    let surname = if !words.iter().all(|w| cyrillic(w)) {
        last
    } else if words.len() == 3 && patronymic(&words[2]) && !patronymic(&words[1]) {
        0
    } else if words.len() == 3 && patronymic(&words[1]) {
        2
    } else if words.len() == 2 {
        // "Петров Иван" or "Иван Петров"; "Ирина Пушкина" is left unsplit.
        match russian_surname_score(&words[0]).cmp(&russian_surname_score(&words[1])) {
            std::cmp::Ordering::Greater => 0,
            std::cmp::Ordering::Less => 1,
            std::cmp::Ordering::Equal if russian_surname_score(&words[1]) == 0 => 1,
            std::cmp::Ordering::Equal => return None,
        }
    } else {
        last
    };
    let surname = words.remove(surname);
    (surname.chars().count() >= 3).then_some((surname, words))
}
/// Lowercase wordings that stand for `surname` alone: itself and, for a
/// Russian surname, its conservative case forms ("раман" → "рамана",
/// "раману"). Some forms are shared with relatives ("петрова"), which is why
/// they are proposed, never linked.
fn surname_forms(surname: &str) -> Vec<String> {
    let mut forms = vec![surname.to_owned()];
    if cyrillic(surname) {
        let mut decline = |stem: &str, endings: &[&str]| {
            forms.extend(endings.iter().map(|e| format!("{stem}{e}")));
        };
        if let Some(stem) = surname
            .strip_suffix("ий")
            .filter(|s| s.ends_with("ск") || s.ends_with("цк"))
        {
            decline(stem, &["ого", "ому", "им", "ом"]);
        } else if let Some(stem) = surname
            .strip_suffix("ая")
            .filter(|s| s.ends_with("ск") || s.ends_with("цк"))
        {
            decline(stem, &["ой", "ую"]);
        } else if ["ова", "ева", "ёва", "ина", "ына"]
            .iter()
            .any(|s| surname.ends_with(s))
        {
            // A feminine nominative, or the genitive of "Петров".
            let stem = surname.strip_suffix('а').unwrap();
            decline(stem, &["ой", "у", "", "ым", "е"]);
        } else if surname
            .chars()
            .next_back()
            .is_some_and(|c| !"аеёиоуыэюяйь-".contains(c))
        {
            let instrumental = if ["ов", "ев", "ёв", "ин", "ын"]
                .iter()
                .any(|s| surname.ends_with(s))
            {
                "ым"
            } else {
                "ом"
            };
            decline(surname, &["а", "у", instrumental, "е"]);
        }
    }
    forms.sort_unstable();
    forms.dedup();
    forms
}
/// Whether `short` is a title + surname or a bare surname, possibly in a
/// Russian case form, of the full name `full`: "Ms Raman", "Рамана".
fn short_form_of(short: &str, full: &str) -> bool {
    let words = name_words(short);
    words.len() == 1
        && split_full_name(full)
            .is_some_and(|(surname, _)| surname_forms(&surname).contains(&words[0]))
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
/// Whether `part` occurs as whole words inside the longer `whole` ("Raman" in
/// "Priya Raman"): a proposed link, never an automatic one.
/// Containment suggests only names: "Alice" inside `Alice@example.com` is not
/// that email's identity.
fn is_name(category: Category) -> bool {
    matches!(category, Category::Person | Category::Organization)
}
fn contained_name(whole: &str, part: &str) -> bool {
    whole.len() > part.len()
        && whole
            .match_indices(part)
            .any(|(start, _)| exact_boundary(whole, &(start..start + part.len()), part))
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
        self.revision.advance();
        self.identities.policy = snapshot;
        self.sync_identity_variants();
    }
    pub fn commit_identity_snapshot(&mut self, before: u64, after: u64, old: IdentitySnapshot) {
        self.revision.advance();
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
    /// Identities that still own a pending or applied mention. One whose
    /// mentions all moved elsewhere frees its alias.
    pub fn active_identities(&self) -> Vec<u64> {
        let mut ids: HashSet<_> = self
            .candidates()
            .iter()
            .filter(|c| self.variant(c.variant).is_some_and(|g| !g.kept))
            .filter_map(|c| self.occurrence_identity(c.variant, &c.range))
            .collect();
        ids.extend(
            self.applied()
                .iter()
                .map(|a| a.step.identity)
                .filter(|id| *id != 0),
        );
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
    pub(super) fn set_kept(&mut self, group: u64, kept: bool) {
        Arc::make_mut(&mut self.identities.policy)
            .kept
            .insert(group, kept);
        self.sync_identity_variants();
    }
    pub fn rename_identity(&mut self, id: u64, alias: &str) -> Result<(), String> {
        self.revision.advance();
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
        // No mention uses the alias any more (checked above): identities left
        // idle by earlier moves hand it over and take fresh ones.
        let idle: Vec<_> = self
            .identities
            .ids()
            .filter(|&other| other != id && self.identity(other).is_some_and(|i| i.alias == alias))
            .collect();
        for other in idle {
            let mut idle = self.identity(other).unwrap().clone();
            idle.alias = self.allocate_alias(idle.category);
            idle.custom_alias = false;
            Arc::make_mut(&mut self.identities.policy)
                .edited
                .insert(other, Arc::new(idle));
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
        self.revision.advance();
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
    /// The identity currently called `alias`, preferring one with mentions.
    pub fn identity_by_alias(&self, alias: &str) -> Option<u64> {
        let active = self.active_identities();
        active
            .iter()
            .copied()
            .chain(self.identities.ids())
            .find(|id| self.identity(*id).is_some_and(|i| i.alias == alias))
    }
    /// Give several identities new aliases (and categories) at once, as a case
    /// does when a document joins it. The result must leave active aliases
    /// unique; nothing changes otherwise.
    pub fn reassign_aliases(
        &mut self,
        changes: &[(u64, String, Category, bool)],
    ) -> Result<(), String> {
        self.revision.advance();
        let mut aliases: HashMap<u64, String> = self
            .active_identities()
            .into_iter()
            .filter_map(|id| Some((id, self.identity(id)?.alias.clone())))
            .collect();
        for (id, alias, _, _) in changes {
            if !valid_replacement(alias) || self.identity(*id).is_none() {
                return Err("Identity or alias is no longer valid.".into());
            }
            aliases.insert(*id, alias.clone());
        }
        let mut seen = HashSet::new();
        if aliases.values().any(|a| !seen.insert(a)) {
            return Err("Two identities would share an alias.".into());
        }
        for (id, alias, category, custom) in changes {
            let mut identity = self.identity(*id).unwrap().clone();
            if identity.category != *category {
                identity.owner = None;
            }
            identity.alias = alias.clone();
            identity.category = *category;
            identity.custom_alias = *custom;
            self.occupied_tokens.insert(alias.clone());
            if let Some((c, n)) = crate::restore::parse_alias(alias) {
                let counter = self.counters.entry(c).or_default();
                *counter = (*counter).max(n as usize);
            }
            Arc::make_mut(&mut self.identities.policy)
                .edited
                .insert(*id, Arc::new(identity));
        }
        // Contacts owned by an identity that is no longer a person or
        // organization lose that owner, as in `recategorize_identity`.
        let demoted: HashSet<u64> = changes
            .iter()
            .filter(|(_, _, c, _)| !matches!(c, Category::Person | Category::Organization))
            .map(|c| c.0)
            .collect();
        if !demoted.is_empty() {
            let dependents: Vec<_> = self
                .identities
                .ids()
                .filter(|id| {
                    self.identity(*id)
                        .and_then(|i| i.owner)
                        .is_some_and(|owner| demoted.contains(&owner))
                })
                .collect();
            for id in dependents {
                let mut identity = self.identity(id).unwrap().clone();
                identity.owner = None;
                Arc::make_mut(&mut self.identities.policy)
                    .edited
                    .insert(id, Arc::new(identity));
            }
        }
        // An idle identity that held one of these aliases takes a fresh one.
        let moved: HashSet<_> = changes.iter().map(|c| c.0).collect();
        let claimed: HashSet<_> = changes.iter().map(|c| c.1.clone()).collect();
        let idle: Vec<_> = self
            .identities
            .ids()
            .filter(|id| !moved.contains(id))
            .filter(|id| {
                self.identity(*id)
                    .is_some_and(|i| claimed.contains(&i.alias))
            })
            .collect();
        for id in idle {
            let mut identity = self.identity(id).unwrap().clone();
            identity.alias = self.allocate_alias(identity.category);
            identity.custom_alias = false;
            Arc::make_mut(&mut self.identities.policy)
                .edited
                .insert(id, Arc::new(identity));
        }
        self.sync_identity_variants();
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
        self.revision.advance();
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
        self.revision.advance();
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
        self.revision.advance();
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
        self.revision.advance();
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
                g.id != group
                    && ((g.category == Category::Person
                        && related_name(&g.original, &variant.original))
                        || (g.category == Category::Person
                            && variant.category == Category::Person
                            && short_form_of(&variant.original, &g.original))
                        || (is_name(g.category)
                            && is_name(variant.category)
                            && contained_name(&g.original, &variant.original)))
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
    /// Propose title + surname, the bare surname and its Russian case forms of
    /// each full person name outside existing mentions, as variants of their
    /// own that Same as… offers to link (ADR 0068). A word that is also a
    /// known first name is never proposed.
    pub(super) fn discover_short_variants(&mut self, source: &str) {
        static WORD: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
        // Abbreviated titles in any case ("MR SMITH", "mr Smith"); "miss" in
        // lowercase is the verb ("did not miss Raman").
        let regex = WORD.get_or_init(|| {
            regex::Regex::new(
                r"\b(?:(?:(?i:Mr|Mrs|Ms|Mx|Dr|Prof)|Miss|MISS)\.?[ \t]+)?\p{Lu}\p{L}*(?:['’-]\p{Lu}\p{L}*)*",
            )
            .expect("static surname regex")
        });
        // Mentions of the seeds so far, so a surname inside a full name or a
        // longer wording stays part of it.
        self.refresh(source);
        // A name the user kept everywhere yields no forms; given names of every
        // full name stay unproposed.
        let applied: HashSet<&str> = self
            .applied()
            .iter()
            .map(|a| a.step.original_shared().as_ref())
            .collect();
        let mut forms = HashSet::new();
        let mut given = HashSet::new();
        for group in self.variants() {
            if group.category != Category::Person {
                continue;
            }
            let Some((surname, names)) = split_full_name(&group.original) else {
                continue;
            };
            if !group.kept
                && (!group.mentions.is_empty() || applied.contains(group.original.as_ref()))
            {
                forms.extend(surname_forms(&surname));
            }
            given.extend(names);
        }
        forms.retain(|form| !given.contains(form));
        if forms.is_empty() {
            return;
        }
        // Pending, applied and kept mentions, merged so `intersects` can
        // search them.
        let mut spans: Vec<_> = self
            .candidates()
            .iter()
            .map(|c| c.range.clone())
            .chain(self.applied().iter().map(|a| a.range.clone()))
            .chain(self.tracking.exclusions.iter().map(|e| e.range.clone()))
            .collect();
        spans.sort_by_key(|r| r.start);
        let mut covered: Vec<Range<usize>> = Vec::with_capacity(spans.len());
        for span in spans {
            match covered.last_mut() {
                Some(last) if span.start < last.end => last.end = last.end.max(span.end),
                _ => covered.push(span),
            }
        }
        let protected = protected_syntax(source);
        let found: Vec<_> = regex
            .find_iter(source)
            .filter(|m| {
                let word = m.as_str().rsplit([' ', '\t']).next().unwrap_or_default();
                forms.contains(&word.to_lowercase())
                    && safe_span(m.as_str())
                    && !intersects(&protected, &m.range())
                    && !intersects(&covered, &m.range())
                    && exact_boundary(source, &m.range(), m.as_str())
            })
            .map(|m| m.as_str().to_owned())
            .collect();
        for wording in found {
            self.add_seed(&wording, Category::Person);
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

    #[test]
    fn surnames_follow_russian_name_order_and_skip_ambiguous_pairs() {
        let surname = |name| split_full_name(name).map(|(surname, _)| surname);
        assert_eq!(
            surname("Павлова Марина Сергеевна").as_deref(),
            Some("павлова")
        );
        assert_eq!(
            surname("Марина Сергеевна Павлова").as_deref(),
            Some("павлова")
        );
        assert_eq!(surname("Петров Иван").as_deref(), Some("петров"));
        assert_eq!(surname("Иван Петров").as_deref(), Some("петров"));
        assert_eq!(surname("Марина Павлова").as_deref(), Some("павлова"));
        assert_eq!(surname("Ирина Пушкина"), None);
        assert_eq!(surname("Ms Maya Bennett").as_deref(), Some("bennett"));
        assert_eq!(surname("Павлова М.С."), None);
        assert_eq!(surname("Bennett"), None);
    }
}
