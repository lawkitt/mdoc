//! Reading and checking the pack.
use super::*;
use crate::rules::load::rust_classes;
use mdoc_pii::Recognizer;

#[test]
fn every_rule_file_loads_and_each_entry_is_commented() {
    for file in FILES {
        Pack::load(&[file]).unwrap_or_else(|problem| panic!("{problem}"));
        // Each entry and section opens under a comment saying what it is for.
        let lines: Vec<&str> = file.1.lines().collect();
        for (index, line) in lines.iter().enumerate() {
            let opens = line.starts_with("[[")
                || ["never", "context", "shape", "span", "variants", "reference"]
                    .iter()
                    .any(|section| line.starts_with(&format!("{section} =")));
            if opens {
                assert!(
                    index > 0 && lines[index - 1].starts_with('#'),
                    "{}:{}: {line} needs a comment above it",
                    file.0,
                    index + 1
                );
            }
        }
    }
    let pack = pack();
    // Manual selections try dates before phones (2026-03-12 has eight
    // digits) and legal forms before name shapes ("Acme Ltd").
    let order: Vec<_> = pack.shapes.iter().map(|s| s.category).collect();
    let position = |category| order.iter().position(|&c| c == category).unwrap();
    assert!(position(Category::Date) < position(Category::Phone));
    assert!(position(Category::Organization) < position(Category::Person));
}

/// The pack's Unicode classes follow the compiler's tables, which can be
/// newer than `regex`'s: U+A7CE is uppercase and U+11DE0 a digit in
/// Unicode 17.
#[test]
fn letters_and_digits_mean_what_rust_says() {
    assert!('\u{A7CE}'.is_uppercase() && '\u{11DE0}'.is_numeric());
    let pack = pack();
    assert!(pack.identifying("\u{A7CE}a", Category::Person));
    assert_eq!(pack.guess("\u{A7CE}a"), Some(Category::Person));
    assert!(pack.identifying(&"\u{11DE0}".repeat(5), Category::Phone));
    for (name, predicate) in [
        (r"\p{Uppercase}", char::is_uppercase as fn(char) -> bool),
        (r"\p{Alphabetic}", char::is_alphabetic),
        (r"\p{N}", char::is_numeric),
    ] {
        let class = Regex::new(&format!("^{}$", rust_classes(name))).unwrap();
        for c in (0..=char::MAX as u32).filter_map(char::from_u32) {
            assert_eq!(
                class.is_match(c.encode_utf8(&mut [0; 4])),
                predicate(c),
                "{name} {c:?}"
            );
        }
    }
    // Shapes split words and trim with `\s`, as Rust does with
    // `split_whitespace` and `trim`.
    let space = Regex::new(r"^\s$").unwrap();
    for c in (0..=char::MAX as u32).filter_map(char::from_u32) {
        assert_eq!(
            space.is_match(c.encode_utf8(&mut [0; 4])),
            c.is_whitespace(),
            "{c:?}"
        );
    }
}

/// One pack holds both kinds of `span` entry: a pattern proposes whole
/// spans, an entry without one fixes model spans' boundaries, and
/// neither does the other's work.
#[test]
fn span_patterns_and_span_fixes_load_from_one_pack() {
    let pack = Pack::load(&[(
        "test.toml",
        "[[span]]\ncategory = \"ADDRESS\"\npattern = '{street}, {town}'\n\
         parts.street = '\\d+ \\p{Uppercase}\\p{Alphabetic}+ Road'\n\
         parts.town = '\\p{Uppercase}\\p{Alphabetic}+'\n\
         [[span]]\ncategory = \"ORG\"\nstrip_end = \".\"\n\
         [[span]]\ncategory = \"PERSON\"\ntrim = [\"ms\"]\n",
    )])
    .unwrap();
    let source = "Ms Bennett of Acme Ltd. lives at 41 Larch Road, Bristol.";
    let proposed: Vec<_> = pack
        .spans(source)
        .into_iter()
        .map(|d| (source[d.range].to_owned(), d.category, d.recognizer))
        .collect();
    assert_eq!(
        proposed,
        [(
            "41 Larch Road, Bristol".to_owned(),
            Category::Address,
            Recognizer::Span
        )]
    );
    let mut detections: Vec<Detection> = [
        ("Ms Bennett", Category::Person),
        ("Acme Ltd.", Category::Organization),
        ("Larch Road, Bristol.", Category::Address),
    ]
    .into_iter()
    .map(|(text, category)| {
        let start = source.find(text).unwrap();
        Detection {
            range: start..start + text.len(),
            category,
            score: 0.9,
            recognizer: Recognizer::Model,
        }
    })
    .collect();
    pack.fix_spans(source, &mdoc_pii::view::View::new(source), &mut detections);
    let fixed: Vec<_> = detections
        .iter()
        .map(|d| &source[d.range.clone()])
        .collect();
    assert_eq!(fixed, ["Bennett", "Acme Ltd", "Larch Road, Bristol."]);
}

#[test]
fn mistakes_in_a_rule_file_are_named() {
    for (source, problem) in [
        (
            "[[context]]\ncategory = \"DATE\"\nlabels = [\"birth date\"]\nafter = true\nline_start = true\nbetween = ' '\nvalue = '\\d{4}'",
            "no line_start or unless_after",
        ),
        (
            "[[context]]\ncategory = \"DATE\"\nlabels = [\"dob\"]\nfollow = '\\.'\nbetween = ' '\nvalue = '\\d{4}'",
            "only a label after its value takes only_after or follow",
        ),
        (
            "[[context]]\ncategory = \"DATE\"\nlabels = [\"Birth date\"]\nafter = true\nbetween = ' '\nvalue = '\\d{4}'",
            "after its value must be lowercase",
        ),
        (
            "[[context]]\ncategory = \"DATE\"\nlabels = [\"birth date\"]\nafter = true\nonly_after = [\"The\"]\nbetween = ' '\nvalue = '\\d{4}'",
            "is not one lowercase word or 's",
        ),
        (
            "[[context]]\ncategory = \"DATE\"\nlabels = [\"birth date\"]\nafter = true\nunless_slashed = true\nbetween = ' '\nvalue = '\\d{4}'",
            "only a label before its value takes unless_slashed",
        ),
        (
            "[[never]]\nmatch = \"plural\"\nwords = [\"Ms\"]",
            "not lowercase",
        ),
        ("[[never]]\nmatch = \"stem\"\nwords = [\"истц\"]", "ending"),
        (
            "[[shape]]\ncategory = \"PERSON\"\nforbid_quotes = [\"«\"]",
            "not an opening and a closing quote",
        ),
        (
            "[[shape]]\ncategory = \"PERSON\"\nquotes_after = [\"повест\"]",
            "needs forbid_quotes",
        ),
        (
            "[[shape]]\ncategory = \"PERSON\"\nforbid_quotes = [\"«»\"]\nquotes_after = [\"Повест\"]",
            "lowercase stem",
        ),
        (
            "[[never]]\nmatch = \"whole\"\nwords = [\"the\", \"the\"]",
            "twice",
        ),
        (
            "[[never]]\nmatch = \"whole\"\nwords = [\"vice\"]\nphrase_only = true",
            "phrase_only needs unless_named alone",
        ),
        (
            "[[never]]\nmatch = \"whole\"\nwords = [\"vice\"]\nunless_named = true\nphrase_only = true\nperson_role = true",
            "phrase_only needs unless_named alone",
        ),
        (
            "[[never]]\nmatch = \"whole\"\nwords = [\"vice\"]\nunless_named = true\nphrase_only = true\nlast_word_of = [\"ORG\"]",
            "phrase_only needs unless_named alone",
        ),
        (
            "[[shape]]\ncategory = \"NAME\"\nrequire = ['x']",
            "unknown category",
        ),
        ("[[shape]]\ncategory = \"DATE\"\nguess = ['(']", "shape #1"),
        ("[[shape]]\ncategory = \"DATE\"", "no rules"),
        (
            "[[shape]]\ncategory = \"PERSON\"\nword = 'a'\n[[shape]]\ncategory = \"PERSON\"\nword = 'b'",
            "second word rule",
        ),
        (
            "[[shape]]\ncategory = \"ORG\"\nguess_words = [\"S.A.\"]",
            "one word",
        ),
        (
            "[[context]]\ncategory = \"DATE\"\nlabels = []\nbetween = ' '\nvalue = '\\d+'",
            "labels",
        ),
        (
            "[[context]]\ncategory = \"CASE\"\nlabels = [\"дело №\"]\nbetween = ' '\nvalue = '\\d+'",
            "start and end",
        ),
        (
            "[[context]]\ncategory = \"CASE\"\nlabels = [\"case\"]\nbetween = ' '\nvalue = '(\\d+)'",
            "capture",
        ),
        ("[[span]]\ncategory = \"PERSON\"", "trim"),
        (
            "[[span]]\ncategory = \"PERSON\"\ntrim = [\"Ms\"]",
            "not lowercase",
        ),
        (
            "[[span]]\ncategory = \"PERSON\"\ntrim_start = ['x*']",
            "empty prefix",
        ),
        (
            "[[span]]\ncategory = \"PERSON\"\ntrim_start = ['(']",
            "span #1",
        ),
        (
            "[[span]]\ncategory = \"ORG\"\nstrip_end = \"a.\"",
            "punctuation only",
        ),
        ("[[span]]\ncategory = \"ORG\"\nextend = ['(']", "span #1"),
        (
            "[[span]]\ncategory = \"ORG\"\nextend = ['(?P<lead>by )X Ltd']",
            "only a PERSON extend takes a `lead`",
        ),
        (
            "[[span]]\ncategory = \"ORG\"\nextend = ['X Ltd(?P<tail>,)']",
            "only a PERSON extend takes a `lead` or a `tail`",
        ),
        (
            "[[span]]\ncategory = \"PERSON\"\nextend = ['(?P<name>x)y']",
            "extend group \"name\" is not `lead` or `tail`",
        ),
        (
            "[[span]]\ncategory = \"PERSON\"\ntrim = [\"ms\"]\nunless_over = [\"part\"]",
            "unless_over needs extend",
        ),
        (
            "[[span]]\ncategory = \"PERSON\"\ntrim_start = ['(?P<name>Dr )x']",
            "is not `rest`",
        ),
        (
            "[[span]]\ncategory = \"BANK\"\nlist_letter = true",
            "only PERSON or ORG",
        ),
        (
            "[[span]]\ncategory = \"PERSON\"\nnested_quotes = ['ООО']",
            "only an ORG fix takes nested_quotes",
        ),
        (
            "[[span]]\ncategory = \"ORG\"\nnested_quotes = ['(?P<form>ООО)']",
            "nested_quotes takes no groups",
        ),
        (
            "[[span]]\ncategory = \"ORG\"\nnested_quotes = ['x?']",
            "nested_quotes matches an empty form",
        ),
        ("[[span]]\ncategory = \"ADDRESS\"", "pattern"),
        (
            "[[span]]\ncategory = \"PERSON\"\nsplit = '<'",
            "need a pattern",
        ),
        (
            "[[span]]\ncategory = \"PERSON\"\ntrim = [\"ms\"]\ngaps_only = true",
            "need a pattern",
        ),
        (
            "[[span]]\ncategory = \"PERSON\"\npattern = 'x'\nsplit = '<*'",
            "empty piece",
        ),
        (
            "[[span]]\ncategory = \"PERSON\"\npattern = 'x'\nline_length = [5, 4]",
            "line_length",
        ),
        (
            "[[span]]\ncategory = \"PERSON\"\npattern = 'x'\nline_length = [0, 4]",
            "least 1",
        ),
        (
            "[[span]]\ncategory = \"PERSON\"\npattern = 'x'\nskip = 'X*'",
            "every span",
        ),
        (
            "[[span]]\ncategory = \"PERSON\"\nskip = 'X'",
            "need a pattern",
        ),
        (
            "[[span]]\ncategory = \"PERSON\"\npattern = '(?P<mark>a)x'",
            "is not `lead` or `tail`",
        ),
        (
            "[[span]]\ncategory = \"PERSON\"\npattern = '(?P<marker>a)x'\nline_length = [1, 4]",
            "or a `mark`",
        ),
        (
            "[[span]]\ncategory = \"ADDRESS\"\npattern = '\\d+'\nparts = { street = 'x' }",
            "never used",
        ),
        (
            "[[span]]\ncategory = \"ADDRESS\"\npattern = ''",
            "empty span",
        ),
        (
            "[[span]]\ncategory = \"ADDRESS\"\npattern = 'x*'",
            "empty span",
        ),
        (
            "[[span]]\ncategory = \"ADDRESS\"\npattern = '(?P<city>\\w+), \\d+'",
            "is not `lead` or `tail`",
        ),
        (
            "[[span]]\ncategory = \"ADDRESS\"\npattern = '\\d+'\ntrim = [\"ms\"]",
            "takes no trim",
        ),
        (
            "[[span]]\ncategory = \"ORG\"\nstrip_end = \".\"\nparts = { street = 'x' }",
            "need a pattern",
        ),
        (
            "[[span]]\ncategory = \"ORG\"\nstrip_end = \".\"\nabsorbs = { ORG = '(?P<span>x)' }",
            "need a pattern",
        ),
        (
            "[[span]]\ncategory = \"ADDRESS\"\npattern = '\\d+'\nabsorbs = { ORG = 'x' }",
            "no `span` group",
        ),
        (
            "[[span]]\ncategory = \"ADDRESS\"\npattern = '\\d+'\nabsorbs = { NOPE = '(?P<span>x)' }",
            "span #1",
        ),
        (
            "[[span]]\ncategory = \"PERSON\"\nperson_before = [\"decl\"]",
            "only an ORG fix",
        ),
        (
            "[[span]]\ncategory = \"ORG\"\nperson_before = [\"Decl\"]",
            "not lowercase",
        ),
        (
            "[[span]]\ncategory = \"ORG\"\norg_before = [\"airlines\"]",
            "only a PERSON fix takes org_before",
        ),
        (
            "[[span]]\ncategory = \"PERSON\"\norg_after = [\"team  known as\"]",
            "not words single-spaced",
        ),
        (
            "[[span]]\ncategory = \"PERSON\"\norg_before = [\"Bank\"]",
            "not lowercase",
        ),
        (
            "[[reference]]\npattern = 'x*'",
            "reference #1: the pattern matches an empty span",
        ),
        (
            "[[reference]]\npattern = 'x'\ncategories = ['PERSON']",
            "categories needs when",
        ),
        (
            "[[reference]]\npattern = 'x'\nwhen = ['x']",
            "when needs categories",
        ),
        (
            "[[reference]]\npattern = 'x'\nwhen = ['x']\ncategories = ['PERSON']",
            "need a series group",
        ),
        (
            "[[reference]]\npattern = '(?P<series>C)'\nwhen = ['(?P<series>C)']\ncategories = ['NAME']",
            "unknown category",
        ),
        (
            "[[reference]]\npattern = '\\d+'\nparts = { code = 'x' }",
            "never used",
        ),
        (
            "[[reference]]\npattern = '(?P<code>x)'",
            "is not `lead`, `tail`, `docket` or a `party`",
        ),
        (
            "[[reference]]\npattern = 'x'\nroles = [\"plaintiff\"]",
            "only a cited case",
        ),
        (
            "[[reference]]\npattern = '(?P<party>x)'\nroles = [\"Plaintiff\"]",
            "not a lowercase word",
        ),
        (
            "[[reference]]\npattern = 'x'\nown_before = [\"this case\"]",
            "only a cited case",
        ),
        (
            "[[reference]]\npattern = '(?P<party>x)'\nown_before = [\"This case\"]",
            "not a lowercase word",
        ),
        ("[[variants]]\nname = \"Liz\"\nforms = [\"Liz\"]", "forms"),
        (
            "[[never]]\nmatch = \"whole\"\nwords = [\"a\"]\ntypo = 1",
            "typo",
        ),
        ("[[url]]", "needs zones or public domains"),
        ("[[url]]\nzones = [\"co.uk\"]", "is not a domain"),
        ("[[url]]\npublic = [\"Gov.ru\"]", "is not lowercase"),
        ("[[url]]\npublic = [\"gov\", \"gov\"]", "listed twice"),
        (
            "[[label]]\ncategory = \"NAME\"\nwords = [\"tel\"]",
            "unknown category",
        ),
        (
            "[[label]]\ncategory = \"PERSON\"\nwords = [\"mr\"]",
            "needs TAX, BANK, IDENTITY or PHONE",
        ),
        ("[[label]]\ncategory = \"PHONE\"\nwords = []", "needs words"),
        (
            "[[label]]\ncategory = \"PHONE\"\nwords = [\"Tel\"]",
            "needs lowercase words",
        ),
        (
            "[[label]]\ncategory = \"PHONE\"\nwords = [\"tel.\"]",
            "without dots",
        ),
        (
            "[[label]]\ncategory = \"PHONE\"\nwords = [\"\"]",
            "needs lowercase words",
        ),
        (
            "[[label]]\ncategory = \"IDENTITY\"\nwords = [\"серии\"]\nonly_after = [\"Паспорт\"]",
            "needs lowercase words",
        ),
        (
            "[[label]]\ncategory = \"PHONE\"\nwords = [\"fax\"]\n[[label]]\ncategory = \"BANK\"\nwords = [\"fax\"]",
            "label #2: \"fax\" is another category's label too",
        ),
        (
            "[[label]]\ncategory = \"PHONE\"\nwords = [\"tel\"]\nlabels = [\"x\"]",
            "unknown field",
        ),
        // #491: each role list counts its own entries.
        (
            "initials_after = ['истец']\ninitials_after_party = ['ответчик', '(?:)']",
            "test.toml: initials_after_party #2: matches no role",
        ),
        (
            "initials_after_citizen = ['(']",
            "initials_after_citizen #1",
        ),
        (
            "initials_companies = [\"Ооо\"]",
            "initials_companies needs words in capitals",
        ),
        (
            "initials_never = [\"\"]",
            "initials_never needs words in capitals",
        ),
    ] {
        let Err(error) = Pack::load(&[("test.toml", source)]) else {
            panic!("accepted {source:?}");
        };
        assert!(error.contains(problem), "{error}");
        assert!(error.starts_with("test.toml"), "{error}");
    }
}

/// ADR 0094: a public domain's subdomain is never listed beside it, in
/// one file or across files.
#[test]
fn a_public_domain_inside_another_is_rejected() {
    let Err(error) = Pack::load(&[
        ("a.toml", "[[url]]\npublic = [\"gov.ru\"]"),
        ("b.toml", "[[url]]\npublic = [\"pravo.gov.ru\"]"),
    ]) else {
        panic!("accepted a listed parent's subdomain");
    };
    assert!(
        error.contains("\"pravo.gov.ru\" is inside \"gov.ru\""),
        "{error}"
    );
}

/// #549: a `lookalike` entry's mistakes are named.
#[test]
fn mistakes_in_a_lookalike_entry_are_named() {
    for (entry, problem) in [
        ("[[lookalike]]\n", "needs letters, words or unless_after"),
        (
            "[[lookalike]]\nunless_after = [\"Свидетел\"]\n",
            "unless_after needs lowercase Cyrillic stems",
        ),
        (
            "[[lookalike]]\nletters = [\"H\"]\n",
            "is no pair of two letters",
        ),
        (
            "[[lookalike]]\nletters = [\"НH\"]\n",
            "needs another script's letter",
        ),
        (
            "[[lookalike]]\nletters = [\"HН\", \"HА\"]\n",
            "reads as two letters",
        ),
        (
            "[[lookalike]]\nwords = [\"На\"]\n",
            "needs lowercase Cyrillic letters",
        ),
        (
            "[[lookalike]]\nwords = [\"т.п.\"]\n",
            "needs lowercase Cyrillic letters",
        ),
    ] {
        let error = Pack::load(&[("test.toml", entry)])
            .err()
            .unwrap_or_default();
        assert!(error.contains(problem), "{entry}: {error}");
    }
}
