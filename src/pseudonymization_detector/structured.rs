//! Selected structured recognizers. Declining a value never rejects model output.
//! Heuristic rule evidence is ranked separately from model probabilities.
use crate::pseudonymization::{Category, Detection, Recognizer};
use presidio_analyzer::{Pattern, PatternRecognizer, country, predefined};
use regex::Regex;
use std::sync::OnceLock;

struct Rule {
    recognizer: PatternRecognizer,
    category: Category,
    evidence: Recognizer,
    score: f32,
    label: Option<Regex>,
}
struct Rules([Rule; 4]);
impl Rules {
    fn new() -> Self {
        let label = |body: &str| {
            Regex::new(&format!(r#"(?i)\b(?:{body})\b[ \t:№=#_'"-]{{0,8}}$"#))
                .expect("static context regex")
        };
        // Preserve mdoc's frozen EN/RU email coverage; the upstream default is
        // ASCII-only and would turn Cyrillic local parts into partial matches.
        let mut email = predefined::email();
        email.patterns = vec![Pattern::new(
            "mdoc Unicode email",
            r"[\p{L}\p{N}][\p{L}\p{N}._%+-]*@[\p{L}\p{N}](?:[\p{L}\p{N}-]*[\p{L}\p{N}])?(?:\.[\p{L}\p{N}](?:[\p{L}\p{N}-]*[\p{L}\p{N}])?)+",
            0.9,
        )];
        // Narrow the upstream SNILS formats to the existing contract, including
        // tab separators. Label/boundary requirements remain app policy.
        let mut snils = country::ru_snils().with_validator(snils_valid);
        snils.patterns = vec![
            Pattern::new("mdoc contiguous SNILS", r"[0-9]{9,20}", 0.95),
            Pattern::new(
                "mdoc formatted SNILS",
                r"[0-9]{3}-[0-9]{3}-[0-9]{3}[ \t]?[0-9]{2}",
                0.95,
            ),
        ];
        // Presidio 0.1.11 has no INN recognizer; only its pattern/validator
        // configuration is shared. Keep the previously verified local checksum.
        let inn = PatternRecognizer::new(
            "MdocInnRecognizer",
            "RU_INN",
            vec![Pattern::new("mdoc INN", r"[0-9]{9,20}", 0.95)],
        )
        .with_validator(|value| Some(inn_valid(value)));
        let registration = PatternRecognizer::new(
            "MdocRegistrationRecognizer",
            "RU_REGISTRATION",
            vec![Pattern::new("OGRN/OGRNIP", r"[0-9]{13,20}", 0.95)],
        )
        .with_validator(|value| Some(registration_valid(value)));
        Self([
            Rule {
                recognizer: email,
                category: Category::Email,
                evidence: Recognizer::Email,
                score: 0.9,
                label: None,
            },
            Rule {
                recognizer: inn,
                category: Category::Tax,
                evidence: Recognizer::Inn,
                score: 0.95,
                label: Some(label("инн|inn|tax id")),
            },
            Rule {
                recognizer: registration,
                category: Category::Identity,
                evidence: Recognizer::Ogrn,
                score: 0.95,
                label: Some(label("огрн|огрнип|ogrn|ogrnip")),
            },
            Rule {
                recognizer: snils,
                category: Category::Identity,
                evidence: Recognizer::Snils,
                score: 0.95,
                label: Some(label("снилс|snils")),
            },
        ])
    }
}
fn context(text: &str, start: usize, label: &Regex) -> bool {
    let suffix: String = text[..start]
        .chars()
        .rev()
        .take_while(|&c| c != '\n')
        .take(80)
        .collect();
    label.is_match(&suffix.chars().rev().collect::<String>())
}
fn boundary(text: &str, start: usize, end: usize) -> bool {
    !text[..start]
        .chars()
        .next_back()
        .is_some_and(|c| c.is_ascii_digit())
        && !text[end..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit())
}
fn inn_valid(value: &str) -> bool {
    if !value.bytes().all(|b| b.is_ascii_digit()) || value.bytes().all(|b| b == b'0') {
        return false;
    }
    let d: Vec<u32> = value.bytes().map(|b| u32::from(b - b'0')).collect();
    let check = |w: &[u32]| d.iter().zip(w).map(|(d, w)| d * w).sum::<u32>() % 11 % 10;
    match d.len() {
        10 => check(&[2, 4, 10, 3, 5, 9, 4, 6, 8]) == d[9],
        12 => {
            check(&[7, 2, 4, 10, 3, 5, 9, 4, 6, 8]) == d[10]
                && check(&[3, 7, 2, 4, 10, 3, 5, 9, 4, 6, 8]) == d[11]
        }
        _ => false,
    }
}
fn registration_valid(value: &str) -> bool {
    if !value.bytes().all(|b| b.is_ascii_digit()) || value.bytes().all(|b| b == b'0') {
        return false;
    }
    let modulus = match value.len() {
        13 => 11,
        15 => 13,
        _ => return false,
    };
    let Ok(body) = value[..value.len() - 1].parse::<u64>() else {
        return false;
    };
    body % modulus % 10 == u64::from(value.as_bytes()[value.len() - 1] - b'0')
}
fn snils_valid(value: &str) -> Option<bool> {
    // Upstream accepts all-zero SNILS checksums; mdoc has always declined them.
    Some(
        value.bytes().any(|b| b.is_ascii_digit() && b != b'0')
            && country::validate_ru_snils(value) == Some(true),
    )
}
pub(super) fn scan(text: &str) -> Vec<Detection> {
    static RULES: OnceLock<Rules> = OnceLock::new();
    let rules = RULES.get_or_init(Rules::new);
    let mut out = Vec::new();
    for rule in &rules.0 {
        // Read only vetted patterns and validators, rather than registering the
        // full analyzer. PatternRecognizer::analyze in 0.1.11 builds explanations
        // per occurrence and uses quadratic remove_contained; mdoc already owns
        // evidence ranking/overlap resolution. Regex spans are UTF-8 byte ranges
        // in this exact immutable source, with no normalization or re-offsetting.
        for pattern in &rule.recognizer.patterns {
            for m in pattern.regex.find_iter(text) {
                if rule
                    .recognizer
                    .validator
                    .is_some_and(|validate| validate(m.as_str()) != Some(true))
                {
                    continue;
                }
                let accepted = if let Some(label) = &rule.label {
                    boundary(text, m.start(), m.end()) && context(text, m.start(), label)
                } else {
                    !text[..m.start()]
                        .chars()
                        .next_back()
                        .is_some_and(|c| c == '@')
                        && !text[m.end()..]
                            .chars()
                            .next()
                            .is_some_and(|c| c == '@' || c == '-')
                };
                if accepted {
                    out.push(Detection {
                        range: m.start()..m.end(),
                        category: rule.category,
                        score: rule.score,
                        recognizer: rule.evidence,
                    });
                }
            }
        }
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registration_checksums_require_labels_and_preserve_unicode_byte_spans() {
        let ogrn = "1027700132195";
        let ogrnip = "322213000038573";
        assert!(registration_valid(ogrn));
        assert!(registration_valid(ogrnip));
        for value in [
            "0000000000000",
            "000000000000000",
            "1027700132196",
            "322213000038574",
            "10277001321950",
        ] {
            assert!(!registration_valid(value));
        }
        let source = format!(
            "😀 ОГРН: {ogrn}; ОГРНИП: {ogrnip}; unlabeled {ogrn}; ОГРН:\n{ogrn}; ОГРН: 1027700132196"
        );
        let hits: Vec<_> = scan(&source)
            .into_iter()
            .filter(|d| d.recognizer == Recognizer::Ogrn)
            .collect();
        assert_eq!(hits.len(), 2);
        assert_eq!(&source[hits[0].range.clone()], ogrn);
        assert_eq!(&source[hits[1].range.clone()], ogrnip);
        assert!(hits.iter().all(|d| d.category == Category::Identity));
    }
    #[test]
    fn only_selected_entities_with_strict_context_and_boundaries() {
        let source = "СНИЛС: 11223344595; snils: 112-233-445\t95; TAX ID: 500100732259; ИНН:\n7707083893; СНИЛС: 000-000-000 00; СНИЛС: 111223344595; email @user@example.invalid and user@example.invalid-; case 12.10.2026; amount 10000; card 4111111111111111";
        let hits = scan(source);
        assert_eq!(
            hits.iter()
                .map(|h| (&source[h.range.clone()], h.category, h.recognizer, h.score))
                .collect::<Vec<_>>(),
            [
                ("500100732259", Category::Tax, Recognizer::Inn, 0.95),
                ("11223344595", Category::Identity, Recognizer::Snils, 0.95),
                (
                    "112-233-445\t95",
                    Category::Identity,
                    Recognizer::Snils,
                    0.95
                ),
            ]
        );
    }

    #[test]
    fn dense_maximum_source_preserves_all_byte_spans() {
        let row = "😀 юрист@example.invalid; ИНН: 7707083893; СНИЛС: 112-233-445 95\n";
        let mut source = row.repeat(6_667);
        source.extend(std::iter::repeat_n(
            ' ',
            super::super::MAX_SOURCE_BYTES - source.len(),
        ));
        let started = std::time::Instant::now();
        let hits = scan(&source);
        eprintln!(
            "Presidio adapter: {} bytes, {} occurrences, {:?}",
            source.len(),
            hits.len(),
            started.elapsed()
        );
        assert_eq!(hits.len(), 20_001);
        for hit in hits {
            assert_eq!(
                &source[hit.range],
                match hit.recognizer {
                    Recognizer::Email => "юрист@example.invalid",
                    Recognizer::Inn => "7707083893",
                    Recognizer::Snils => "112-233-445 95",
                    Recognizer::Ogrn => panic!("unexpected registration identifier"),
                    Recognizer::Model => panic!("structured adapter emitted model evidence"),
                }
            );
        }
    }

    #[test]
    fn exact_hidden_unicode_and_contextual_checksum_spans() {
        let source = "😀 [mail](mailto:юрист@example.invalid) <span title='ИНН: 7707083893'>Ё</span> `СНИЛС: 112-233-445 95`";
        let mut hits = scan(source);
        hits.sort_by_key(|h| h.range.start);
        assert_eq!(
            hits.iter()
                .map(|h| &source[h.range.clone()])
                .collect::<Vec<_>>(),
            ["юрист@example.invalid", "7707083893", "112-233-445 95"]
        );
        assert_eq!(
            hits.iter().map(|h| h.recognizer).collect::<Vec<_>>(),
            [Recognizer::Email, Recognizer::Inn, Recognizer::Snils]
        );
    }
    #[test]
    fn legal_numbers_and_invalid_checksums_decline_without_model_veto() {
        assert!(scan("Договор 7707083893; ИНН: 7707083894; СНИЛС: 112-233-445 96; ИНН: 0000000000; case 112-233-445 95").is_empty());
        assert!(inn_valid("500100732259"));
        assert!(!inn_valid("500100732258"));
        let source = "ИНН: 7707083894";
        let mut review = crate::pseudonymization::Review::default();
        review
            .ingest(
                source,
                vec![Detection {
                    range: source.find("770").unwrap()..source.len(),
                    category: Category::Tax,
                    score: 0.8,
                    recognizer: Recognizer::Model,
                }],
            )
            .unwrap();
        assert_eq!(review.remaining(), 1);
    }
}

#[cfg(test)]
mod captured_corpus {
    use super::*;
    #[test]
    fn selected_recognizers_match_the_frozen_research_predictions() {
        let corpus: Vec<serde_json::Value> = serde_json::from_str(include_str!(
            "../../tests/fixtures/pseudonymization/hybrid/corpus.json"
        ))
        .unwrap();
        let comparison: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/pseudonymization/results/2026-10-07-hybrid/comparison.json"
        ))
        .unwrap();
        for fixture in corpus {
            let annotated = fixture["annotated"].as_str().unwrap();
            let mut source = String::new();
            let mut tail = annotated;
            while let Some(open) = tail.find('⟦') {
                source.push_str(&tail[..open]);
                let mark = &tail[open + '⟦'.len_utf8()..];
                let close = mark.find('⟧').unwrap();
                source.push_str(mark[..close].split_once('|').unwrap().1);
                tail = &mark[close + '⟧'.len_utf8()..];
            }
            source.push_str(tail);
            let saved = comparison["fixtures"]
                .as_array()
                .unwrap()
                .iter()
                .find(|f| f["id"] == fixture["id"])
                .unwrap();
            let mut expected: Vec<_> = saved["pipelines"]["rules"]["predictions"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|p| {
                    matches!(
                        p["category"].as_str().unwrap(),
                        "EMAIL" | "TAX" | "IDENTITY"
                    )
                })
                .map(|p| {
                    (
                        p["start"].as_u64().unwrap() as usize,
                        p["end"].as_u64().unwrap() as usize,
                        p["category"].as_str().unwrap().to_owned(),
                    )
                })
                .collect();
            let mut actual: Vec<_> = scan(&source)
                .iter()
                .filter(|d| d.recognizer != Recognizer::Ogrn)
                .map(|d| (d.range.start, d.range.end, d.category.token().to_owned()))
                .collect();
            expected.sort();
            actual.sort();
            assert_eq!(actual, expected, "fixture {}", fixture["id"]);
        }
    }
}
