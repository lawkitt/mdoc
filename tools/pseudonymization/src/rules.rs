//! Research-only structured recognizers; no application dependency or activation.
use crate::Prediction;
use anyhow::Result;
use regex::Regex;
use std::collections::BTreeMap;

pub struct Rules {
    email: Regex,
    phone: Regex,
    numbers: Regex,
    snils: Regex,
    iban: Regex,
    phone_label: Regex,
    inn_label: Regex,
    snils_label: Regex,
    account_label: Regex,
    bic_label: Regex,
}

impl Rules {
    pub fn new() -> Result<Self> {
        let label = |body: &str| Regex::new(&format!(r#"(?i)(?:{body})[ \t:№=#_'"-]{{0,8}}$"#));
        Ok(Self {
            email: Regex::new(
                r"[\p{L}\p{N}][\p{L}\p{N}._%+-]*@[\p{L}\p{N}](?:[\p{L}\p{N}-]*[\p{L}\p{N}])?(?:\.[\p{L}\p{N}](?:[\p{L}\p{N}-]*[\p{L}\p{N}])?)+",
            )?,
            phone: Regex::new(r"\+?[0-9](?:[ \t().-]*[0-9]){8,14}")?,
            numbers: Regex::new(r"[0-9]{9,20}")?,
            snils: Regex::new(r"[0-9]{3}-[0-9]{3}-[0-9]{3}[ \t]?[0-9]{2}")?,
            iban: Regex::new(r"[A-Z]{2}[0-9]{2}")?,
            phone_label: label(r"\b(?:phone|telephone|tel|телефон|тел)\b\.?")?,
            inn_label: label(r"\b(?:инн|inn|tax id)\b")?,
            snils_label: label(r"\b(?:снилс|snils)\b")?,
            account_label: label(r"\b(?:расч[её]тный сч[её]т|р/с|сч[её]т|account)\b")?,
            bic_label: label(r"\b(?:бик|bic)\b")?,
        })
    }

    pub fn scan(&self, text: &str) -> Vec<Prediction> {
        let mut out = Vec::new();
        for m in self.email.find_iter(text) {
            if !text[..m.start()]
                .chars()
                .next_back()
                .is_some_and(|c| c == '@')
                && !text[m.end()..]
                    .chars()
                    .next()
                    .is_some_and(|c| c == '@' || c == '-')
            {
                push(
                    &mut out,
                    text,
                    m.start(),
                    m.end(),
                    "EMAIL",
                    "rule:email-format",
                    0.9,
                );
            }
        }
        for m in self.phone.find_iter(text) {
            let count = m.as_str().bytes().filter(u8::is_ascii_digit).count();
            if (10..=15).contains(&count)
                && digit_boundary(text, m.start(), m.end())
                && (m.as_str().starts_with('+') || context(text, m.start(), &self.phone_label))
            {
                push(
                    &mut out,
                    text,
                    m.start(),
                    m.end(),
                    "PHONE",
                    "rule:phone-format-context",
                    0.8,
                );
            }
        }
        for m in self.numbers.find_iter(text) {
            if !digit_boundary(text, m.start(), m.end()) {
                continue;
            }
            let (category, id, score) =
                if context(text, m.start(), &self.inn_label) && inn_valid(m.as_str()) {
                    ("TAX", "rule:inn-checksum-context", 0.95)
                } else if context(text, m.start(), &self.snils_label) && snils_valid(m.as_str()) {
                    ("IDENTITY", "rule:snils-checksum-context", 0.95)
                } else if m.len() == 20 && context(text, m.start(), &self.account_label) {
                    ("BANK", "rule:ru-account-format-context", 0.8)
                } else if m.len() == 9
                    && m.as_str().starts_with("04")
                    && context(text, m.start(), &self.bic_label)
                {
                    ("BANK", "rule:ru-bic-format-context", 0.8)
                } else {
                    continue;
                };
            push(&mut out, text, m.start(), m.end(), category, id, score);
        }
        for m in self.snils.find_iter(text) {
            if digit_boundary(text, m.start(), m.end())
                && context(text, m.start(), &self.snils_label)
                && snils_valid(m.as_str())
            {
                push(
                    &mut out,
                    text,
                    m.start(),
                    m.end(),
                    "IDENTITY",
                    "rule:snils-checksum-context",
                    0.95,
                );
            }
        }
        for m in self.iban.find_iter(text) {
            let expected = match &m.as_str()[..2] {
                "GB" | "DE" => 22,
                "FR" | "IT" => 27,
                "NL" => 18,
                "ES" => 24,
                "PL" => 28,
                _ => continue,
            };
            let mut normalized = String::with_capacity(expected);
            let mut end = m.start();
            for (offset, c) in text[m.start()..].char_indices() {
                if c.is_ascii_uppercase() || c.is_ascii_digit() {
                    normalized.push(c);
                } else if c != ' ' {
                    break;
                }
                end = m.start() + offset + c.len_utf8();
                if normalized.len() == expected {
                    break;
                }
            }
            if normalized.len() == expected
                && iban_valid(&normalized)
                && !text[..m.start()]
                    .chars()
                    .next_back()
                    .is_some_and(char::is_alphanumeric)
                && !text[end..]
                    .chars()
                    .next()
                    .is_some_and(char::is_alphanumeric)
            {
                push(
                    &mut out,
                    text,
                    m.start(),
                    end,
                    "BANK",
                    "rule:iban-mod97-country-length",
                    0.95,
                );
            }
        }
        resolve(text, &out, &[])
    }
}

fn context(text: &str, start: usize, label: &Regex) -> bool {
    // A bounded, UTF-8-safe lookbehind; never scans a whole prefix per hit.
    let suffix: String = text[..start]
        .chars()
        .rev()
        .take_while(|&c| c != '\n')
        .take(80)
        .collect();
    label.is_match(&suffix.chars().rev().collect::<String>())
}

fn digit_boundary(text: &str, start: usize, end: usize) -> bool {
    !text[..start]
        .chars()
        .next_back()
        .is_some_and(|c| c.is_ascii_digit())
        && !text[end..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit())
}

fn push(
    out: &mut Vec<Prediction>,
    text: &str,
    start: usize,
    end: usize,
    category: &str,
    label: &str,
    score: f32,
) {
    out.push(Prediction {
        category: category.into(),
        label: label.into(),
        start,
        end,
        text: text[start..end].into(),
        score,
    });
}

pub fn inn_valid(value: &str) -> bool {
    if !value.bytes().all(|b| b.is_ascii_digit()) || value.bytes().all(|b| b == b'0') {
        return false;
    }
    let digits: Vec<u32> = value.bytes().map(|b| u32::from(b - b'0')).collect();
    let check =
        |weights: &[u32]| digits.iter().zip(weights).map(|(d, w)| d * w).sum::<u32>() % 11 % 10;
    match digits.len() {
        10 => check(&[2, 4, 10, 3, 5, 9, 4, 6, 8]) == digits[9],
        12 => {
            check(&[7, 2, 4, 10, 3, 5, 9, 4, 6, 8]) == digits[10]
                && check(&[3, 7, 2, 4, 10, 3, 5, 9, 4, 6, 8]) == digits[11]
        }
        _ => false,
    }
}

pub fn snils_valid(value: &str) -> bool {
    if value
        .chars()
        .any(|c| !c.is_ascii_digit() && c != '-' && c != ' ' && c != '\t')
    {
        return false;
    }
    let d: Vec<u32> = value
        .bytes()
        .filter(u8::is_ascii_digit)
        .map(|b| u32::from(b - b'0'))
        .collect();
    if d.len() != 11 || d.iter().all(|&n| n == 0) {
        return false;
    }
    let sum = d[..9]
        .iter()
        .enumerate()
        .map(|(i, &n)| n * (9 - i as u32))
        .sum::<u32>();
    let check = match sum % 101 {
        100 => 0,
        n => n,
    };
    check == d[9] * 10 + d[10]
}

pub fn iban_valid(value: &str) -> bool {
    if value.len() < 4 || !value.is_ascii() {
        return false;
    }
    let mut remainder = 0_u32;
    for b in value[4..].bytes().chain(value[..4].bytes()) {
        match b {
            b'0'..=b'9' => remainder = (remainder * 10 + u32::from(b - b'0')) % 97,
            b'A'..=b'Z' => remainder = (remainder * 100 + u32::from(b - b'A') + 10) % 97,
            _ => return false,
        }
    }
    remainder == 1
}

/// Research policy: rules take explicit precedence; model score ranks model
/// spans only. Never pretend heuristic rule scores are calibrated model scores.
pub fn resolve(text: &str, rules: &[Prediction], model: &[Prediction]) -> Vec<Prediction> {
    let mut pending: Vec<_> = rules
        .iter()
        .map(|p| (true, p))
        .chain(model.iter().map(|p| (false, p)))
        .collect();
    pending.retain(|(_, p)| p.valid_for(text));
    pending.sort_by(|(ar, a), (br, b)| {
        br.cmp(ar)
            .then_with(|| b.score.total_cmp(&a.score))
            .then_with(|| (b.end - b.start).cmp(&(a.end - a.start)))
            .then_with(|| a.start.cmp(&b.start))
            .then_with(|| a.label.cmp(&b.label))
    });
    let mut kept: BTreeMap<usize, Prediction> = BTreeMap::new();
    for (_, p) in pending {
        if kept
            .range(..=p.start)
            .next_back()
            .is_some_and(|(_, prev)| prev.end > p.start)
            || kept
                .range(p.start..)
                .next()
                .is_some_and(|(&next, _)| next < p.end)
        {
            continue;
        }
        kept.insert(p.start, p.clone());
    }
    kept.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validators_reject_mutated_checksums_and_zero_values() {
        assert!(inn_valid("7707083893"));
        assert!(inn_valid("500100732259"));
        assert!(!inn_valid("7707083894"));
        assert!(!inn_valid("500100732258"));
        assert!(!inn_valid("0000000000"));
        assert!(snils_valid("112-233-445 95"));
        assert!(!snils_valid("112-233-445 96"));
        assert!(!snils_valid("00000000000"));
        assert!(iban_valid("GB29NWBK60161331926819"));
        assert!(!iban_valid("GB28NWBK60161331926819"));
        assert!(!iban_valid("ЯА00"));
    }
    #[test]
    fn hidden_source_and_unicode_offsets_remain_exact() {
        let text = "😀 Ё [x](mailto:elkinа@example.invalid) <span title='ИНН: 7707083893'>x</span> СНИЛС: 112-233-445 95";
        let predictions = Rules::new().unwrap().scan(text);
        assert_eq!(predictions.len(), 3);
        assert!(predictions.iter().all(|p| p.valid_for(text)));
        assert_eq!(predictions[0].text, "elkinа@example.invalid");
    }
    #[test]
    fn legal_numbers_and_invalid_labeled_checksums_do_not_match() {
        let text = "Дело А40-123456/2026, договор 7707083893, 500100732259 руб.; ИНН: 7707083894; СНИЛС: 112-233-445 96. Article 2025550147. Account 12345.67.";
        assert!(Rules::new().unwrap().scan(text).is_empty());
    }
    #[test]
    fn hybrid_precedence_does_not_suppress_disjoint_model_mentions() {
        let text = "Анна 7707083893 Борис";
        let mut rules = Vec::new();
        let mut model = Vec::new();
        let start = text.find("770").unwrap();
        push(&mut rules, text, start, start + 10, "TAX", "rule", 0.8);
        push(&mut model, text, start, start + 10, "PHONE", "model", 0.99);
        push(&mut model, text, 0, "Анна".len(), "PERSON", "model", 0.5);
        let b = text.find("Борис").unwrap();
        push(&mut model, text, b, text.len(), "PERSON", "model", 0.5);
        let output = resolve(text, &rules, &model);
        assert_eq!(
            output
                .iter()
                .map(|p| p.category.as_str())
                .collect::<Vec<_>>(),
            ["PERSON", "TAX", "PERSON"]
        );
    }

    #[test]
    fn malformed_offsets_are_rejected_before_sorting() {
        let text = "Я";
        let bad = Prediction {
            category: "PERSON".into(),
            label: "bad".into(),
            start: 2,
            end: 1,
            text: "Я".into(),
            score: 0.5,
        };
        let split = Prediction {
            start: 0,
            end: 1,
            ..bad.clone()
        };
        assert!(resolve(text, &[bad], &[split]).is_empty());
    }
}
