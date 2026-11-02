//! Web addresses by their shape (ADR 0094): a scheme, `www.` or a bare
//! domain in one of the rule pack's zones, minus its public and legal
//! domains. Read on the source itself, not the view: an address is written
//! as it is, and the view reads a link target's "_" as a space.
use crate::rules;
use mdoc_pii::{Category, Detection, Recognizer, web};
use regex::Regex;
use std::{ops::Range, sync::OnceLock};

/// A web address is rule evidence that wins over any other inside it.
const URL_SCORE: f32 = 1.0;

fn candidates() -> &'static Regex {
    static CANDIDATES: OnceLock<Regex> = OnceLock::new();
    CANDIDATES.get_or_init(|| {
        Regex::new(
            r#"(?i)(?:(?:https?|ftp)://|www\.)[^\s<>"'«»“”‘’|`]+|[\p{L}\p{N}][\p{L}\p{N}_-]*(?:\.[\p{L}\p{N}][\p{L}\p{N}_-]*)+(?::\d{1,5})?(?:[/?#][^\s<>"'«»“”‘’|`]*)?"#,
        )
        .expect("web address pattern")
    })
}

/// Identifying web addresses in `source`: every address but public and
/// legal sites'.
pub(crate) fn scan(source: &str) -> Vec<Detection> {
    find(source)
        .into_iter()
        .filter(|(_, host)| !rules::pack().public_host(host))
        .map(|(range, _)| Detection {
            range,
            category: Category::Url,
            score: URL_SCORE,
            recognizer: Recognizer::Url,
        })
        .collect()
}

/// Whether `text` is one web address and nothing else, public or not: a
/// manual selection's guess.
pub(crate) fn whole(text: &str) -> bool {
    let trimmed = text.trim();
    let lead = text.len() - text.trim_start().len();
    find(text)
        .first()
        .is_some_and(|(range, _)| *range == (lead..lead + trimmed.len()))
}

/// Every web address in `source` with its host, in order. Candidates come
/// from the pattern; where each starts and ends is [`web`]'s, which
/// discovery shares.
fn find(source: &str) -> Vec<(Range<usize>, String)> {
    let mut found = Vec::new();
    let mut at = 0;
    while let Some(m) = candidates().find_at(source, at) {
        let start = m.start();
        let first = m.as_str().chars().next().map_or(1, char::len_utf8);
        // An address may end before its match does (at a Markdown link's
        // "]"), so the next search starts where the address ends.
        let end = web::end(source, start);
        let glued = web::glued_before(source, start);
        at = if glued {
            m.end()
        } else {
            end.max(start + first)
        };
        let address = &source[start..end];
        let scheme = web::scheme_len(address);
        let explicit = web::explicit(address);
        let rest = &address[scheme..];
        let authority = &rest[..rest.find(['/', '?', '#']).unwrap_or(rest.len())];
        // A user's name and password before the host belong to the address.
        let host_port = if explicit {
            authority
                .rsplit_once('@')
                .map_or(authority, |(_, host)| host)
        } else {
            authority
        };
        let host = host_port
            .rsplit_once(':')
            .filter(|(_, port)| port.chars().all(|c| c.is_ascii_digit()))
            .map_or(host_port, |(host, _)| host);
        let www = scheme == 0 && explicit;
        if glued
            || host.is_empty()
            || (www && !host.contains('.'))
            || (!explicit && !bare_domain(source, start, end, host, authority))
        {
            continue;
        }
        found.push((start..end, host.to_owned()));
    }
    found
}

/// Whether a bare domain is written as an address: a known zone, a letter
/// in its name, its whole host and no email around it ("alice.morgan" of
/// "alice.morgan@…" is not one).
fn bare_domain(source: &str, start: usize, end: usize, host: &str, authority: &str) -> bool {
    // Only a port may follow the host ("site.invalid:8080").
    let port_ok = authority.strip_prefix(host).is_some_and(|port| {
        port.is_empty()
            || port.strip_prefix(':').is_some_and(|digits| {
                !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())
            })
    });
    if !port_ok || source[end..].starts_with('@') {
        return false;
    }
    let labels: Vec<&str> = host.split('.').collect();
    let (Some(zone), Some(name)) = (labels.last(), labels.iter().rev().nth(1)) else {
        return false;
    };
    // A one-letter name only with a path ("t.me/alva_r"), never "a.co".
    let path = end > start + host.len();
    rules::pack().web_zone(zone)
        && zone.chars().all(char::is_alphabetic)
        && (name.chars().count() >= 2 || path)
        && name.chars().any(char::is_alphabetic)
}

#[cfg(test)]
mod tests {
    use super::{scan, whole};

    fn urls(source: &str) -> Vec<&str> {
        scan(source).into_iter().map(|d| &source[d.range]).collect()
    }

    #[test]
    fn addresses_by_scheme_www_or_a_bare_domain_in_a_zone() {
        for (source, expected) in [
            (
                "Profile: https://example.invalid/clients/Alice_Morgan.",
                vec!["https://example.invalid/clients/Alice_Morgan"],
            ),
            (
                "See www.ivanov-law.ru, then call.",
                vec!["www.ivanov-law.ru"],
            ),
            ("сайт ivanov-law.ru.", vec!["ivanov-law.ru"]),
            ("сайт адвоката: юрист-петров.рф", vec!["юрист-петров.рф"]),
            (
                "(see https://drive.example.invalid/s/Qx7_k9?usp=sharing)",
                vec!["https://drive.example.invalid/s/Qx7_k9?usp=sharing"],
            ),
            (
                "[the claim](https://share.invalid/d/AbC123 \"Claim\")",
                vec!["https://share.invalid/d/AbC123"],
            ),
            (
                "[https://t.me/alva_r](https://t.me/alva_r)",
                vec!["https://t.me/alva_r", "https://t.me/alva_r"],
            ),
            (
                "https://wiki.invalid/Page_(disambiguation) and <ftp://files.invalid/a>",
                vec![
                    "https://wiki.invalid/Page_(disambiguation)",
                    "ftp://files.invalid/a",
                ],
            ),
            (
                "**https://vk.com/id48213077**",
                vec!["https://vk.com/id48213077"],
            ),
            (
                "https://site.invalid/u/alice@mail.invalid/",
                vec!["https://site.invalid/u/alice@mail.invalid/"],
            ),
            ("http://localhost:8080/x", vec!["http://localhost:8080/x"]),
            (
                "HTTPS://Example.INVALID/A",
                vec!["HTTPS://Example.INVALID/A"],
            ),
        ] {
            assert_eq!(urls(source), expected, "{source}");
        }
    }

    #[test]
    fn emails_links_files_numbers_and_public_sites_are_no_addresses() {
        for source in [
            "Email alice.morgan@example.invalid or mailto:alice@example.invalid",
            "tel:+79001234567",
            "readme.md, run.sh, contract.docx, report.pdf",
            "п. 3.2.1, ст. 12.15, v1.2.3, т.е., e.g., U.S.",
            "See https://kad.arbitr.ru/Card/0c1f and www.consultant.ru/document/cons_doc_LAW_5142/",
            "http://pravo.gov.ru/proxy/ips/?docbody=&nd=102041891",
            "https://www.legislation.gov.uk/ukpga/2018/12 and https://www.gov.uk/x",
            "mos-gorsud.ru, garant.ru, NALOG.GOV.RU, https://www.courtlistener.com/opinion/1/",
            "https://msk.sudrf.ru/modules.php?name=sud_delo",
            "files/site.ru and C:\\docs\\site.ru",
            "<pavel@\u{200b}vector.invalid>, ksen.rav@\nsvod.invalid, a&#64;b-c.invalid",
            "the email uses the reserved example.com domain",
            "ООО«Север».Москва",
        ] {
            assert_eq!(urls(source), Vec::<&str>::new(), "{source}");
        }
    }

    #[test]
    fn a_selection_of_one_address_guesses_it() {
        assert!(whole("https://kad.arbitr.ru/Card/1"));
        assert!(whole(" ivanov-law.ru "));
        assert!(!whole("see ivanov-law.ru"));
        assert!(!whole("alice@ivanov-law.ru"));
    }
}
