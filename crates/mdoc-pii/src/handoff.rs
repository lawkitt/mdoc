//! The text handed to an external AI (ADR 0041, ADR 0084): Copy for AI is
//! the Markdown with a note asking the AI to keep placeholders exactly. The
//! editor and the shelf build it here, so both copy the same bytes.

/// The note Copy for AI puts before the text. It names aliases only, never
/// original values; `examples` are the first aliases of the text.
pub fn ai_notice(examples: &[&str]) -> String {
    let examples = match examples {
        [] => "PERSON_1".to_string(),
        [one] => one.to_string(),
        [first, second, ..] => format!("{first} or {second}"),
    };
    format!(
        "Note: names and other identifying details in this document were replaced \
         with placeholders such as {examples}. Keep every placeholder exactly as \
         written, with the same capitals, underscore and number. Do not invent new \
         placeholders or guess the real values; they are restored locally after \
         your reply.\n\n---\n\n"
    )
}

/// Copy for AI's text: `text` after the note naming its first two distinct
/// `aliases` (in source order), or `text` alone when it has none.
pub fn for_ai<'a>(text: &str, aliases: impl IntoIterator<Item = &'a str>) -> String {
    let mut examples = Vec::new();
    for alias in aliases {
        if !examples.contains(&alias) {
            examples.push(alias);
        }
        if examples.len() == 2 {
            break;
        }
    }
    if examples.is_empty() {
        text.to_owned()
    } else {
        ai_notice(&examples) + text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_note_names_the_first_two_distinct_aliases() {
        let text = "PERSON_1 and PERSON_1 met ORG_1 and PERSON_2.";
        let copied = for_ai(text, ["PERSON_1", "PERSON_1", "ORG_1", "PERSON_2"]);
        assert!(copied.starts_with("Note: "));
        assert!(copied.contains("such as PERSON_1 or ORG_1."));
        assert!(copied.ends_with(&format!("---\n\n{text}")));
        assert_eq!(for_ai(text, []), text, "no alias, no note");
    }
}
