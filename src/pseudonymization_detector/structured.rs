//! Selected structured recognizers. Declining a value never rejects model output.
//! Heuristic rule evidence is ranked separately from model probabilities.
use crate::pseudonymization::{Category, Detection, Recognizer};
use regex::Regex;
use std::sync::OnceLock;

struct Rules {
    email: Regex,
    numbers: Regex,
    snils: Regex,
    inn_label: Regex,
    snils_label: Regex,
}
impl Rules {
    fn new() -> Self {
        let label = |body: &str| {
            Regex::new(&format!(r#"(?i)\b(?:{body})\b[ \t:№=#_'"-]{{0,8}}$"#))
                .expect("static context regex")
        };
        Self {
            email:Regex::new(r"[\p{L}\p{N}][\p{L}\p{N}._%+-]*@[\p{L}\p{N}](?:[\p{L}\p{N}-]*[\p{L}\p{N}])?(?:\.[\p{L}\p{N}](?:[\p{L}\p{N}-]*[\p{L}\p{N}])?)+").expect("static email regex"),
            numbers:Regex::new(r"[0-9]{9,20}").expect("static digit regex"),
            snils:Regex::new(r"[0-9]{3}-[0-9]{3}-[0-9]{3}[ \t]?[0-9]{2}").expect("static SNILS regex"),
            inn_label:label("инн|inn|tax id"), snils_label:label("снилс|snils"),
        }
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
fn snils_valid(value: &str) -> bool {
    let d: Vec<u32> = value
        .bytes()
        .filter(u8::is_ascii_digit)
        .map(|b| u32::from(b - b'0'))
        .collect();
    if d.len() != 11 || d.iter().all(|&n| n == 0) {
        return false;
    }
    let check = d[..9]
        .iter()
        .enumerate()
        .map(|(i, &n)| n * (9 - i as u32))
        .sum::<u32>()
        % 101;
    let check = if check == 100 { 0 } else { check };
    check == d[9] * 10 + d[10]
}
pub(super) fn scan(text: &str) -> Vec<Detection> {
    static RULES: OnceLock<Rules> = OnceLock::new();
    let rules = RULES.get_or_init(Rules::new);
    let mut out = Vec::new();
    for m in rules.email.find_iter(text) {
        if !text[..m.start()]
            .chars()
            .next_back()
            .is_some_and(|c| c == '@')
            && !text[m.end()..]
                .chars()
                .next()
                .is_some_and(|c| c == '@' || c == '-')
        {
            out.push(Detection {
                range: m.start()..m.end(),
                category: Category::Email,
                score: 0.9,
                recognizer: Recognizer::Email,
            });
        }
    }
    for m in rules.numbers.find_iter(text) {
        if !boundary(text, m.start(), m.end()) {
            continue;
        }
        let (category, recognizer) =
            if context(text, m.start(), &rules.inn_label) && inn_valid(m.as_str()) {
                (Category::Tax, Recognizer::Inn)
            } else if context(text, m.start(), &rules.snils_label) && snils_valid(m.as_str()) {
                (Category::Identity, Recognizer::Snils)
            } else {
                continue;
            };
        out.push(Detection {
            range: m.start()..m.end(),
            category,
            score: 0.95,
            recognizer,
        });
    }
    for m in rules.snils.find_iter(text) {
        if boundary(text, m.start(), m.end())
            && context(text, m.start(), &rules.snils_label)
            && snils_valid(m.as_str())
        {
            out.push(Detection {
                range: m.start()..m.end(),
                category: Category::Identity,
                score: 0.95,
                recognizer: Recognizer::Snils,
            });
        }
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
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
                .map(|d| (d.range.start, d.range.end, d.category.token().to_owned()))
                .collect();
            expected.sort();
            actual.sort();
            assert_eq!(actual, expected, "fixture {}", fixture["id"]);
        }
    }
}
