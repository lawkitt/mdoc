//! Web addresses in the review (ADR 0094): one span that wins over what is
//! inside it, link targets replaced in place, and one identity across
//! scheme, `www.` and trailing-slash writings.
use super::tests::plan_all;
use super::*;
use crate::restore::{ExportedIdentity, RestoreMap};

fn url(source: &str, text: &str) -> Detection {
    let start = source.find(text).unwrap();
    Detection {
        range: start..start + text.len(),
        category: Category::Url,
        score: 1.0,
        recognizer: Recognizer::Url,
    }
}
fn found(source: &str, text: &str, category: Category, recognizer: Recognizer) -> Detection {
    let start = source.rfind(text).unwrap();
    Detection {
        range: start..start + text.len(),
        category,
        score: if recognizer == Recognizer::Model {
            0.9
        } else {
            0.95
        },
        recognizer,
    }
}
fn replaced(review: &Review, source: &str) -> String {
    let mut out = source.to_owned();
    for (range, alias) in plan_all(review, source).unwrap().into_iter().rev() {
        out.replace_range(range, &alias);
    }
    out
}
fn alias_of(review: &Review, original: &str) -> String {
    let variant = review
        .variants()
        .iter()
        .find(|v| v.original.as_ref() == original)
        .unwrap_or_else(|| panic!("no variant {original:?}"));
    review
        .identity(variant.identity)
        .map(|i| i.alias.clone())
        .unwrap()
}

#[test]
fn a_web_address_wins_over_a_name_email_or_id_inside_it() {
    let source = "Alice Morgan: [profile](https://example.invalid/clients/Alice_Morgan), \
                  https://example.invalid/?email=alice@mail.invalid and \
                  https://files.invalid/id/4687731604.";
    let first = "https://example.invalid/clients/Alice_Morgan";
    let second = "https://example.invalid/?email=alice@mail.invalid";
    let third = "https://files.invalid/id/4687731604";
    let mut review = Review::default();
    review
        .ingest(
            source,
            vec![
                found(source, "Alice Morgan", Category::Person, Recognizer::Model),
                found(source, "Alice_Morgan", Category::Person, Recognizer::Model),
                found(
                    source,
                    "alice@mail.invalid",
                    Category::Email,
                    Recognizer::Email,
                ),
                found(source, "4687731604", Category::Tax, Recognizer::Inn),
                url(source, first),
                url(source, second),
                url(source, third),
            ],
        )
        .unwrap();
    let mut originals: Vec<&str> = review.variants().iter().map(|v| &*v.original).collect();
    originals.sort_unstable();
    assert_eq!(originals, ["Alice Morgan", second, first, third]);
    assert_eq!(
        replaced(&review, source),
        "PERSON_1: [profile](URL_1), URL_2 and URL_3."
    );
}

#[test]
fn a_link_target_is_replaced_in_place_and_shares_its_texts_alias() {
    let source = "See [the claim](https://share.invalid/d/AbC \"Claim\"), \
                  [https://t.invalid/a](https://t.invalid/a) and \
                  [ivanov-law.invalid](https://www.ivanov-law.invalid/).";
    let mut review = Review::default();
    review
        .ingest(
            source,
            vec![
                url(source, "https://share.invalid/d/AbC"),
                url(source, "https://t.invalid/a"),
                url(source, "ivanov-law.invalid"),
                url(source, "https://www.ivanov-law.invalid/"),
            ],
        )
        .unwrap();
    assert_eq!(
        replaced(&review, source),
        "See [the claim](URL_1 \"Claim\"), [URL_2](URL_2) and [URL_3](URL_3)."
    );
}

#[test]
fn scheme_www_and_a_trailing_slash_are_one_address_anything_else_another() {
    let same = [
        "http://site.invalid/a",
        "https://www.site.invalid/a/",
        "site.invalid/a",
        "HTTPS://WWW.site.invalid/a",
    ];
    let others = [
        "https://site.invalid/b",
        "https://site.invalid/A",
        "ftp://site.invalid/a",
    ];
    let mut source = String::new();
    let mut detections = Vec::new();
    for text in same.iter().chain(&others) {
        if !source.is_empty() {
            source.push_str(" ; ");
        }
        detections.push(Detection {
            range: source.len()..source.len() + text.len(),
            category: Category::Url,
            score: 1.0,
            recognizer: Recognizer::Url,
        });
        source.push_str(text);
    }
    let mut review = Review::default();
    review.ingest(&source, detections).unwrap();
    for text in same {
        assert_eq!(alias_of(&review, text), "URL_1", "{text}");
    }
    let aliases: HashSet<String> = others.iter().map(|t| alias_of(&review, t)).collect();
    assert_eq!(aliases.len(), others.len());
    assert!(!aliases.contains("URL_1"));
}

#[test]
fn a_kept_address_still_holds_the_name_inside_it() {
    let source = "Ms Morgan's page is https://example.invalid/staff/Morgan today.";
    let address = "https://example.invalid/staff/Morgan";
    let mut review = Review::default();
    review.ingest(source, vec![url(source, address)]).unwrap();
    let morgan = review.add_manual(source, 3..9, Category::Person).unwrap();
    let start = source.find(address).unwrap();
    assert_eq!(review.variant(morgan).unwrap().mentions, vec![3..9]);
    let page = review
        .variants()
        .iter()
        .find(|v| v.category == Category::Url)
        .unwrap()
        .id;
    review.keep(page, None);
    assert_eq!(review.variant(morgan).unwrap().mentions, vec![3..9]);
    assert!(
        review
            .candidates()
            .iter()
            .all(|c| c.range.start < start || c.range.start >= start + address.len())
    );
}

#[test]
fn a_web_address_alias_restores_to_its_wording() {
    let map = RestoreMap::new(vec![ExportedIdentity {
        alias: "URL_1".into(),
        previous: Vec::new(),
        category: Category::Url,
        custom: false,
        originals: vec![
            Arc::from("https://drive.invalid/s/Q7x"),
            Arc::from("drive.invalid/s/Q7x"),
        ],
    }]);
    let restored = map.restore("Open URL_1 or url-1, not URL_10.");
    let mut out = "Open URL_1 or url-1, not URL_10.".to_owned();
    for (range, value) in restored.edits.iter().rev() {
        out.replace_range(range.clone(), value);
    }
    assert_eq!(
        out,
        "Open https://drive.invalid/s/Q7x or https://drive.invalid/s/Q7x, not URL_10."
    );
    assert!(generated_alias("URL_1"));
}

#[test]
fn web_address_comes_right_after_email_and_older_tokens_are_unchanged() {
    let tokens: Vec<&str> = Category::ALL.iter().map(|c| c.token()).collect();
    assert_eq!(
        tokens,
        [
            "PERSON", "ORG", "EMAIL", "URL", "PHONE", "ADDRESS", "DATE", "CASE", "IDENTITY", "TAX",
            "BANK", "REDACTED"
        ]
    );
    assert_eq!(Category::DETECTED[3], Category::Url);
    assert_eq!(Category::Url.label(), "Web address");
}
