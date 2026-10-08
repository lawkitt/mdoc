use cloakrs_core::{Confidence, EntityType, Locale, PiiEntity, PromptSanitizer, Recognizer, Scanner, Span};
use presidio_analyzer::{AnalyzerEngine, RecognizerRegistry};
use presidio_anonymizer::{AnonymizerEngine, RecognizerResult};
use serde_json::json;
use std::collections::HashMap;

// Synthetic prelocated mentions isolate mapping behavior from detector quality.
struct Names;
impl Recognizer for Names {
    fn id(&self) -> &str { "synthetic_names_probe" }
    fn entity_type(&self) -> EntityType { EntityType::PersonName }
    fn supported_locales(&self) -> &[Locale] { &[] }
    fn scan(&self, source: &str) -> Vec<PiiEntity> {
        ["Анна Иванова", "Анны Ивановой"].iter().flat_map(|needle| {
            source.match_indices(needle).map(|(start, value)| PiiEntity {
                entity_type: EntityType::PersonName,
                span: Span::new(start, start + value.len()),
                text: value.into(),
                confidence: Confidence::new(0.99).unwrap(),
                recognizer_id: self.id().into(),
            })
        }).collect()
    }
}

fn main() {
    let engine = AnalyzerEngine::new();
    let ru = "😀 СНИЛС: 112-233-445 95; email anna@example.invalid; Анна Иванова";
    let found = engine.analyze(ru, "ru", None, None);
    assert!(found.iter().all(|r| ru.get(r.start..r.end).is_some()));
    assert!(found.iter().any(|r| r.entity_type == "RU_SNILS"));
    assert!(found.iter().any(|r| r.entity_type == "EMAIL_ADDRESS"));
    assert!(!found.iter().any(|r| r.entity_type == "PERSON"));
    let ru_registry = RecognizerRegistry::with_predefined("ru");
    assert_eq!(ru_registry.recognizers.len(), 0);
    let source = "😀 Анна Иванова / Анны Ивановой / Анна Иванова";
    let spans: Vec<_> = Names.scan(source).into_iter().map(|r|
        RecognizerResult::new("PERSON", r.span.start, r.span.end, 0.99)).collect();
    let out = AnonymizerEngine::new().anonymize(source, spans, &HashMap::new()).unwrap();
    assert_eq!(out.text, "😀 <PERSON> / <PERSON> / <PERSON>");
    assert!(out.items.iter().all(|r| out.text.get(r.start..r.end) == Some(r.text.as_str())));

    let sanitizer = PromptSanitizer::new(Scanner::builder().recognizer(Names).build().unwrap());
    let (clean, mapping) = sanitizer.sanitize(source).unwrap();
    assert_eq!(mapping.entries().count(), 2);
    let restored = mapping.restore_strict(&clean);
    assert_eq!(restored, source);
    let mut values: HashMap<&str, &str> = HashMap::new();
    for entry in mapping.entries() { values.insert(entry.original_value(), &entry.placeholder); }
    assert_ne!(values["Анна Иванова"], values["Анны Ивановой"]);

    let old_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let invalid = std::panic::catch_unwind(|| {
        AnonymizerEngine::new().anonymize("Я", vec![RecognizerResult::new("PERSON", 1, 2, 0.9)], &HashMap::new())
    });
    std::panic::set_hook(old_hook);
    assert!(invalid.is_err());
    println!("{}", serde_json::to_string_pretty(&json!({
        "presidio_default_engine_ru_results": found.iter().map(|r| json!({"type":r.entity_type,"start":r.start,"end":r.end,"text":&ru[r.start..r.end]})).collect::<Vec<_>>(),
        "presidio_ru_registry_size": ru_registry.recognizers.len(),
        "presidio_default_ner_person_found": false,
        "presidio_default_replace": out.text,
        "presidio_output_ranges_valid": true,
        "presidio_invalid_utf8_public_span_panics": invalid.is_err(),
        "cloakrs_aliases": clean,
        "cloakrs_unique_entries": mapping.entries().count(),
        "cloakrs_exact_repeat_reuses_alias": true,
        "cloakrs_inflection_remains_separate": true,
        "cloakrs_strict_unicode_round_trip": restored == source,
        "scope": "Synthetic mapping/registration/integrity probes, no model inference or corpus qualification"
    })).unwrap());
}
