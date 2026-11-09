//! People: bare initials after a role, titles, programmes' titles and
//! person documents ("Ostrand Decl.").
use super::defined::definitions;
use super::shapes::bare;
use super::{Pack, Role, Span};
use mdoc_pii::{Category, Detection};
use regex::Regex;

impl Pack {
    /// The role that introduces bare initials at the end of `before`, a
    /// line's text up to them (#491): its start 5 in "Он и потерпевший "
    /// before "КЛМ". The longest role counts ("законный представитель"
    /// over "представитель"); none, `None`.
    pub(crate) fn initials_role(&self, before: &str) -> Option<Role> {
        self.initials_after
            .iter()
            .filter_map(|(regex, role)| {
                let start = regex.captures(before)?.name("role")?.start();
                Some(Role { start, ..*role })
            })
            .min_by_key(|role| role.start)
    }

    /// Whether `word`, in capitals, is never bare initials after a role
    /// (#491): an `initials_never` word («ЖКХ», «НЕ»), or a public body's
    /// word the `never` entries with `anywhere_in` list («МЧС», «ФМС»,
    /// «УФК»), which [`Self::never`] leaves to the spans holding them.
    pub(crate) fn initials_never(&self, word: &str) -> bool {
        let lower = word.to_lowercase();
        self.initials_never.iter().any(|never| never == word)
            || self
                .never
                .iter()
                .filter(|entry| !entry.anywhere_in.is_empty())
                .any(|entry| entry.has(&lower))
    }

    /// Whether `source` defines `word` as a company's short name: «ООО
    /// «Кедр Строй Сервис» (далее – КСС)». The company must end the clause
    /// right before the bracket, after its last comma: a legal form
    /// (`initials_companies`) last («Кедр ООО (далее – КСС)»), or a quoted
    /// name last that a legal form or the quote opens («ООО «Кедр»», «
    /// «Кедр»»). A person beside a company («Иванов, работник ООО
    /// «Ромашка» (далее – ИИИ)») defines no company's name (#491).
    pub(crate) fn defined_company(&self, source: &str, word: &str) -> bool {
        let form = |part: &str| self.initials_companies.iter().any(|f| f == part);
        definitions(source).iter().any(|definition| {
            let company = definition
                .subject
                .rsplit(',')
                .next()
                .unwrap_or_default()
                .trim();
            let mut words = company
                .split(|c: char| !c.is_alphanumeric())
                .filter(|word| !word.is_empty());
            let first = words.next();
            let last = words.next_back().or(first);
            definition.term == word
                && (last.is_some_and(form)
                    || company.ends_with(['»', '"', '”', '“'])
                        && (company.starts_with(['«', '"', '“', '„']) || first.is_some_and(form)))
        })
    }

    /// Whether `before`, a line's text up to bare initials, ends with a
    /// legal form (`initials_companies`), an opening quote and spaces
    /// aside: "АО " or "ООО «" before "БАВ" make the letters a company's
    /// (#491).
    pub(crate) fn initials_company(&self, before: &str) -> bool {
        let before = before.trim_end_matches(|c: char| c.is_whitespace() || "«\"„“*_".contains(c));
        self.initials_companies.iter().any(|form| {
            before.strip_suffix(form.as_str()).is_some_and(|rest| {
                !rest
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_alphanumeric() || c == '-')
            })
        })
    }

    /// Lowercase words of the person spans in `detections` that name
    /// someone without context ("Dawn French", not "New York" or "Federal
    /// Election Commission"): a common word among them is that person's
    /// name wherever it stands alone ("French").
    pub(super) fn named_words(
        &self,
        view: &mdoc_pii::view::View,
        detections: &[Detection],
    ) -> std::collections::HashSet<String> {
        detections
            .iter()
            .filter(|d| d.category == Category::Person && !d.range.is_empty())
            .filter(|d| !self.determined_role(&view.text, view.view_range(d.range.clone())))
            .map(|d| &view.text[view.view_range(d.range.clone())])
            .filter(|text| self.identifying_named(text, Category::Person, &|_| false, false))
            .filter(|text| !self.programme_title(text, false))
            .flat_map(|text| {
                text.split_whitespace()
                    .map(|word| bare(word).to_lowercase())
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// Whether a person span is a programme's title by an
    /// `unless_last_named` pattern ("CAFTA-DR Environmental Cooperation
    /// Program"), `titled` saying whether a title stands before it.
    pub(super) fn programme_title(&self, text: &str, titled: bool) -> bool {
        self.never
            .iter()
            .filter(|entry| entry.unless_last_named && !(titled && entry.unless_titled))
            .flat_map(|entry| &entry.patterns)
            .any(|pattern| pattern.is_match(text))
    }

    /// Whether `source` names the last word of the person span `text`: a
    /// person span has it beside a name (`named`), or a title stands
    /// before it as a whole word ("Ms Standard").
    pub(super) fn last_word_named(
        &self,
        source: &str,
        text: &str,
        named: &std::collections::HashSet<String>,
    ) -> bool {
        let Some(last) = text.split_whitespace().map(bare).next_back() else {
            return false;
        };
        let word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '-');
        named.contains(&last.to_lowercase())
            || !last.is_empty()
                && source.match_indices(last).any(|(at, _)| {
                    !word(source[..at].chars().next_back())
                        && !word(source[at + last.len()..].chars().next())
                        && self.titled(source, at)
                })
    }

    /// Whether a person's title stands just before `at` in `source` ("Mr
    /// He", "Dr. General", "г-н Суд"): the person fixes' `trim` words.
    pub(super) fn titled(&self, source: &str, at: usize) -> bool {
        let before = source[..at].trim_end();
        let before = before.strip_suffix('.').unwrap_or(before);
        let word = before
            .rsplit(|c: char| !c.is_alphanumeric() && c != '-')
            .next()
            .unwrap_or_default()
            .to_lowercase();
        !word.is_empty()
            && self.spans.iter().any(|span| match span {
                Span::Fix(fix) => fix.category == Category::Person && fix.trim.contains(&word),
                Span::Pattern { .. } => false,
            })
    }

    /// An organization span ending in a `person_before` word ("Ostrand
    /// Decl.") becomes the person before it ("Ostrand") when a person's span
    /// holds each of that name's words: a declaration is a person's, never
    /// a company. The same suffix comes off an existing person's span in
    /// a document citation ("Ostrand Decl. ¶ 3", #499).
    pub(super) fn person_documents(&self, source: &str, detections: &mut [Detection]) {
        let words: Vec<&String> = self
            .spans
            .iter()
            .filter_map(|span| match span {
                Span::Fix(fix) => Some(&fix.person_before),
                Span::Pattern { .. } => None,
            })
            .flatten()
            .collect();
        if words.is_empty() {
            return;
        }
        let name_words = |text: &str| -> Vec<String> {
            text.split_whitespace()
                .map(|word| word.trim_matches(|c: char| !c.is_alphanumeric() && c != '-'))
                .filter(|word| !word.is_empty())
                .map(str::to_owned)
                .collect()
        };
        let people: Vec<Vec<String>> = detections
            .iter()
            .filter(|d| d.category == Category::Person)
            .map(|d| name_words(&source[d.range.clone()]))
            .collect();
        for detection in detections
            .iter_mut()
            .filter(|d| matches!(d.category, Category::Organization | Category::Person))
        {
            let text = source[detection.range.clone()].trim_end();
            let Some((name, last)) = text.rsplit_once(char::is_whitespace) else {
                continue;
            };
            let last = last.trim_end_matches('.').to_lowercase();
            let name = name.trim_end();
            let wanted = name_words(name);
            // An existing PERSON needs citation punctuation after the
            // abbreviated document word. A surname at a sentence end
            // ("Ivo Decl.") or without the dot stays as the model gave it.
            let after = &source[detection.range.end..];
            let citation_tail = if text.ends_with('.') {
                Some(after)
            } else {
                after.strip_prefix('.')
            }
            .is_some_and(|after| {
                let after = after.trim_start_matches([' ', '\t']);
                let after = after
                    .strip_prefix(',')
                    .unwrap_or(after)
                    .trim_start_matches([' ', '\t']);
                static PAGE_LINE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
                    Regex::new(r"^\d+:\d+(?:[-–](?:\d+:)?\d+)?(?:$|[\s),;.\]])")
                        .expect("constant page-line pinpoint pattern")
                });
                // "Ex. 1 to Penrow Decl." explicitly names a document;
                // punctuation plus another span's given name does not.
                static EXHIBIT: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
                    Regex::new(r"\bEx\.[\p{Zs}\t]+\d+[\p{Zs}\t]+to[\p{Zs}\t]+$")
                        .expect("constant exhibit document pattern")
                });
                static AT_PAGE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
                    // A clock time or a street address after "at"
                    // supplies no document pinpoint evidence.
                    Regex::new(r"^at[\p{Zs}\t]+\d+(?:[-–]\d+)?(?:[\p{Zs}\t]*$|[\p{Zs}\t]*[),;.\]])")
                        .expect("constant at-page pinpoint pattern")
                });
                after.starts_with('¶')
                    || PAGE_LINE.is_match(after)
                    || (after.is_empty() || after.starts_with([')', ';']))
                        && EXHIBIT.is_match(&source[..detection.range.start])
                    || AT_PAGE.is_match(after)
            });
            // "Ivo Affleck" corroborates "Ivo Aff", not an affidavit
            // label. Sharing only a given name proves no document label.
            let abbreviated_surname = people.iter().any(|person| {
                person.len() == wanted.len() + 1
                    && wanted
                        .iter()
                        .zip(person)
                        .all(|(a, b)| a.eq_ignore_ascii_case(b))
                    && person.last().is_some_and(|surname| {
                        surname.len() > last.len() && surname.to_lowercase().starts_with(&last)
                    })
            });
            if !words.iter().any(|word| **word == last)
                || wanted.is_empty()
                || abbreviated_surname
                || if detection.category == Category::Person {
                    !citation_tail
                } else {
                    !people
                        .iter()
                        .any(|person| wanted.iter().all(|word| person.contains(word)))
                }
            {
                continue;
            }
            let at = detection.range.start + (text.len() - text.trim_start().len());
            let name = name.trim_start();
            detection.range = at..at + name.len();
            detection.category = Category::Person;
        }
    }
}
